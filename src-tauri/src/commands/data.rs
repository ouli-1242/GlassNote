//! 数据命令：备份、导出、导入、回收站清理、维护。
//!
//! 文件路径由前端通过 dialog 插件选好后传进来（前端有 `dialog:default` 权限），
//! Rust 侧只负责读写 —— 这样避免在命令里同步阻塞地弹系统对话框。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::db::now_ms;
use crate::error::{AppError, Result};
use crate::commands::tasks::TASK_COLS;
use crate::models::{Memo, Task};
use crate::settings::{self, AppSettings};
use crate::state::AppState;

const MS_DAY: i64 = 86_400_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub modified_at: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub tasks: usize,
    pub memos: usize,
    pub settings: usize,
    pub skipped: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceReport {
    pub trashed: usize,
    pub purged_tasks: usize,
    pub purged_memos: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Dump {
    version: i32,
    exported_at: i64,
    tasks: Vec<Task>,
    memos: Vec<Memo>,
    settings: Option<AppSettings>,
}

// ─────────────────────────────── 目录 ───────────────────────────────

#[tauri::command]
pub fn data_dir(state: State<'_, AppState>) -> Result<String> {
    Ok(state.paths.root.to_string_lossy().to_string())
}

#[tauri::command]
pub fn open_data_dir(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    // Windows 分支不需要 app，这里统一标记为已使用，避免 cfg 差异带来的告警
    let _ = &app;
    let dir = state.paths.root.clone();
    // 用资源管理器打开目录。这里不引 opener 插件：调一次系统命令就够了，
    // 少一个依赖也少一份权限声明。
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(dir.as_os_str())
            .spawn()
            .map_err(|e| AppError::other(format!("打开目录失败：{e}")))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = &dir;
    }
    Ok(())
}

// ─────────────────────────────── 备份 ───────────────────────────────

fn list_backup_files(dir: &Path) -> Vec<BackupInfo> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<BackupInfo> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("glassnote-"))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            Some(BackupInfo {
                path: e.path().to_string_lossy().to_string(),
                name: e.file_name().to_string_lossy().to_string(),
                size_bytes: meta.len(),
                modified_at: modified,
            })
        })
        .collect();
    // 新的在前，方便 UI 直接展示最近几份
    out.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    out
}

#[tauri::command]
pub fn backup_now(state: State<'_, AppState>) -> Result<BackupInfo> {
    let db = state.db()?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dest = state.paths.backup_dir.join(format!("glassnote-{stamp}.db"));
    db.backup_to(&dest)?;

    let keep = state.settings().backup_keep.max(1) as usize;
    db.rotate_backups(keep)?;

    let meta = std::fs::metadata(&dest)?;
    Ok(BackupInfo {
        path: dest.to_string_lossy().to_string(),
        name: dest.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        size_bytes: meta.len(),
        modified_at: now_ms(),
    })
}

#[tauri::command]
pub fn list_backups(state: State<'_, AppState>) -> Result<Vec<BackupInfo>> {
    Ok(list_backup_files(&state.paths.backup_dir))
}

/// 从备份恢复。
///
/// 恢复本身也是破坏性操作，所以先把当前库另存一份，
/// 万一用户选错了文件，至少还能回到恢复前的状态。
#[tauri::command]
pub fn restore_backup(state: State<'_, AppState>, path: String) -> Result<()> {
    let src = PathBuf::from(&path);
    if !src.exists() {
        return Err(AppError::other("备份文件不存在"));
    }
    let db = state.db()?;

    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let safety = state.paths.backup_dir.join(format!("glassnote-before-restore-{stamp}.db"));
    db.backup_to(&safety)?;

    // 走 SQLite 在线备份 API 把页灌进活动连接，而不是覆盖文件（原因见 Db::restore_from）
    db.restore_from(&src)?;

    // 设置可能被备份覆盖了，重新读进内存缓存
    let reloaded = db.with(|c| settings::load(c))?;
    state.set_settings(reloaded);
    Ok(())
}

// ─────────────────────────────── 导出 ───────────────────────────────

