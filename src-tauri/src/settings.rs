//! 设置模型与持久化。
//!
//! 存储策略：`settings` 表按**字段一行**存（key = 字段名，value = 该字段的 JSON）。
//! 相比整份设置存成一坨 JSON，按字段存的好处是：
//!   - 改一个开关只写一行，不必重写整份配置；
//!   - 库里能直接看懂某一项的值，便于排查问题；
//!   - 新增字段时旧库不需要迁移 —— 读的时候用默认值补齐即可。
//!
//! 缺失字段一律回落到 `Default`，所以任何历史版本的库都能安全读出来。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub autostart: bool,
    /// 开机自启后延迟多少秒才做初始化，避开开机资源争抢
    pub autostart_delay_sec: i64,
    pub opacity: f64,
    /// 只有 "light" / "dark" 两套，没有"跟随系统"
    pub theme: String,
    pub font_scale: f64,
    pub default_repeat_type: String,
    pub default_remind_offset_min: i64,
    pub always_on_top: bool,
    pub show_tray_icon: bool,
    pub close_to_tray: bool,
    /// 隐藏时销毁窗口以释放 WebView2 内存，代价是再次显示要重建
    pub release_memory_when_hidden: bool,
    pub notifications_enabled: bool,
    pub dnd_start: String,
    pub dnd_end: String,
    pub shortcut_capture: String,
    pub shortcut_toggle: String,
    pub language: String,
    pub lock_enabled: bool,
    pub auto_backup_enabled: bool,
    pub backup_keep: i64,
    pub trash_keep_days: i64,
    pub done_keep_days: i64,
    pub window_collapsed: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            autostart: false,
            autostart_delay_sec: 5,
            opacity: 0.96,
            theme: "light".into(),
            font_scale: 1.0,
            default_repeat_type: "once".into(),
            default_remind_offset_min: 0,
            always_on_top: false,
            show_tray_icon: true,
            close_to_tray: true,
            release_memory_when_hidden: true,
            notifications_enabled: true,
            dnd_start: "22:30".into(),
            dnd_end: "07:30".into(),
            shortcut_capture: "CommandOrControl+Alt+N".into(),
            shortcut_toggle: "CommandOrControl+Shift+Space".into(),
            language: "zh".into(),
            lock_enabled: false,
            auto_backup_enabled: true,
            backup_keep: 7,
            trash_keep_days: 30,
            done_keep_days: 0,
            window_collapsed: false,
        }
    }
}

impl AppSettings {
    /// 面板透明度限制在 20%~100%：再低文字就没法保证可读性了。
    pub fn normalized(mut self) -> Self {
        self.opacity = self.opacity.clamp(0.2, 1.0);
        self.font_scale = self.font_scale.clamp(0.85, 1.3);
        self.backup_keep = self.backup_keep.clamp(1, 30);
        self.autostart_delay_sec = self.autostart_delay_sec.clamp(0, 120);
        // 旧版本允许 "system"，也允许空值。收敛到两套之一：
        // 主题字符串会直接决定 <html> 上挂哪个 class，值非法的话
        // 两边 class 都加不上，整套 CSS 变量取不到值 —— 窗口会变成一片透明。
        if self.theme != "dark" && self.theme != "light" {
            self.theme = "light".into();
        }
        self
    }
}

/// 从 settings 表读出全部设置。
///
/// 做法：先把默认值序列化成 JSON 对象，再用库里的行逐个覆盖同名字段，
/// 最后反序列化。这样库里的脏值/未知键都不会导致读取失败。
pub fn load(conn: &Connection) -> Result<AppSettings> {
    let mut base = serde_json::to_value(AppSettings::default())?;
    let obj = base.as_object_mut().expect("AppSettings 一定是对象");

    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;

    for row in rows {
        let (key, value) = row?;
        if !obj.contains_key(&key) {
            continue; // 已废弃的键直接忽略
        }
        match serde_json::from_str::<serde_json::Value>(&value) {
            Ok(v) => {
                obj.insert(key, v);
            }
            Err(_) => {
                // 单个字段解析失败只丢这一项，其余设置照常生效
                eprintln!("[glassnote] 设置项 {key} 的值无法解析，使用默认值");
            }
        }
    }

    let parsed: AppSettings = serde_json::from_value(base).unwrap_or_default();
    Ok(parsed.normalized())
}

/// 只写入 patch 里出现的字段。返回合并后的完整设置。
pub fn save_patch(conn: &Connection, patch: &serde_json::Value) -> Result<AppSettings> {
    let Some(patch_obj) = patch.as_object() else {
        return load(conn);
    };

    let valid: serde_json::Map<String, serde_json::Value> =
        serde_json::to_value(AppSettings::default())?.as_object().cloned().unwrap_or_default();

    let mut stmt = conn.prepare("INSERT INTO settings(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value")?;
    for (key, value) in patch_obj {
        // 只接受已知字段：防止前端传错键名时在库里悄悄积累垃圾数据
        if !valid.contains_key(key) {
            continue;
        }
        stmt.execute(rusqlite::params![key, serde_json::to_string(value)?])?;
    }
    drop(stmt);

    load(conn)
}

