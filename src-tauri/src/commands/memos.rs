//! 备忘录命令。
//!
//! 折叠状态存在库里（`memos.collapsed`），不是前端内存状态 ——
//! 规格明确要求"折叠状态持久化，下次打开保持"。

use rusqlite::{params, Row};
use tauri::State;

use crate::db::now_ms;
use crate::error::{AppError, Result};
use crate::models::{Memo, MemoInput};
use crate::state::AppState;

pub fn row_to_memo(r: &Row) -> rusqlite::Result<Memo> {
    Ok(Memo {
        id: r.get("id")?,
        title: r.get("title")?,
        content: r.get("content")?,
        color: r.get("color")?,
        collapsed: r.get::<_, i64>("collapsed")? != 0,
        pinned: r.get::<_, i64>("pinned")? != 0,
        sort_order: r.get("sort_order")?,
        created_at: r.get("created_at")?,
        updated_at: r.get("updated_at")?,
        deleted_at: r.get("deleted_at")?,
    })
}

/// 备忘录列清单，同样供导出复用。
pub const MEMO_COLS: &str = "id, title, content, color, collapsed, pinned, sort_order, created_at, updated_at, deleted_at";

#[tauri::command]
pub fn list_memos(state: State<'_, AppState>, include_trashed: Option<bool>) -> Result<Vec<Memo>> {
    let db = state.db()?;
    let trashed = include_trashed.unwrap_or(false);
    db.with(|c| {
        // 置顶优先，其次手工排序；时间只作为最后兜底
        let sql = format!(
            "SELECT {MEMO_COLS} FROM memos WHERE deleted_at IS {} ORDER BY pinned DESC, sort_order ASC, created_at DESC",
            if trashed { "NOT NULL" } else { "NULL" }
        );
        let mut stmt = c.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_memo)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
}

fn get_memo(c: &rusqlite::Connection, id: i64) -> Result<Memo> {
    c.query_row(&format!("SELECT {MEMO_COLS} FROM memos WHERE id=?1"), params![id], row_to_memo)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => AppError::other(format!("备忘录 {id} 不存在")),
            other => other.into(),
        })
}

/// 新建或更新备忘录。id 为空即新建。
#[tauri::command]
pub fn save_memo(state: State<'_, AppState>, input: MemoInput) -> Result<Memo> {
    let db = state.db()?;
    let now = now_ms();

    db.with_tx(|tx| {
        match input.id {
            Some(id) => {
                // 用 COALESCE 语义：前端只传想改的字段，其余保持原值。
                // 自动保存会频繁调用，不该要求前端每次都回传完整对象。
                tx.execute(
                    "UPDATE memos SET
                        title = COALESCE(?2, title),
                        content = COALESCE(?3, content),
                        color = COALESCE(?4, color),
                        collapsed = COALESCE(?5, collapsed),
                        pinned = COALESCE(?6, pinned),
                        updated_at = ?7
                     WHERE id = ?1",
                    params![
                        id,
                        input.title.as_str(),
                        input.content.as_deref(),
                        input.color.as_deref(),
                        input.collapsed.map(|b| if b { 1 } else { 0 }),
                        input.pinned.map(|b| if b { 1 } else { 0 }),
                        now,
                    ],
                )?;
                get_memo(tx, id)
            }
            None => {
                let min_order: f64 = tx
                    .query_row("SELECT IFNULL(MIN(sort_order), 0) FROM memos WHERE deleted_at IS NULL", [], |r| r.get(0))
                    .unwrap_or(0.0);
                tx.execute(
                    "INSERT INTO memos (title, content, color, collapsed, pinned, sort_order, created_at, updated_at)
                     VALUES (?1, ?2, ?3, 0, 0, ?4, ?5, ?5)",
                    params![
                        if input.title.trim().is_empty() { "新备忘录" } else { input.title.trim() },
                        input.content.clone().unwrap_or_default(),
                        input.color.clone().unwrap_or_else(|| "default".into()),
                        min_order - 1.0,
                        now,
                    ],
                )?;
                get_memo(tx, tx.last_insert_rowid())
            }
        }
    })
}

#[tauri::command]
pub fn set_memo_collapsed(state: State<'_, AppState>, id: i64, collapsed: bool) -> Result<()> {
    let db = state.db()?;
    db.with(|c| {
        c.execute(
            "UPDATE memos SET collapsed=?2, updated_at=?3 WHERE id=?1",
            params![id, if collapsed { 1 } else { 0 }, now_ms()],
        )?;
        Ok(())
    })
}

#[tauri::command]
pub fn set_memo_pinned(state: State<'_, AppState>, id: i64, pinned: bool) -> Result<()> {
    let db = state.db()?;
    db.with(|c| {
        c.execute(
            "UPDATE memos SET pinned=?2, updated_at=?3 WHERE id=?1",
            params![id, if pinned { 1 } else { 0 }, now_ms()],
        )?;
        Ok(())
    })
}

#[tauri::command]
pub fn delete_memo(state: State<'_, AppState>, id: i64) -> Result<()> {
    let db = state.db()?;
    db.with(|c| {
        c.execute("UPDATE memos SET deleted_at=?2, updated_at=?2 WHERE id=?1", params![id, now_ms()])?;
        Ok(())
    })
}

#[tauri::command]
pub fn restore_memo(state: State<'_, AppState>, id: i64) -> Result<()> {
    let db = state.db()?;
    db.with(|c| {
        c.execute("UPDATE memos SET deleted_at=NULL, updated_at=?2 WHERE id=?1", params![id, now_ms()])?;
        Ok(())
    })
}

#[tauri::command]
pub fn purge_memo(state: State<'_, AppState>, id: i64) -> Result<()> {
    let db = state.db()?;
    db.with(|c| {
        c.execute("DELETE FROM memos WHERE id=?1", params![id])?;
        Ok(())
    })
}

#[tauri::command]
pub fn reorder_memos(state: State<'_, AppState>, ids: Vec<i64>) -> Result<()> {
    let db = state.db()?;
    db.with_tx(|tx| {
        let now = now_ms();
        let mut stmt = tx.prepare("UPDATE memos SET sort_order=?2, updated_at=?3 WHERE id=?1")?;
        for (i, id) in ids.iter().enumerate() {
            stmt.execute(params![id, i as f64, now])?;
        }
        Ok(())
    })
}
