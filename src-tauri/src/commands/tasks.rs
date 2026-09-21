//! 任务相关命令。
//!
//! 最需要理解的一点是**重复任务如何"完成"**：
//! 一条重复任务在库里只有一行，`due_at` 指向下一次出现时间。
//! 完成后不是插入新记录，而是把这一行的 `due_at` 推进到下一次，
//! 同时把完成事实记进 `completed_logs`（含推进前的 due_at，供撤销精确回滚）。
//! 因此列表查询不需要任何"已完成实例"的概念，比较时间即可。

use rusqlite::{params, Connection, Row};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::db::{now_ms, parse_i64_array, parse_str_array, to_json_array};
use crate::error::{AppError, Result};
use crate::models::{CompletedLog, Task, TaskInput, TaskQuery};
use crate::recurrence::{self, Rule};
use crate::state::AppState;

/// 任务完成后的返回体。带上 `next_at` 让前端能提示"下次什么时候"。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteResult {
    pub task: Option<Task>,
    pub log_id: i64,
    pub next_at: Option<i64>,
    pub recurring: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResult {
    pub next_at: Option<i64>,
    pub label: String,
}

// ─────────────────────────────── 行映射 ───────────────────────────────

pub fn row_to_task(r: &Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get("id")?,
        title: r.get("title")?,
        note: r.get("note")?,
        status: r.get("status")?,
        priority: r.get("priority")?,
        tags: parse_str_array(&r.get::<_, String>("tags")?),
        due_at: r.get("due_at")?,
        remind_at: r.get("remind_at")?,
        repeat_type: r.get("repeat_type")?,
        repeat_interval: r.get("repeat_interval")?,
        repeat_unit: r.get("repeat_unit")?,
        repeat_weekdays: parse_i64_array(&r.get::<_, String>("repeat_weekdays")?),
        repeat_monthday: r.get("repeat_monthday")?,
        repeat_end_at: r.get("repeat_end_at")?,
        repeat_cron: r.get("repeat_cron")?,
        parent_id: r.get("parent_id")?,
        pinned: r.get::<_, i64>("pinned")? != 0,
        sort_order: r.get("sort_order")?,
        created_at: r.get("created_at")?,
        updated_at: r.get("updated_at")?,
        completed_at: r.get("completed_at")?,
        deleted_at: r.get("deleted_at")?,
        remind_fired_at: r.get("remind_fired_at")?,
    })
}

/// 任务列清单。导出/备份等模块复用同一份，避免列顺序漂移导致映射错位。
pub const TASK_COLS: &str = "id, title, note, status, priority, tags, due_at, remind_at, repeat_type, \
     repeat_interval, repeat_unit, repeat_weekdays, repeat_monthday, repeat_end_at, repeat_cron, \
     parent_id, pinned, sort_order, created_at, updated_at, completed_at, deleted_at, remind_fired_at";

pub fn get_task(conn: &Connection, id: i64) -> Result<Task> {
    conn.query_row(&format!("SELECT {TASK_COLS} FROM tasks WHERE id = ?1"), params![id], row_to_task)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => AppError::other(format!("任务 {id} 不存在")),
            other => other.into(),
        })
}

// ─────────────────────────────── 查询 ───────────────────────────────

/// 构造任务列表查询语句。
///
/// 为什么单独抽出来：**视图过滤的语义全在这段 SQL 里**，而它出错的后果是静默的 ——
/// 过滤条件写错不会抛异常，只会让任务莫名消失或永远不再出现
/// （"打勾后从今日消失"、"明天重新出现"、"完成后能在已完成里找到"都靠它）。
/// 抽成纯函数后可以直接在内存库上断言这些行为。
pub fn build_task_query(q: &TaskQuery, now: i64) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
    let view = q.view.as_deref().unwrap_or("today");
    let mut wheres: Vec<String> = Vec::new();
    let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    match view {
        "trash" => wheres.push("deleted_at IS NOT NULL".into()),
        "done" => wheres.push("deleted_at IS NULL AND status = 'done'".into()),
        "all" => wheres.push("deleted_at IS NULL AND status = 'todo'".into()),
        "upcoming" => {
            wheres.push("deleted_at IS NULL AND status = 'todo' AND due_at IS NOT NULL AND due_at > ?".into());
            binds.push(Box::new(now));
        }
        // today：无到期时间的一律可见；有到期时间的只在到点后出现。
        //
        // "每天任务今天完成后明天再出现"就是靠这条规则自然成立的 ——
        // 完成后 due_at 被推进到明天，于是它今天不再满足 due_at <= now。
        // 不需要任何"已完成实例"的概念，比较时间就够了。
        _ => {
            wheres.push("deleted_at IS NULL AND status = 'todo' AND (due_at IS NULL OR due_at <= ?)".into());
            binds.push(Box::new(now));
        }
    }

    if let Some(s) = q.search.as_deref().filter(|s| !s.trim().is_empty()) {
        wheres.push("(title LIKE ? OR IFNULL(note,'') LIKE ?)".into());
        let like = format!("%{}%", s.trim());
        binds.push(Box::new(like.clone()));
        binds.push(Box::new(like));
    }
    if let Some(tag) = q.tag.as_deref().filter(|t| !t.is_empty()) {
        // tags 存成 JSON 数组字符串，用带引号的包含匹配，
        // 否则 "工作" 会命中 "工作台"
        wheres.push("tags LIKE ?".into());
        binds.push(Box::new(format!("%\"{tag}\"%")));
    }
    if let Some(p) = q.priority {
        wheres.push("priority = ?".into());
        binds.push(Box::new(p));
    }

    // 排序里一律把 pinned 放最前：置顶的意义就是无视其它排序规则
    let order = match q.sort.as_deref().unwrap_or("due") {
        "priority" => "pinned DESC, priority DESC, due_at IS NULL, due_at ASC, created_at DESC",
        "created" => "pinned DESC, created_at DESC",
        "manual" => "pinned DESC, sort_order ASC, created_at DESC",
        _ => "pinned DESC, due_at IS NULL, due_at ASC, priority DESC, created_at DESC",
    };

    (
        format!("SELECT {TASK_COLS} FROM tasks WHERE {} ORDER BY {order}", wheres.join(" AND ")),
        binds,
    )
}

