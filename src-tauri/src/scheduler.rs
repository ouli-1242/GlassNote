//! 调度器：整个应用**唯一**的定时源。
//!
//! ## 为什么只有一个线程、一个 tick
//!
//! 需求里"空闲 CPU < 0.3%"这一条，最容易被破坏的方式就是到处 setTimeout：
//! 每个定时器都是一次周期性唤醒，几十个加起来就是可观测的 CPU 占用。
//! 所以这里把提醒、重复任务复活、列表刷新、定期维护全部收进**一个**线程，
//! 而且它大部分时间在 `sleep`，不占 CPU。
//!
//! ## 为什么睡眠时长是算出来的，不是固定轮询
//!
//! 每轮先问数据库"下一次有意义的时间点是什么"（最近的提醒或出现时间），
//! 然后睡到那个时刻 —— 没有任何待办时就直接睡满上限。
//! 上限 60 秒的作用是兜住系统休眠/唤醒、时区变更、系统时间被改这些情况：
//! 睡太久会让这些场景下的计算基准过时。
//!
//! 前端不参与任何计时，只被动接收事件。

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::commands::{data, tasks};
use crate::db::now_ms;
use crate::settings::in_dnd;
use crate::state::AppState;

/// 单次睡眠上限。即使没有任何待办也要定期醒一次，用于校准系统时间变化。
const MAX_TICK: Duration = Duration::from_secs(60);

/// 维护任务（备份、清理回收站）的执行间隔
const MAINTENANCE_EVERY_MS: i64 = 6 * 3600 * 1000;

/// 超过这个时长的提醒不再补发。
///
/// 关掉应用一整晚再打开时，昨晚的提醒已经没有意义了 ——
/// 补发一堆过期通知比不提醒更烦人。这里只记录一条日志。
const STALE_REMINDER_MS: i64 = 12 * 3600 * 1000;

pub fn spawn(app: AppHandle) {
    std::thread::Builder::new()
        .name("glassnote-scheduler".into())
        .spawn(move || {
            // 上一轮 tick 的时刻，用于判断"这段时间里有没有任务到点"
            let mut last_tick = now_ms();
            let mut last_maintenance = 0i64;

            loop {
                if app.state::<AppState>().is_quitting() {
                    break;
                }

                let now = now_ms();
                let wait = next_wait(&app, now);
                std::thread::sleep(wait);

                if app.state::<AppState>().is_quitting() {
                    break;
                }

                let now = now_ms();
                if let Err(e) = tick(&app, last_tick, now) {
                    // 调度器不能因为单次失败就退出，否则提醒功能会永久失效
                    eprintln!("[glassnote] 调度 tick 失败：{e}");
                }
                last_tick = now;

                if now - last_maintenance > MAINTENANCE_EVERY_MS {
                    last_maintenance = now;
                    if let Err(e) = data::maintenance_inner(&app) {
                        eprintln!("[glassnote] 定期维护失败：{e}");
                    }
                }
            }
        })
        .expect("无法创建调度线程");
}

/// 算出这一轮该睡多久。
fn next_wait(app: &AppHandle, now: i64) -> Duration {
    let state = app.state::<AppState>();

    // 数据库还没初始化（开机自启的延迟期内）就先按上限睡，等延迟结束再说
    let Ok(db) = state.db() else {
        return MAX_TICK;
    };

    let hint = db
        .with(|c| tasks::next_wake_hint(c, now))
        .ok()
        .flatten();

    match hint {
        Some(at) if at > now => {
            let delta = (at - now) as u64;
            // 至少睡 1 秒：若某个时间点正好是"现在"，直接睡 0 会变成忙循环
            Duration::from_millis(delta.clamp(1_000, MAX_TICK.as_millis() as u64))
        }
        _ => MAX_TICK,
    }
}

fn tick(app: &AppHandle, since: i64, now: i64) -> crate::error::Result<()> {
    let state = app.state::<AppState>();
    let db = state.db()?;
    let settings_now = state.settings();

    // ── 1) 到点提醒 ──
    let due = db.with(|c| tasks::due_reminders(c, now))?;
    for (id, title, note, due_at) in due {
        db.with(|c| tasks::mark_reminded(c, id, now))?;

        // 关掉应用很久之后的过期提醒直接跳过，只留一条日志
        if let Some(at) = due_at {
            if now - at > STALE_REMINDER_MS {
                eprintln!("[glassnote] 跳过过期提醒：{title}（{at}）");
                continue;
            }
        }

        if settings_now.notifications_enabled && !in_dnd(&settings_now, local_minutes(now)) {
            let body = note
                .clone()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| "任务到时间了".to_string());
            if let Err(e) = app
                .notification()
                .builder()
                .title(&title)
                .body(&body)
                .show()
            {
                eprintln!("[glassnote] 发送通知失败：{e}");
            }
        }

        // 应用内也推一条，点通知聚焦窗口时前端能定位到这条任务
        let _ = app.emit(
            "glassnote://reminder",
            serde_json::json!({ "taskId": id, "title": title, "note": note, "dueAt": due_at }),
        );
    }

    // ── 2) 有任务在这段时间里"到点出现" → 让前端刷新列表 ──
    //
    // 每日任务完成后 due_at 被推到明天，明天到点时它就重新满足 due_at <= now，
    // 于是"次日重新出现"这件事就是靠这里触发的一次刷新完成的。
    let appeared: i64 = db.with(|c| {
        Ok(c.query_row(
            "SELECT COUNT(*) FROM tasks
             WHERE deleted_at IS NULL AND status='todo' AND due_at IS NOT NULL
               AND due_at > ?1 AND due_at <= ?2",
            rusqlite::params![since, now],
            |r| r.get(0),
        )?)
    })?;
    if appeared > 0 {
        let _ = app.emit("glassnote://tasks-changed", ());
    }

    Ok(())
}

fn local_minutes(ms: i64) -> i64 {
    use chrono::{Local, Timelike};
    match chrono::DateTime::from_timestamp_millis(ms) {
        Some(dt) => {
            let l = dt.with_timezone(&Local);
            l.hour() as i64 * 60 + l.minute() as i64
        }
        None => 0,
    }
}