fn csv_escape(s: &str) -> String {
    // CSV 里逗号、引号、换行都要用引号包起来，内部引号翻倍
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn export_markdown(tasks: &[Task], memos: &[Memo]) -> String {
    let mut out = String::from("# GlassNote 导出\n\n");
    out.push_str(&format!("导出时间：{}\n\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));

    out.push_str(&format!("## 任务（{}）\n\n", tasks.len()));
    for t in tasks {
        let mark = if t.status == "done" { "x" } else { " " };
        out.push_str(&format!("- [{mark}] {}\n", t.title));
        if let Some(note) = t.note.as_deref().filter(|n| !n.is_empty()) {
            out.push_str(&format!("  - 备注：{note}\n"));
        }
        if let Some(due) = t.due_at {
            let dt = chrono::DateTime::from_timestamp_millis(due)
                .map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default();
            out.push_str(&format!("  - 时间：{dt}\n"));
        }
        if t.repeat_type != "once" {
            out.push_str(&format!("  - 重复：{}\n", t.repeat_type));
        }
        if !t.tags.is_empty() {
            out.push_str(&format!("  - 标签：{}\n", t.tags.join("、")));
        }
    }

    out.push_str(&format!("\n## 备忘录（{}）\n\n", memos.len()));
    for m in memos {
        out.push_str(&format!("### {}\n\n{}\n\n", m.title, m.content));
    }
    out
}

fn export_csv(tasks: &[Task]) -> String {
    let mut out = String::from("id,title,status,priority,tags,due_at,remind_at,repeat_type,note,created_at,completed_at\n");
    for t in tasks {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{}\n",
            t.id,
            csv_escape(&t.title),
            t.status,
            t.priority,
            csv_escape(&t.tags.join("|")),
            t.due_at.map(|v| v.to_string()).unwrap_or_default(),
            t.remind_at.map(|v| v.to_string()).unwrap_or_default(),
            t.repeat_type,
            csv_escape(t.note.as_deref().unwrap_or("")),
            t.created_at,
            t.completed_at.map(|v| v.to_string()).unwrap_or_default(),
        ));
    }
    out
}

#[tauri::command]
pub fn export_data(state: State<'_, AppState>, format: String, path: String) -> Result<serde_json::Value> {
    if path.trim().is_empty() {
        return Err(AppError::other("未选择导出路径"));
    }
    let db = state.db()?;

    // 导出包含回收站内容：用户点"导出"时通常想要完整数据
    let (tasks, memos) = db.with(|c| {
        let mut stmt = c.prepare(&format!("SELECT {TASK_COLS} FROM tasks ORDER BY created_at"))?;
        let tasks: Vec<Task> = stmt
            .query_map([], crate::commands::tasks::row_to_task)?
            .collect::<rusqlite::Result<_>>()?;
        let mut stmt = c.prepare(
            "SELECT id, title, content, color, collapsed, pinned, sort_order, created_at, updated_at, deleted_at
             FROM memos ORDER BY created_at",
        )?;
        let memos: Vec<Memo> = stmt
            .query_map([], crate::commands::memos::row_to_memo)?
            .collect::<rusqlite::Result<_>>()?;
        Ok((tasks, memos))
    })?;

    let content = match format.as_str() {
        "markdown" => export_markdown(&tasks, &memos),
        "csv" => export_csv(&tasks),
        _ => serde_json::to_string_pretty(&Dump {
            version: 1,
            exported_at: now_ms(),
            tasks,
            memos,
            settings: Some(state.settings()),
        })?,
    };

    let dest = PathBuf::from(&path);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&dest, &content)?;

    Ok(serde_json::json!({ "path": dest.to_string_lossy(), "bytes": content.len() }))
}

// ─────────────────────────────── 导入 ───────────────────────────────