pub fn write_all(conn: &Connection, s: &AppSettings) -> Result<()> {
    let obj = serde_json::to_value(s)?.as_object().cloned().unwrap_or_default();
    let mut stmt = conn.prepare("INSERT INTO settings(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value")?;
    for (k, v) in obj {
        stmt.execute(rusqlite::params![k, serde_json::to_string(&v)?])?;
    }
    Ok(())
}

/// 判断当前是否处于免打扰时段。
///
/// 跨零点的时间段（如 22:30–07:30）必须当成两段处理，
/// 直接比较 `now >= start && now <= end` 会永远为假。
pub fn in_dnd(s: &AppSettings, now_min: i64) -> bool {
    let parse = |t: &str| -> Option<i64> {
        let (h, m) = t.split_once(':')?;
        Some(h.trim().parse::<i64>().ok()? * 60 + m.trim().parse::<i64>().ok()?)
    };
    let (Some(start), Some(end)) = (parse(&s.dnd_start), parse(&s.dnd_end)) else {
        return false;
    };
    if start == end {
        return false;
    }
    if start < end {
        now_min >= start && now_min < end
    } else {
        // 跨零点
        now_min >= start || now_min < end
    }
}

/// 引导期注入给前端的负载。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootPayload {
    pub theme: String,
    pub opacity: f64,
    pub font_scale: f64,
    pub portable: bool,
    pub autostart_run: bool,
    pub window_collapsed: bool,
}

impl BootPayload {
    pub fn from_settings(s: &AppSettings, portable: bool, autostart_run: bool) -> Self {
        Self {
            theme: s.theme.clone(),
            opacity: s.opacity,
            font_scale: s.font_scale,
            portable,
            autostart_run,
            window_collapsed: s.window_collapsed,
        }
    }

    /// 生成注入脚本。用 JSON 而不是字符串拼接，避免值里的引号把脚本写坏。
    pub fn to_script(&self) -> String {
        let json = serde_json::to_string(self).unwrap_or_else(|_| "{}".into());
        format!("window.__GLASSNOTE_BOOT__ = {json};")
    }
}

/// 引导缓存的存放文件名。
///
/// 为什么需要它：窗口创建时必须立刻拿到主题/透明度来生成 `initialization_script`，
/// 而此时数据库可能**还没打开**（开机自启场景下我们要等延迟结束才开库）。
/// 用 tauri-plugin-store 存一份极小的引导缓存，读它是纯文件操作，与数据库解耦。
/// 它是 SQLite 设置的**派生副本**，唯一用途就是启动首帧，不是第二份事实来源。
const BOOT_STORE: &str = "appearance.cache.json";
const BOOT_KEY: &str = "boot";

pub fn store_boot(app: &tauri::AppHandle, payload: &BootPayload) -> Result<()> {
    use tauri_plugin_store::StoreExt;
    let store = app
        .store(BOOT_STORE)
        .map_err(|e| crate::error::AppError::other(format!("打开引导缓存失败：{e}")))?;
    store.set(BOOT_KEY, serde_json::to_value(payload)?);
    store
        .save()
        .map_err(|e| crate::error::AppError::other(format!("写入引导缓存失败：{e}")))?;
    Ok(())
}

/// 读引导缓存。缓存不存在或损坏时返回 None，由调用方回落到默认设置。
pub fn load_boot(app: &tauri::AppHandle) -> Option<BootPayload> {
    use tauri_plugin_store::StoreExt;
    let store = app.store(BOOT_STORE).ok()?;
    let value = store.get(BOOT_KEY)?;
    let mut payload: BootPayload = serde_json::from_value(value).ok()?;
    // 这份缓存是上一版应用写下的，里面可能还留着已经废弃的 "system"。
    // 不收敛的话前端两个主题 class 都加不上，整套 CSS 变量取不到值，
    // 表现是窗口一片透明 —— 而缓存文件不会自动清理。
    if payload.theme != "dark" && payload.theme != "light" {
        payload.theme = "light".into();
    }
    Some(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dnd_handles_cross_midnight() {
        let mut s = AppSettings::default();
        s.dnd_start = "22:30".into();
        s.dnd_end = "07:30".into();
        assert!(in_dnd(&s, 23 * 60)); // 23:00 在区间内
        assert!(in_dnd(&s, 6 * 60)); // 06:00 在区间内
        assert!(!in_dnd(&s, 12 * 60)); // 12:00 不在
        assert!(!in_dnd(&s, 22 * 60 + 29));
    }

    #[test]
    fn dnd_same_start_end_means_disabled() {
        let mut s = AppSettings::default();
        s.dnd_start = "08:00".into();
        s.dnd_end = "08:00".into();
        assert!(!in_dnd(&s, 8 * 60));
    }

    #[test]
    fn dnd_malformed_input_does_not_panic() {
        let mut s = AppSettings::default();
        s.dnd_start = "not a time".into();
        assert!(!in_dnd(&s, 600));
    }

    #[test]
    fn opacity_is_clamped_to_readable_range() {
        let s = AppSettings {
            opacity: 0.01,
            ..Default::default()
        }
        .normalized();
        assert_eq!(s.opacity, 0.2);
    }
}
