//! 数据库结构与迁移。
//!
//! 迁移用 `PRAGMA user_version` 做版本号，每次升级只追加一段 SQL，不修改历史段。
//! 这样任何老版本的库都能顺着版本号一路升上来，且升级过程可重复执行。

use rusqlite::Connection;

use crate::error::Result;

/// 当前 schema 版本。新增迁移时 +1 并在 `apply` 里加一段 match 分支。
pub const SCHEMA_VERSION: i32 = 1;

pub fn migrate(conn: &Connection) -> Result<()> {
    let current: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current >= SCHEMA_VERSION {
        return Ok(());
    }
    if current < 1 {
        apply_v1(conn)?;
    }
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

fn apply_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- ─────────────────────────── 任务 ───────────────────────────
        -- due_at 存的是"下一次出现的时间"，不是创建时的截止时间。
        -- 重复任务完成后由应用层把 due_at 推进到下一次，见 recurrence.rs 的模块注释。
        CREATE TABLE IF NOT EXISTS tasks (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            title           TEXT    NOT NULL,
            note            TEXT,
            status          TEXT    NOT NULL DEFAULT 'todo',   -- todo | done
            priority        INTEGER NOT NULL DEFAULT 2,        -- 0..4
            tags            TEXT    NOT NULL DEFAULT '[]',     -- JSON 数组
            due_at          INTEGER,                           -- Unix 毫秒
            remind_at       INTEGER,                           -- Unix 毫秒，可独立于 due_at
            repeat_type     TEXT    NOT NULL DEFAULT 'once',
            repeat_interval INTEGER NOT NULL DEFAULT 1,
            repeat_unit     TEXT    NOT NULL DEFAULT 'day',    -- day | week | month
            repeat_weekdays TEXT    NOT NULL DEFAULT '[]',     -- JSON 数组，ISO 1..7
            repeat_monthday INTEGER,                           -- 1..31，-1 表示沿用 due_at 的日
            repeat_end_at   INTEGER,
            repeat_cron     TEXT,
            parent_id       INTEGER REFERENCES tasks(id) ON DELETE CASCADE,
            pinned          INTEGER NOT NULL DEFAULT 0,
            sort_order      REAL    NOT NULL DEFAULT 0,
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL,
            completed_at    INTEGER,
            deleted_at      INTEGER,                           -- 非空即在回收站
            remind_fired_at INTEGER                            -- 本实例提醒是否已发出
        );

        -- 列表主查询是 status='todo' AND deleted_at IS NULL AND due_at <= ?
        -- 复合索引让这一条走索引扫描，不必回表过滤
        CREATE INDEX IF NOT EXISTS idx_tasks_live_due   ON tasks(deleted_at, status, due_at);
        CREATE INDEX IF NOT EXISTS idx_tasks_status     ON tasks(status);
        CREATE INDEX IF NOT EXISTS idx_tasks_due_at     ON tasks(due_at);
        CREATE INDEX IF NOT EXISTS idx_tasks_remind_at  ON tasks(remind_at);
        CREATE INDEX IF NOT EXISTS idx_tasks_deleted_at ON tasks(deleted_at);
        CREATE INDEX IF NOT EXISTS idx_tasks_parent_id  ON tasks(parent_id);
        CREATE INDEX IF NOT EXISTS idx_tasks_completed  ON tasks(completed_at DESC);

        -- ─────────────────────────── 备忘录 ───────────────────────────
        CREATE TABLE IF NOT EXISTS memos (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            title      TEXT    NOT NULL DEFAULT '',
            content    TEXT    NOT NULL DEFAULT '',
            color      TEXT    NOT NULL DEFAULT 'default',
            collapsed  INTEGER NOT NULL DEFAULT 0,   -- 折叠状态持久化
            pinned     INTEGER NOT NULL DEFAULT 0,
            sort_order REAL    NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            deleted_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_memos_live ON memos(deleted_at, pinned, sort_order);

        -- ─────────────────────────── 设置 ───────────────────────────
        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        -- ─────────────────────── 完成记录 / 例外 ───────────────────────
        -- 刻意不加指向 tasks 的外键：任务被永久删除后，历史仍应保留。
        -- 因此把标题冗余进来，避免历史列表出现空行。
        CREATE TABLE IF NOT EXISTS completed_logs (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id      INTEGER NOT NULL,
            task_title   TEXT    NOT NULL DEFAULT '',
            completed_at INTEGER NOT NULL,
            action       TEXT    NOT NULL,        -- complete | uncomplete | skip
            prev_due_at  INTEGER                  -- 完成前的 due_at，撤销时精确回滚
        );
        CREATE INDEX IF NOT EXISTS idx_logs_task      ON completed_logs(task_id);
        CREATE INDEX IF NOT EXISTS idx_logs_completed ON completed_logs(completed_at DESC);

        CREATE TABLE IF NOT EXISTS recurrence_exceptions (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id        INTEGER NOT NULL,
            exception_date INTEGER NOT NULL,   -- 该次实例的 due_at
            action         TEXT    NOT NULL,   -- skip | done
            UNIQUE(task_id, exception_date, action)
        );
        CREATE INDEX IF NOT EXISTS idx_exc_task ON recurrence_exceptions(task_id);
        "#,
    )?;
    Ok(())
}
