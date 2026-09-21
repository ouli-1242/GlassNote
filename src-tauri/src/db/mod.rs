pub mod schema;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, Transaction};

use crate::error::Result;
use crate::paths::Paths;

/// 数据库句柄。
///
/// 用单连接 + Mutex 而不是连接池：这是个单用户桌面应用，并发度极低，
/// 连接池只会多占内存和线程。WAL 模式下读操作本身不互相阻塞，
/// 真正的串行点只有写，Mutex 的开销可以忽略。
pub struct Db {
    conn: Mutex<Connection>,
    pub paths: Paths,
}

impl Db {
    pub fn open(paths: &Paths) -> Result<Self> {
        let conn = Connection::open(&paths.db)?;
        configure(&conn)?;
        schema::migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            paths: paths.clone(),
        })
    }

    /// 借出连接执行一段操作。
    ///
    /// 锁中毒时取回内部值继续用，而不是 panic：SQLite 连接本身没有跨语句的不变量，
    /// 前一次操作 panic 不会让连接进入不可用状态，为此把整个应用拖垮不划算。
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        f(&guard)
    }

    /// 在一个事务里执行。闭包返回 Err 时自动回滚。
    pub fn with_tx<T>(&self, f: impl FnOnce(&Transaction) -> Result<T>) -> Result<T> {
        let mut guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = guard.transaction()?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }

    /// 一致性快照备份。
    ///
    /// 用 `VACUUM INTO` 而不是复制文件：WAL 模式下直接拷 .db 会丢掉未 checkpoint 的写入，
    /// 而 VACUUM INTO 在事务内生成一份完整、已整理的目标库。
    pub fn backup_to(&self, dest: &Path) -> Result<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if dest.exists() {
            std::fs::remove_file(dest)?;
        }
        let dest_str = dest.to_string_lossy().replace('\'', "''");
        self.with(|c| {
            c.execute_batch(&format!("VACUUM INTO '{dest_str}'"))?;
            Ok(())
        })
    }

    /// 从另一个 SQLite 文件把数据灌回当前连接。
    ///
    /// 为什么不用"复制文件覆盖"：连接是长期持有的，直接覆盖 .db 会绕过 WAL ——
    /// 尚未 checkpoint 的写入仍留在 `-wal` 里，重启后会被写回，把恢复的数据冲掉。
    /// 用 SQLite 的在线备份 API 把页拷进**活动连接**，是唯一不需要关连接、
    /// 也不需要动文件的做法。
    pub fn restore_from(&self, src: &Path) -> Result<()> {
        let source = Connection::open(src)?;
        let mut guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        {
            // rusqlite 的 Backup 是方向无关的：`new(from, to)` 就是把 from 的内容拷到 to。
            // 这里 from = 备份文件、to = 活动连接，即"从备份恢复"。
            // 备份期间 SQLite 禁止对目标库做任何 API 调用，所以 to 必须是 &mut。
            let backup = rusqlite::backup::Backup::new(&source, &mut guard)?;
            backup.run_to_completion(64, std::time::Duration::from_millis(20), None)?;
        }
        drop(guard);
        // 恢复后重跑一次迁移：备份可能来自更老的版本
        self.with(schema::migrate)
    }

    /// 轮转备份：按修改时间保留最近 `keep` 份，多余的删掉。
    pub fn rotate_backups(&self, keep: usize) -> Result<usize> {
        let mut files: Vec<_> = std::fs::read_dir(&self.paths.backup_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("glassnote-"))
            .collect();
        files.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
        let mut removed = 0;
        while files.len() > keep {
            let victim = files.remove(0);
            if std::fs::remove_file(victim.path()).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }
}

/// 连接级 PRAGMA。
///
/// 注意这些是**每连接**生效的，所以必须在打开连接后立刻设置，
/// 不能只在建库时设一次。
fn configure(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;        -- 读写不互相阻塞，这是崩溃恢复的基础
        PRAGMA synchronous = NORMAL;      -- WAL 下 NORMAL 已能保证不丢已提交事务，且省一次 fsync
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;
        PRAGMA temp_store = MEMORY;       -- 临时表放内存，避免写磁盘
        PRAGMA cache_size = -4000;        -- 约 4MB，够用且把内存占用钉死
        PRAGMA mmap_size = 33554432;      -- 32MB 虚拟映射，加速读，不占物理内存
        PRAGMA journal_size_limit = 8388608;
        "#,
    )?;
    Ok(())
}

/// 当前 Unix 毫秒。
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}



/// 把 JSON 数组字符串解析成整数数组，解析失败时退化为空数组。
///
/// 这里刻意不返回 Err：一条脏数据不应该让整个列表查询失败。
pub fn parse_i64_array(raw: &str) -> Vec<i64> {
    serde_json::from_str::<Vec<i64>>(raw).unwrap_or_default()
}

pub fn parse_str_array(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

pub fn to_json_array<T: serde::Serialize>(v: &[T]) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "[]".into())
}

