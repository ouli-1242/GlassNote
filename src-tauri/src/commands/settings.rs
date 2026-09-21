//! 设置、开机自启、快捷键、隐私锁相关命令。

use serde_json::json;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt as AutostartExt;

use crate::db::now_ms;
use crate::error::{AppError, Result};
use crate::settings::{self, AppSettings, BootPayload};
use crate::shortcuts;
use crate::state::AppState;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppSettings> {
    state.ensure_db()?;
    Ok(state.settings())
}

/// 只更新 patch 里出现的字段。返回合并后的完整设置。
#[tauri::command]
pub fn set_settings(app: AppHandle, state: State<'_, AppState>, patch: serde_json::Value) -> Result<AppSettings> {
    let db = state.db()?;

    // 先落库，再更新内存缓存，最后写引导缓存 —— 顺序保证任何一步失败时
    // 内存里的值与磁盘一致，不会出现"看起来生效了但重启就丢"
    let merged = db.with(|c| settings::save_patch(c, &patch))?;
    state.set_settings(merged.clone());

    // 外观相关的几项要同步进引导缓存，供下次启动时首帧使用
    let boot = BootPayload::from_settings(&merged, state.paths.portable, state.autostart_run);
    if let Err(e) = settings::store_boot(&app, &boot) {
        eprintln!("[glassnote] 引导缓存写入失败（不影响功能）：{e}");
    }

    // 托盘可见性可能被改了
    crate::tray::sync_visibility(&app, merged.show_tray_icon);

    Ok(merged)
}

#[tauri::command]
pub fn get_boot_payload(state: State<'_, AppState>) -> Result<BootPayload> {
    Ok(BootPayload::from_settings(&state.settings(), state.paths.portable, state.autostart_run))
}

#[tauri::command]
pub fn reset_settings(app: AppHandle, state: State<'_, AppState>) -> Result<AppSettings> {
    let db = state.db()?;
    let fresh = AppSettings::default();
    db.with(|c| settings::write_all(c, &fresh))?;
    state.set_settings(fresh.clone());
    let boot = BootPayload::from_settings(&fresh, state.paths.portable, state.autostart_run);
    let _ = settings::store_boot(&app, &boot);
    Ok(fresh)
}

// ─────────────────────────────── 开机自启 ───────────────────────────────

/// 开关开机自启。
///
/// 注册时带 `--autostart` 参数：应用启动后据此判断"这次是被开机拉起的"，
/// 从而不创建窗口、静默驻留托盘，并在初始化前等待用户设定的延迟秒数。
#[tauri::command]
pub fn set_autostart(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> Result<bool> {
    let manager = app.autolaunch();
    if enabled {
        manager
            .enable()
            .map_err(|e| AppError::other(format!("启用开机自启失败：{e}")))?;
    } else {
        manager
            .disable()
            .map_err(|e| AppError::other(format!("关闭开机自启失败：{e}")))?;
    }
    // 把真实状态回读一次再写进设置，避免"以为开了其实没开"
    let actual = manager.is_enabled().unwrap_or(enabled);
    let db = state.db()?;
    let merged = db.with(|c| settings::save_patch(c, &json!({ "autostart": actual })))?;
    state.set_settings(merged);
    Ok(actual)
}

#[tauri::command]
pub fn is_autostart_enabled(app: AppHandle) -> Result<bool> {
    Ok(app.autolaunch().is_enabled().unwrap_or(false))
}

// ─────────────────────────────── 全局快捷键 ───────────────────────────────

/// 重新注册全局快捷键，返回注册失败的项（成功时为空数组）。
///
/// 不返回 Err 而是返回失败列表：两个快捷键可能只冲突其中一个，
/// 此时应该让另一个照常生效，而不是整体失败。
#[tauri::command]
pub fn set_shortcuts(
    app: AppHandle,
    state: State<'_, AppState>,
    capture: String,
    toggle: String,
) -> Result<Vec<String>> {
    let failures = shortcuts::reregister(&app, &capture, &toggle);
    let db = state.db()?;
    let merged = db.with(|c| settings::save_patch(c, &json!({
        "shortcutCapture": capture,
        "shortcutToggle": toggle,
    })))?;
    state.set_settings(merged);
    Ok(failures)
}

// ─────────────────────────────── 隐私锁 ───────────────────────────────

/// PIN 加盐哈希后存进 settings 表。
///
/// 为什么不放进 `AppSettings` 结构体：那个结构体会被整体序列化发给前端，
/// 把哈希放进去等于把口令派生物送到渲染进程，没有任何理由这么做。
/// 所以这里绕过 `save_patch` 的字段白名单，直接读写这一行。
///
/// 说明局限：4-8 位纯数字 PIN 的熵很低，本地哈希挡不住拿到文件后的暴力破解。
/// 它的实际作用是**防止他人随手打开窗口看到内容**，不是加密存储 ——
/// 真正的数据保护需要整库加密，那超出了当前版本的范围。
fn hash_pin(pin: &str, salt: &str) -> String {
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(pin.as_bytes());
    format!("{salt}${:x}", h.finalize())
}

const PIN_KEY: &str = "lockPinHash";

fn read_pin_hash(conn: &rusqlite::Connection) -> String {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [PIN_KEY], |r| {
        r.get::<_, String>(0)
    })
    // 库里存的是 JSON 字符串，所以这里要再解一层
    .ok()
    .and_then(|raw| serde_json::from_str::<String>(&raw).ok())
    .unwrap_or_default()
}

fn write_pin_hash(conn: &rusqlite::Connection, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![PIN_KEY, serde_json::to_string(value)?],
    )?;
    Ok(())
}

#[tauri::command]
pub fn set_lock_pin(state: State<'_, AppState>, pin: Option<String>) -> Result<()> {
    let db = state.db()?;
    let value = match pin.filter(|p| !p.is_empty()) {
        Some(p) => {
            if p.len() < 4 || p.len() > 8 || !p.chars().all(|c| c.is_ascii_digit()) {
                return Err(AppError::other("PIN 必须是 4-8 位数字"));
            }
            // 盐用时间戳：同一 PIN 在不同机器、不同次设置下哈希都不同
            hash_pin(&p, &format!("{:x}", now_ms()))
        }
        None => String::new(),
    };
    db.with(|c| write_pin_hash(c, &value))
}

#[tauri::command]
pub fn verify_lock_pin(state: State<'_, AppState>, pin: String) -> Result<bool> {
    let db = state.db()?;
    let stored = db.with(|c| Ok(read_pin_hash(c)))?;
    if stored.is_empty() {
        // 没设过 PIN 时视为已解锁，避免把用户永久锁在外面
        return Ok(true);
    }
    let Some((salt, _)) = stored.split_once('$') else {
        return Ok(false);
    };
    Ok(hash_pin(&pin, salt) == stored)
}

/// 退出应用（托盘菜单与设置面板都会用到）。
#[tauri::command]
pub fn relaunch_app(app: AppHandle) -> Result<()> {
    // tauri-plugin-process 只提供 JS 侧命令（exit / restart），没有 Rust 侧 trait。
    // Rust 里等价的能力是 AppHandle::request_restart()。
    // 重启前先标记"正在退出"，否则关闭拦截会把它当成"关闭到托盘"而拦下来。
    app.state::<AppState>().begin_quit();
    app.request_restart();
    Ok(())
}