/// 执行查询。与 `build_task_query` 分开，方便测试直接构造语句而不必跑一遍。
pub fn query_tasks(conn: &Connection, q: &TaskQuery, now: i64) -> Result<Vec<Task>> {
    let (sql, binds) = build_task_query(q, now);
    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), row_to_task)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[tauri::command]
pub fn list_tasks(state: State<'_, AppState>, query: Option<TaskQuery>) -> Result<Vec<Task>> {
    let q = query.unwrap_or_default();
    let now = now_ms();
    state.db()?.with(|conn| query_tasks(conn, &q, now))
}

// ─────────────────────────────── 增改 ───────────────────────────────

#[tauri::command]
pub fn create_task(state: State<'_, AppState>, input: TaskInput) -> Result<Task> {
    let title = input.title.trim().to_string();
    if title.is_empty() {
        return Err(AppError::other("标题不能为空"));
    }
    let db = state.db()?;
    let now = now_ms();
    // 默认提醒提前量来自设置：用户在设置里配"默认提前 N 分钟"，
    // 新建任务时若没显式给提醒时间，就按这个偏移从出现时间推算。
    let default_offset = state.settings().default_remind_offset_min;

    db.with_tx(|tx| {
        // sort_order 取当前最小值 - 1，新任务排在最前面
        let min_order: f64 = tx
            .query_row("SELECT IFNULL(MIN(sort_order), 0) FROM tasks WHERE deleted_at IS NULL", [], |r| r.get(0))
            .unwrap_or(0.0);

        tx.execute(
            "INSERT INTO tasks (title, note, status, priority, tags, due_at, remind_at, repeat_type,
                repeat_interval, repeat_unit, repeat_weekdays, repeat_monthday, repeat_end_at, repeat_cron,
                parent_id, pinned, sort_order, created_at, updated_at)
             VALUES (?1,?2,'todo',?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,0,?15,?16,?16)",
            params![
                title,
                input.note,
                input.priority.unwrap_or(2),
                to_json_array(&input.tags.clone().unwrap_or_default()),
                input.due_at,
                // 没显式给提醒时间时，按默认提前量从出现时间推算；
                // 偏移 <= 0 且没设过就表示"不提醒"
                input
                    .remind_at
                    .or_else(|| input.due_at.map(|d| d - default_offset.max(0) * 60_000))
                    .filter(|_| default_offset >= 0),
                input.repeat_type.clone().unwrap_or_else(|| "once".into()),
                input.repeat_interval.unwrap_or(1).max(1),
                input.repeat_unit.clone().unwrap_or_else(|| "day".into()),
                to_json_array(&input.repeat_weekdays.clone().unwrap_or_default()),
                input.repeat_monthday,
                input.repeat_end_at,
                input.repeat_cron,
                input.parent_id,
                min_order - 1.0,
                now,
            ],
        )?;
        let id = tx.last_insert_rowid();
        Ok(get_task(tx, id)?)
    })
}