#[tauri::command]
pub fn import_data(state: State<'_, AppState>, path: String, mode: String) -> Result<ImportSummary> {
    if path.trim().is_empty() {
        return Err(AppError::other("未选择导入文件"));
    }
    let raw = std::fs::read_to_string(&path)?;
    let dump: Dump = serde_json::from_str(&raw)
        .map_err(|e| AppError::other(format!("文件不是有效的 GlassNote 导出：{e}")))?;

    let db = state.db()?;
    let now = now_ms();
    let replace = mode == "replace";

    let mut summary = ImportSummary { tasks: 0, memos: 0, settings: 0, skipped: 0 };

    db.with_tx(|tx| {
        if replace {
            tx.execute("DELETE FROM tasks", [])?;
            tx.execute("DELETE FROM memos", [])?;
        }

        for t in &dump.tasks {
            // merge 模式下按"标题 + 出现时间"判重：便签场景里这比 id 更贴近用户认知
            if !replace {
                let dup: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM tasks WHERE title = ?1 AND IFNULL(due_at, -1) = IFNULL(?2, -1)",
                    rusqlite::params![t.title, t.due_at],
                    |r| r.get(0),
                )?;
                if dup > 0 {
                    summary.skipped += 1;
                    continue;
                }
            }
            tx.execute(
                "INSERT INTO tasks (title, note, status, priority, tags, due_at, remind_at, repeat_type,
                    repeat_interval, repeat_unit, repeat_weekdays, repeat_monthday, repeat_end_at, repeat_cron,
                    parent_id, pinned, sort_order, created_at, updated_at, completed_at, deleted_at, remind_fired_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,NULL,?15,?16,?17,?18,?19,?20,?21)",
                rusqlite::params![
                    t.title,
                    t.note,
                    t.status,
                    t.priority,
                    crate::db::to_json_array(&t.tags),
                    t.due_at,
                    t.remind_at,
                    t.repeat_type,
                    t.repeat_interval,
                    t.repeat_unit,
                    crate::db::to_json_array(&t.repeat_weekdays),
                    t.repeat_monthday,
                    t.repeat_end_at,
                    t.repeat_cron,
                    if t.pinned { 1 } else { 0 },
                    t.sort_order,
                    t.created_at,
                    now,
                    t.completed_at,
                    t.deleted_at,
                    t.remind_fired_at,
                ],
            )?;
            summary.tasks += 1;
        }

        for m in &dump.memos {
            tx.execute(
                "INSERT INTO memos (title, content, color, collapsed, pinned, sort_order, created_at, updated_at, deleted_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                rusqlite::params![
                    m.title,
                    m.content,
                    m.color,
                    if m.collapsed { 1 } else { 0 },
                    if m.pinned { 1 } else { 0 },
                    m.sort_order,
                    m.created_at,
                    now,
                    m.deleted_at,
                ],
            )?;
            summary.memos += 1;
        }

        // 设置只在 replace 模式下导入：merge 时覆盖用户当前偏好会很意外
        if replace {
            if let Some(s) = &dump.settings {
                settings::write_all(tx, s)?;
                summary.settings = 1;
            }
        }
        Ok(())
    })?;

    Ok(summary)
}

// ─────────────────────────────── 清理 / 维护 ───────────────────────────────

/// 日常维护：回收站超期清理、已完成任务超期清理、备份轮转、自动备份。
///
/// 由调度器定期调用，也可以在设置里手动触发。
pub fn maintenance_inner(app: &AppHandle) -> Result<MaintenanceReport> {
    let state = app.state::<AppState>();
    let db = state.db()?;
    let s = state.settings();
    let now = now_ms();

    let mut report = MaintenanceReport { trashed: 0, purged_tasks: 0, purged_memos: 0 };

    db.with_tx(|tx| {
        // 回收站超期
        if s.trash_keep_days > 0 {
            let cutoff = now - s.trash_keep_days * MS_DAY;
            report.purged_tasks += tx.execute(
                "DELETE FROM tasks WHERE deleted_at IS NOT NULL AND deleted_at < ?1",
                rusqlite::params![cutoff],
            )?;
            report.purged_memos += tx.execute(
                "DELETE FROM memos WHERE deleted_at IS NOT NULL AND deleted_at < ?1",
                rusqlite::params![cutoff],
            )?;
        }
        // 已完成任务超期（0 表示不清理）
        if s.done_keep_days > 0 {
            let cutoff = now - s.done_keep_days * MS_DAY;
            report.trashed += tx.execute(
                "DELETE FROM tasks WHERE status = 'done' AND deleted_at IS NULL AND completed_at IS NOT NULL AND completed_at < ?1",
                rusqlite::params![cutoff],
            )?;
        }
        Ok(())
    })?;

    if s.auto_backup_enabled {
        let stamp = chrono::Local::now().format("%Y%m%d");
        let dest = state.paths.backup_dir.join(format!("glassnote-auto-{stamp}.db"));
        // 同一天只做一次，避免每次维护都写一份
        if !dest.exists() {
            if let Err(e) = db.backup_to(&dest) {
                eprintln!("[glassnote] 自动备份失败：{e}");
            }
        }
        let _ = db.rotate_backups(s.backup_keep.max(1) as usize);
    }

    Ok(report)
}

#[tauri::command]
pub fn run_maintenance(app: AppHandle) -> Result<MaintenanceReport> {
    maintenance_inner(&app)
}

#[tauri::command]
pub fn clear_all_data(state: State<'_, AppState>, include_settings: bool) -> Result<()> {
    let db = state.db()?;
    db.with_tx(|tx| {
        tx.execute("DELETE FROM tasks", [])?;
        tx.execute("DELETE FROM memos", [])?;
        tx.execute("DELETE FROM completed_logs", [])?;
        tx.execute("DELETE FROM recurrence_exceptions", [])?;
        if include_settings {
            tx.execute("DELETE FROM settings", [])?;
        }
        Ok(())
    })?;
    if include_settings {
        state.set_settings(AppSettings::default());
    }
    // VACUUM 回收空间：清空后文件不会自动缩小，用户看到"清了但还占 10MB"会困惑
    db.with(|c| {
        c.execute_batch("VACUUM")?;
        Ok(())
    })?;
    Ok(())
}
