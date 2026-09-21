use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, RwLock};

use crate::db::Db;
use crate::error::Result;
use crate::paths::Paths;
use crate::settings::{self, AppSettings};

/// 应用全局状态。
///
/// 数据库是**懒初始化**的，这一点很关键：
///   - 开机自启场景下，进程要先静默驻留托盘、等 `autostart_delay_sec` 秒之后
///     才去打开数据库和跑迁移，避免和系统开机过程抢 IO；
///   - 但如果用户在延迟期内就点开了窗口，第一条命令会立刻触发初始化，
///     不会因为"还没到点"而失败。
/// 用 OnceLock 表达"最多初始化一次且线程安全"，比加锁判断标志位更干净。
pub struct AppState {
    pub paths: Paths,
    db: OnceLock<Db>,
    settings: RwLock<AppSettings>,
    /// 是否为开机自启拉起（决定要不要弹窗、要不要延迟初始化）
    pub autostart_run: bool,
    /// 正在退出：此时关闭窗口不应再拦截成"最小化到托盘"
    pub quitting: AtomicBool,
    /// 串行化数据库初始化的锁，避免两个命令同时触发时重复打开
    init_lock: Mutex<()>,
}

impl AppState {
    pub fn new(paths: Paths, autostart_run: bool) -> Self {
        Self {
            paths,
            db: OnceLock::new(),
            settings: RwLock::new(AppSettings::default()),
            autostart_run,
            quitting: AtomicBool::new(false),
            init_lock: Mutex::new(()),
        }
    }

    /// 取数据库句柄，必要时初始化。
    pub fn db(&self) -> Result<&Db> {
        if let Some(db) = self.db.get() {
            return Ok(db);
        }
        // 双重检查：先抢锁，抢到后再确认一次是否已被别的线程初始化
        let _guard = self.init_lock.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(db) = self.db.get() {
            return Ok(db);
        }
        let db = Db::open(&self.paths)?;
        // 设置要从库里读出来覆盖默认值
        let loaded = db.with(|c| settings::load(c))?;
        *self.settings.write().unwrap_or_else(|e| e.into_inner()) = loaded;
        // set 失败说明另一个线程抢先了，此时用它的即可
        let _ = self.db.set(db);
        Ok(self.db.get().expect("刚刚设置过"))
    }

    pub fn ensure_db(&self) -> Result<()> {
        self.db().map(|_| ())
    }

    pub fn settings(&self) -> AppSettings {
        self.settings.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_settings(&self, s: AppSettings) {
        *self.settings.write().unwrap_or_else(|e| e.into_inner()) = s;
    }

    pub fn is_quitting(&self) -> bool {
        self.quitting.load(Ordering::SeqCst)
    }

    pub fn begin_quit(&self) {
        self.quitting.store(true, Ordering::SeqCst);
    }
}