#[tauri::command]
pub fn update_task(state: State<'_, AppState>, input: TaskInput) -> Result<Task> {
    let id = input.id.ok_or_else(|| AppError::other("缺少任务 id"))?;
    let title = input.title.trim().to_string();
    if title.is_empty() {
        return Err(AppError::other("标题不能为空"));
    }
    let db = state.db()?;
    let now = now_ms();

    db.with_tx(|tx| {
        let old = get_task(tx, id)?;
        tx.execute(
            "UPDATE tasks SET title=?2, note=?3, priority=?4, tags=?5, due_at=?6, remind_at=?7,
                repeat_type=?8, repeat_interval=?9, repeat_unit=?10, repeat_weekdays=?11,
                repeat_monthday=?12, repeat_end_at=?13, repeat_cron=?14, parent_id=?15, updated_at=?16
             WHERE id=?1",
            params![
                id,
                title,
                input.note,
                input.priority.unwrap_or(old.priority),
                to_json_array(&input.tags.clone().unwrap_or(old.tags.clone())),
                input.due_at.or(old.due_at),
                input.remind_at.or(old.remind_at),
                input.repeat_type.clone().unwrap_or(old.repeat_type.clone()),
                input.repeat_interval.unwrap_or(old.repeat_interval).max(1),
                input.repeat_unit.clone().unwrap_or(old.repeat_unit.clone()),
                to_json_array(&input.repeat_weekdays.clone().unwrap_or(old.repeat_weekdays.clone())),
                input.repeat_monthday.or(old.repeat_monthday),
                input.repeat_end_at.or(old.repeat_end_at),
                input.repeat_cron.clone().or(old.repeat_cron.clone()),
                input.parent_id.or(old.parent_id),
                now,
            ],
        )?;
        // 时间被改动后，之前发出的提醒应当允许重新触发
        tx.execute("UPDATE tasks SET remind_fired_at = NULL WHERE id = ?1", params![id])?;
        Ok(get_task(tx, id)?)
    })
}

// ─────────────────────────────── 完成 / 撤销 ───────────────────────────────

/// 把重复任务的 `due_at` 推进到下一次，并按同样偏移平移提醒时间。
///
/// "完成任务"与"跳过本次"共用这段逻辑，唯一区别是登记到 `recurrence_exceptions`
/// 的动作名（`done` / `skip`）—— 所以用 `exception_action` 参数化，
/// 而不是在两处各写一遍推进代码。
///
/// 返回下一次出现时间；序列已结束（到达 `repeat_end_at`）时返回 `None`，
/// 并把任务收尾成 done，避免它永远挂在列表里却不再出现。
pub fn advance_occurrence(
    conn: &Connection,
    task: &Task,
    rule: &Rule,
    now: i64,
    exception_action: Option<&str>,
) -> Result<Option<i64>> {
    if let (Some(due), Some(action)) = (task.due_at, exception_action) {
        let _ = conn.execute(
            "INSERT OR IGNORE INTO recurrence_exceptions (task_id, exception_date, action) VALUES (?1, ?2, ?3)",
            params![task.id, due, action],
        );
    }

    // 推进基准取 max(now, due_at)：逾期多天完成不会补出多条实例，
    // 提前完成（due_at 在未来）也不会把下一次拉回过去。
    let from = now.max(task.due_at.unwrap_or(now));
    let seed = task.due_at.unwrap_or(now);

    match recurrence::next_after(rule, from, seed) {
        Some(next_ms) => {
            // 提醒时间跟着出现时间平移，保持用户设定的提前量不变
            let new_remind = match (task.remind_at, task.due_at) {
                (Some(remind), Some(due)) => Some(next_ms + (remind - due)),
                (Some(_), None) => Some(next_ms),
                _ => None,
            };
            conn.execute(
                "UPDATE tasks SET due_at=?2, remind_at=?3, remind_fired_at=NULL, updated_at=?4 WHERE id=?1",
                params![task.id, next_ms, new_remind, now],
            )?;
            Ok(Some(next_ms))
        }
        None => {
            conn.execute(
                "UPDATE tasks SET status='done', completed_at=?2, updated_at=?2, remind_fired_at=NULL WHERE id=?1",
                params![task.id, now],
            )?;
            Ok(None)
        }
    }
}

/// 完成任务的核心逻辑，返回 `(日志 id, 下一次出现时间, 是否重复任务)`。
///
/// 抽成独立函数是为了能在内存库上直接测：这是需求里最核心的行为
/// （"打勾后从列表消失"、"重复任务明天再来"、"撤销回到原状"），
/// 而且它错了**只会静默出错** —— 任务永远不回来，或者永远不消失。
///
/// 参数用 `&Connection` 而不是 `&Transaction`：`Transaction` 实现了 `Deref<Target = Connection>`，
/// 所以生产代码传事务进来（保证原子性），测试直接传连接即可。
pub fn apply_completion(conn: &Connection, id: i64, now: i64) -> Result<(i64, Option<i64>, bool)> {
    let task = get_task(conn, id)?;
    let rule = Rule::from_task(&task);
    let recurring = rule.is_repeating();

    conn.execute(
        "INSERT INTO completed_logs (task_id, task_title, completed_at, action, prev_due_at)
         VALUES (?1, ?2, ?3, 'complete', ?4)",
        params![id, task.title, now, task.due_at],
    )?;
    let log_id = conn.last_insert_rowid();

    if !recurring {
        // 一次性任务：状态转 done，永久留在"已完成"里
        conn.execute(
            "UPDATE tasks SET status='done', completed_at=?2, updated_at=?2, remind_fired_at=NULL WHERE id=?1",
            params![id, now],
        )?;
        return Ok((log_id, None, false));
    }

    let next = advance_occurrence(conn, &task, &rule, now, Some("done"))?;
    Ok((log_id, next, true))
}

/// 撤销完成的核心逻辑，返回任务 id。
///
/// 对重复任务而言是"把 due_at 写回旧值"，而不是删掉整条序列 ——
/// 旧值记在 `completed_logs.prev_due_at` 里，所以回滚是精确的。
pub fn apply_uncompletion(conn: &Connection, log_id: i64, now: i64) -> Result<i64> {
    let (task_id, prev_due): (i64, Option<i64>) = conn.query_row(
        "SELECT task_id, prev_due_at FROM completed_logs WHERE id = ?1",
        params![log_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    if let Some(due) = prev_due {
        conn.execute(
            "UPDATE tasks SET due_at=?2, status='todo', completed_at=NULL, remind_fired_at=NULL, updated_at=?3 WHERE id=?1",
            params![task_id, due, now],
        )?;
        conn.execute(
            "DELETE FROM recurrence_exceptions WHERE task_id=?1 AND exception_date=?2 AND action='done'",
            params![task_id, due],
        )?;
    } else {
        conn.execute(
            "UPDATE tasks SET status='todo', completed_at=NULL, updated_at=?2 WHERE id=?1",
            params![task_id, now],
        )?;
    }

    // 记一笔撤销，历史里能看到完整轨迹
    conn.execute(
        "INSERT INTO completed_logs (task_id, task_title, completed_at, action, prev_due_at)
         SELECT id, title, ?2, 'uncomplete', NULL FROM tasks WHERE id=?1",
        params![task_id, now],
    )?;
    Ok(task_id)
}

#[tauri::command]
pub fn complete_task(app: AppHandle, state: State<'_, AppState>, id: i64) -> Result<CompleteResult> {
    let db = state.db()?;
    let now = now_ms();

    // 放在事务里：完成记录、例外登记、due_at 推进必须一起成功或一起失败
    let (log_id, next_at, recurring) = db.with_tx(|tx| apply_completion(tx, id, now))?;

    let task = db.with(|c| get_task(c, id)).ok();
    let _ = app.emit("glassnote://tasks-changed", ());
    Ok(CompleteResult { task, log_id, next_at, recurring })
}

/// 撤销完成。对重复任务而言是"把 due_at 写回旧值"，而不是删掉整条序列。
#[tauri::command]
pub fn uncomplete_task(state: State<'_, AppState>, log_id: i64) -> Result<Option<Task>> {
    let db = state.db()?;
    let now = now_ms();
    let task_id = db.with_tx(|tx| apply_uncompletion(tx, log_id, now))?;
    Ok(db.with(|c| get_task(c, task_id)).ok())
}

/// 跳过本次出现（重复任务专用）：把 due_at 推进到下一次，但**不记完成**。
///
/// 与"完成"共用 `advance_occurrence`，区别只是登记的动作名是 `skip`。
/// 这样"跳过"不会在已完成列表里留下记录 —— 它本来就没被做。
#[tauri::command]
pub fn skip_task_occurrence(state: State<'_, AppState>, id: i64) -> Result<Option<Task>> {
    let db = state.db()?;
    let now = now_ms();

    db.with_tx(|tx| {
        let task = get_task(tx, id)?;
        let rule = Rule::from_task(&task);
        if !rule.is_repeating() {
            return Err(AppError::other("只有重复任务可以跳过本次"));
        }
        advance_occurrence(tx, &task, &rule, now, Some("skip"))?;
        Ok(())
    })?;

    Ok(db.with(|c| get_task(c, id)).ok())
}

/// 稍后提醒：只挪提醒时间，不动出现时间。
#[tauri::command]
pub fn snooze_task(state: State<'_, AppState>, id: i64, minutes: i64) -> Result<Task> {
    let db = state.db()?;
    let when = now_ms() + minutes.clamp(1, 60 * 24 * 30) * 60_000;
    db.with(|c| {
        c.execute(
            "UPDATE tasks SET remind_at=?2, remind_fired_at=NULL, updated_at=?3 WHERE id=?1",
            params![id, when, now_ms()],
        )?;
        get_task(c, id)
    })
}

// ─────────────────────────────── 删除 / 恢复 ───────────────────────────────

#[tauri::command]
pub fn delete_task(state: State<'_, AppState>, id: i64) -> Result<()> {
    let db = state.db()?;
    db.with(|c| {
        // 软删除进回收站，30 天后由 run_maintenance 清理
        c.execute(
            "UPDATE tasks SET deleted_at=?2, updated_at=?2 WHERE id=?1",
            params![id, now_ms()],
        )?;
        Ok(())
    })
}

#[tauri::command]
pub fn restore_task(state: State<'_, AppState>, id: i64) -> Result<()> {
    let db = state.db()?;
    db.with(|c| {
        c.execute("UPDATE tasks SET deleted_at=NULL, updated_at=?2 WHERE id=?1", params![id, now_ms()])?;
        Ok(())
    })
}

/// 永久删除。子任务通过外键 ON DELETE CASCADE 一并清掉。
#[tauri::command]
pub fn purge_task(state: State<'_, AppState>, id: i64) -> Result<()> {
    let db = state.db()?;
    db.with(|c| {
        c.execute("DELETE FROM tasks WHERE id=?1", params![id])?;
        Ok(())
    })
}

#[tauri::command]
pub fn empty_trash(state: State<'_, AppState>) -> Result<usize> {
    let db = state.db()?;
    db.with(|c| {
        let n = c.execute("DELETE FROM tasks WHERE deleted_at IS NOT NULL", [])?;
        Ok(n)
    })
}

// ─────────────────────────────── 排序 / 置顶 ───────────────────────────────

#[tauri::command]
pub fn reorder_tasks(state: State<'_, AppState>, ids: Vec<i64>) -> Result<()> {
    let db = state.db()?;
    db.with_tx(|tx| {
        let mut stmt = tx.prepare("UPDATE tasks SET sort_order=?2, updated_at=?3 WHERE id=?1")?;
        let now = now_ms();
        for (i, id) in ids.iter().enumerate() {
            stmt.execute(params![id, i as f64, now])?;
        }
        Ok(())
    })
}

#[tauri::command]
pub fn set_task_pinned(state: State<'_, AppState>, id: i64, pinned: bool) -> Result<()> {
    let db = state.db()?;
    db.with(|c| {
        c.execute(
            "UPDATE tasks SET pinned=?2, updated_at=?3 WHERE id=?1",
            params![id, if pinned { 1 } else { 0 }, now_ms()],
        )?;
        Ok(())
    })
}

// ─────────────────────────────── 历史 ───────────────────────────────

#[tauri::command]
pub fn list_completed_logs(state: State<'_, AppState>, limit: Option<i64>) -> Result<Vec<CompletedLog>> {
    let db = state.db()?;
    let limit = limit.unwrap_or(200).clamp(1, 2000);
    db.with(|c| {
        let mut stmt = c.prepare(
            "SELECT id, task_id, task_title, completed_at, action, prev_due_at
             FROM completed_logs ORDER BY completed_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |r| {
            Ok(CompletedLog {
                id: r.get(0)?,
                task_id: r.get(1)?,
                task_title: r.get(2)?,
                completed_at: r.get(3)?,
                action: r.get(4)?,
                prev_due_at: r.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
}

/// 预览下一次出现时间。
///
/// 由 Rust 计算而不是前端重算：重复规则语义（短月收敛、逾期不补实例、
/// 工作日跳过周末）只应该存在一份实现，否则前后端必然漂移。
#[tauri::command]
pub fn preview_next_occurrence(input: TaskInput) -> Result<PreviewResult> {
    let rule = Rule {
        repeat_type: input.repeat_type.clone().unwrap_or_else(|| "once".into()),
        interval: input.repeat_interval.unwrap_or(1).max(1),
        unit: input.repeat_unit.clone().unwrap_or_else(|| "day".into()),
        weekdays: input
            .repeat_weekdays
            .clone()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|d| u32::try_from(d).ok())
            .collect(),
        monthday: input.repeat_monthday.and_then(|d| i32::try_from(d).ok()),
        end_at: input.repeat_end_at,
        cron: input.repeat_cron.clone(),
    };

    let now = now_ms();
    // 以"当前时间"为基准预览，用户想看到的是"下一次什么时候来"
    let seed = input.due_at.unwrap_or(now);
    let from = now.max(seed);
    let next = recurrence::next_after(&rule, from, seed);
    Ok(PreviewResult { next_at: next, label: recurrence::describe(&rule) })
}

/// 供调度器复用：查出所有到点但还没提醒过的任务。
pub fn due_reminders(conn: &Connection, now: i64) -> Result<Vec<(i64, String, Option<String>, Option<i64>)>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, note, due_at FROM tasks
         WHERE deleted_at IS NULL AND status='todo'
           AND remind_at IS NOT NULL AND remind_at <= ?1
           AND (remind_fired_at IS NULL OR remind_fired_at < remind_at)",
    )?;
    let rows = stmt.query_map(params![now], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// 供调度器复用：标记提醒已发出，避免同一实例重复通知。
pub fn mark_reminded(conn: &Connection, id: i64, at: i64) -> Result<()> {
    conn.execute("UPDATE tasks SET remind_fired_at=?2 WHERE id=?1", params![id, at])?;
    Ok(())
}

/// 供调度器复用：下一次有意义的唤醒时间。
///
/// 返回 None 表示"当前没有任何待触发的东西"，调度器可以睡到更久以后。
pub fn next_wake_hint(conn: &Connection, now: i64) -> Result<Option<i64>> {
    let next_remind: Option<i64> = conn.query_row(
        "SELECT MIN(remind_at) FROM tasks
         WHERE deleted_at IS NULL AND status='todo' AND remind_at IS NOT NULL AND remind_at > ?1",
        params![now],
        |r| r.get(0),
    )?;
    let next_due: Option<i64> = conn.query_row(
        "SELECT MIN(due_at) FROM tasks
         WHERE deleted_at IS NULL AND status='todo' AND due_at IS NOT NULL AND due_at > ?1",
        params![now],
        |r| r.get(0),
    )?;
    Ok(match (next_remind, next_due) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    })
}


#[cfg(test)]
mod tests {
    //! 为什么这里保留精确断言：
    //!
    //! 这些测试覆盖的是**验收标准本身**，而且全部属于"错了不会报错"的一类 ——
    //! 视图过滤条件写错、完成推进算错，都不会抛异常，表现只是
    //! "任务打勾后没有消失"、"重复任务再也不回来"、"撤销后回不到原位"。
    //! 这类静默错误只能靠精确断言兜住。
    //!
    //! 断言的是**用户可见的行为**（今日视图里有没有这条任务、due_at 变成什么），
    //! 不是实现细节：把 SQL 换成别种写法、把推进逻辑重写，这些断言依然成立。
    //!
    //! 直接跑在内存 SQLite 上，用与生产完全相同的 schema（`db::schema::migrate`），
    //! 所以索引、约束、默认值都是真实的。

    use super::*;
    use crate::db::schema;
    use chrono::{Local, NaiveDate, TimeZone};

    fn mem_db() -> Connection {
        let c = Connection::open_in_memory().expect("内存库");
        schema::migrate(&c).expect("建表");
        c
    }

    fn at(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> i64 {
        Local
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(hh, mm, 0).unwrap(),
            )
            .earliest()
            .unwrap()
            .timestamp_millis()
    }

    fn insert(c: &Connection, title: &str, due: Option<i64>, repeat: &str) -> i64 {
        c.execute(
            "INSERT INTO tasks (title, status, priority, tags, due_at, repeat_type,
                repeat_interval, repeat_unit, repeat_weekdays, sort_order, created_at, updated_at)
             VALUES (?1, 'todo', 2, '[]', ?2, ?3, 1, 'day', '[]', 0, 0, 0)",
            params![title, due, repeat],
        )
        .expect("插入任务");
        c.last_insert_rowid()
    }

    fn view(v: &str) -> TaskQuery {
        TaskQuery { view: Some(v.into()), ..Default::default() }
    }

    fn titles(c: &Connection, q: &TaskQuery, now: i64) -> Vec<String> {
        query_tasks(c, q, now).unwrap().into_iter().map(|t| t.title).collect()
    }

    // ── 核心：打勾后消失、次日回来 ──────────────────────────────────────

    #[test]
    fn completing_a_daily_task_hides_it_today_and_restores_it_tomorrow() {
        let c = mem_db();
        let now = at(2026, 3, 10, 20, 0); // 晚上 8 点
        let due = at(2026, 3, 10, 9, 0); // 今天 9 点该做
        let id = insert(&c, "每日站会", Some(due), "daily");

        assert_eq!(titles(&c, &view("today"), now), vec!["每日站会"]);

        let (_, next, recurring) = apply_completion(&c, id, now).unwrap();
        assert!(recurring, "daily 应被识别为重复任务");
        assert_eq!(next, Some(at(2026, 3, 11, 9, 0)), "下一次应是明天同一时刻");

        // 关键断言：完成后今天不再出现
        assert!(titles(&c, &view("today"), now).is_empty(), "完成后应从今日列表消失");
        // 但它并没有被"完成掉"——只是下一次还没到，所以"全部"里仍在
        assert_eq!(titles(&c, &view("all"), now), vec!["每日站会"]);
        // 也不该出现在已完成里（序列还在继续）
        assert!(titles(&c, &view("done"), now).is_empty());

        // 次日到点后自动回来
        assert!(titles(&c, &view("today"), at(2026, 3, 11, 8, 0)).is_empty(), "次日到点前不应出现");
        assert_eq!(
            titles(&c, &view("today"), at(2026, 3, 11, 9, 30)),
            vec!["每日站会"],
            "次日到点后应重新出现"
        );
    }

    #[test]
    fn completing_a_one_off_moves_it_to_done_permanently() {
        let c = mem_db();
        let now = at(2026, 3, 10, 20, 0);
        let id = insert(&c, "交周报", Some(at(2026, 3, 10, 18, 0)), "once");

        let (_, next, recurring) = apply_completion(&c, id, now).unwrap();
        assert!(!recurring);
        assert_eq!(next, None, "一次性任务没有下一次");

        assert!(titles(&c, &view("today"), now).is_empty());
        assert!(titles(&c, &view("all"), now).is_empty());
        assert_eq!(titles(&c, &view("done"), now), vec!["交周报"]);
    }

    #[test]
    fn a_task_without_due_date_stays_visible_in_today() {
        let c = mem_db();
        let now = at(2026, 3, 10, 20, 0);
        insert(&c, "随手记", None, "once");
        assert_eq!(titles(&c, &view("today"), now), vec!["随手记"], "没有时间限制的任务始终可见");
    }

    #[test]
    fn overdue_tasks_stay_visible() {
        let c = mem_db();
        let now = at(2026, 3, 10, 20, 0);
        insert(&c, "逾期三天", Some(at(2026, 3, 7, 9, 0)), "once");
        assert_eq!(titles(&c, &view("today"), now), vec!["逾期三天"]);
    }

    #[test]
    fn future_tasks_are_upcoming_not_today() {
        let c = mem_db();
        let now = at(2026, 3, 10, 20, 0);
        insert(&c, "下周开会", Some(at(2026, 3, 17, 10, 0)), "once");
        assert!(titles(&c, &view("today"), now).is_empty());
        assert_eq!(titles(&c, &view("upcoming"), now), vec!["下周开会"]);
    }

    // ── 撤销 ────────────────────────────────────────────────────────────

    #[test]
    fn undo_restores_a_recurring_task_to_its_previous_due_date() {
        let c = mem_db();
        let now = at(2026, 3, 10, 20, 0);
        let due = at(2026, 3, 10, 9, 0);
        let id = insert(&c, "每日站会", Some(due), "daily");

        let (log_id, _, _) = apply_completion(&c, id, now).unwrap();
        assert!(titles(&c, &view("today"), now).is_empty());

        apply_uncompletion(&c, log_id, now).unwrap();

        assert_eq!(get_task(&c, id).unwrap().due_at, Some(due), "due_at 应精确回到完成前的值");
        assert_eq!(titles(&c, &view("today"), now), vec!["每日站会"], "撤销后应重新出现在今日");
        // 完成时登记的例外要被清掉，否则同一次实例会被认为"已完成过"
        let exc: i64 = c
            .query_row("SELECT COUNT(*) FROM recurrence_exceptions WHERE task_id=?1 AND action='done'", params![id], |r| r.get(0))
            .unwrap();
        assert_eq!(exc, 0, "撤销应清除完成例外");
    }

    #[test]
    fn undo_of_a_one_off_returns_it_to_todo() {
        let c = mem_db();
        let now = at(2026, 3, 10, 20, 0);
        let id = insert(&c, "交周报", Some(at(2026, 3, 10, 18, 0)), "once");
        let (log_id, _, _) = apply_completion(&c, id, now).unwrap();

        apply_uncompletion(&c, log_id, now).unwrap();
        assert_eq!(get_task(&c, id).unwrap().status, "todo");
        assert_eq!(titles(&c, &view("today"), now), vec!["交周报"]);
    }

    // ── 跳过 / 结束日期 / 提醒平移 ───────────────────────────────────────

    #[test]
    fn skipping_advances_without_recording_a_completion() {
        let c = mem_db();
        let now = at(2026, 3, 10, 20, 0);
        let due = at(2026, 3, 10, 9, 0);
        let id = insert(&c, "每日站会", Some(due), "daily");

        let task = get_task(&c, id).unwrap();
        let rule = Rule::from_task(&task);
        advance_occurrence(&c, &task, &rule, now, Some("skip")).unwrap();

        assert_eq!(get_task(&c, id).unwrap().due_at, Some(at(2026, 3, 11, 9, 0)));
        let logs: i64 = c.query_row("SELECT COUNT(*) FROM completed_logs", [], |r| r.get(0)).unwrap();
        assert_eq!(logs, 0, "跳过不等于完成，不应留下完成记录");
        let action: String = c
            .query_row("SELECT action FROM recurrence_exceptions WHERE task_id=?1", params![id], |r| r.get(0))
            .unwrap();
        assert_eq!(action, "skip");
    }

    #[test]
    fn repeat_end_date_finishes_the_series_instead_of_hanging_forever() {
        let c = mem_db();
        let now = at(2026, 3, 10, 20, 0);
        let id = insert(&c, "限时活动", Some(at(2026, 3, 10, 9, 0)), "daily");
        c.execute("UPDATE tasks SET repeat_end_at=?2 WHERE id=?1", params![id, at(2026, 3, 11, 0, 0)])
            .unwrap();

        let (_, next, recurring) = apply_completion(&c, id, now).unwrap();
        assert!(recurring);
        assert_eq!(next, None, "结束日期之后不应再产生实例");
        // 必须收尾成 done，否则它会永远挂在"全部"里却再也不出现
        assert_eq!(titles(&c, &view("done"), now), vec!["限时活动"]);
        assert!(titles(&c, &view("all"), now).is_empty());
    }

    #[test]
    fn reminder_offset_survives_the_occurrence_advance() {
        let c = mem_db();
        let now = at(2026, 3, 10, 20, 0);
        let due = at(2026, 3, 10, 9, 0);
        let id = insert(&c, "每日站会", Some(due), "daily");
        // 用户设的是"提前 30 分钟"
        c.execute("UPDATE tasks SET remind_at=?2 WHERE id=?1", params![id, due - 30 * 60_000]).unwrap();

        apply_completion(&c, id, now).unwrap();

        let t = get_task(&c, id).unwrap();
        assert_eq!(t.due_at, Some(at(2026, 3, 11, 9, 0)));
        assert_eq!(t.remind_at, Some(at(2026, 3, 11, 8, 30)), "提前量应保持不变，而不是被重置");
        assert_eq!(t.remind_fired_at, None, "新实例的提醒应可再次触发");
    }

    #[test]
    fn completing_an_overdue_daily_task_does_not_create_a_backlog() {
        let c = mem_db();
        let due = at(2026, 1, 1, 8, 30);
        let now = at(2026, 4, 1, 20, 0); // 逾期三个月才完成
        let id = insert(&c, "每日站会", Some(due), "daily");

        let (_, next, _) = apply_completion(&c, id, now).unwrap();
        assert_eq!(next, Some(at(2026, 4, 2, 8, 30)), "只推进到下一次，不补出中间缺失的实例");
        let count: i64 = c.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1, "整条序列始终只有一行");
    }

    // ── 回收站 / 筛选 / 排序 ────────────────────────────────────────────

    #[test]
    fn soft_delete_moves_to_trash_and_restore_brings_it_back() {
        let c = mem_db();
        let now = at(2026, 3, 10, 20, 0);
        let id = insert(&c, "买牛奶", None, "once");
        assert_eq!(titles(&c, &view("today"), now), vec!["买牛奶"]);

        c.execute("UPDATE tasks SET deleted_at=?2 WHERE id=?1", params![id, now]).unwrap();
        assert!(titles(&c, &view("today"), now).is_empty(), "删除后不应出现在今日");
        assert_eq!(titles(&c, &view("trash"), now), vec!["买牛奶"]);

        c.execute("UPDATE tasks SET deleted_at=NULL WHERE id=?1", params![id]).unwrap();
        assert_eq!(titles(&c, &view("today"), now), vec!["买牛奶"], "恢复后应回到列表");
    }

    #[test]
    fn search_matches_both_title_and_note() {
        let c = mem_db();
        let now = at(2026, 3, 10, 20, 0);
        insert(&c, "写周报", None, "once");
        let with_note = insert(&c, "整理文件", None, "once");
        insert(&c, "买牛奶", None, "once");
        c.execute("UPDATE tasks SET note='周报的附件' WHERE id=?1", params![with_note]).unwrap();

        let q = TaskQuery { view: Some("today".into()), search: Some("周报".into()), ..Default::default() };
        let mut found = titles(&c, &q, now);
        found.sort();
        assert_eq!(found, vec!["写周报", "整理文件"], "标题与备注都应命中");
    }

    #[test]
    fn tag_filter_does_not_match_a_substring_of_another_tag() {
        let c = mem_db();
        let now = at(2026, 3, 10, 20, 0);
        let a = insert(&c, "任务A", None, "once");
        let b = insert(&c, "任务B", None, "once");
        c.execute("UPDATE tasks SET tags='[\"工作\"]' WHERE id=?1", params![a]).unwrap();
        c.execute("UPDATE tasks SET tags='[\"工作台\"]' WHERE id=?1", params![b]).unwrap();

        let q = TaskQuery { view: Some("today".into()), tag: Some("工作".into()), ..Default::default() };
        assert_eq!(titles(&c, &q, now), vec!["任务A"], "带引号匹配，不应命中「工作台」");
    }

    #[test]
    fn pinned_tasks_sort_first_regardless_of_due_time() {
        let c = mem_db();
        let now = at(2026, 3, 10, 20, 0);
        insert(&c, "普通", Some(at(2026, 3, 10, 8, 0)), "once");
        let pinned = insert(&c, "置顶", Some(at(2026, 3, 10, 18, 0)), "once");
        c.execute("UPDATE tasks SET pinned=1 WHERE id=?1", params![pinned]).unwrap();

        assert_eq!(titles(&c, &view("today"), now), vec!["置顶", "普通"]);
    }

    #[test]
    fn due_sort_puts_unscheduled_tasks_last() {
        let c = mem_db();
        let now = at(2026, 3, 10, 20, 0);
        insert(&c, "无时间", None, "once");
        insert(&c, "有时间", Some(at(2026, 3, 10, 9, 0)), "once");

        assert_eq!(titles(&c, &view("today"), now), vec!["有时间", "无时间"]);
    }
}
