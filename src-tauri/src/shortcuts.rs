//! 全局快捷键。
//!
//! 注册在 Rust 侧而不是前端：这样即使窗口被销毁（"隐藏时释放内存"开启时的常态），
//! 快捷键依然有效 —— 否则用户关了窗口就再也叫不出来了。
//!
//! 组合键字符串沿用 Tauri 的写法（`CommandOrControl+Alt+N`），
//! 在 Windows 上 `CommandOrControl` 等价于 `Ctrl`，同一份配置将来也能直接用于 macOS。

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::state::AppState;

pub fn parse(s: &str) -> Option<Shortcut> {
    s.trim().parse::<Shortcut>().ok()
}

/// 注册全局快捷键，返回注册失败的描述（成功时为空）。
///
/// 返回列表而不是 Err：两个快捷键可能只有一个冲突，
/// 此时另一个应该照常生效，整体失败会让用户连能用的那个也失去。
pub fn reregister(app: &AppHandle, capture: &str, toggle: &str) -> Vec<String> {
    let gs = app.global_shortcut();
    // 先全部注销，否则改键时旧键会残留
    let _ = gs.unregister_all();

    let mut failures = Vec::new();
    for (label, spec) in [("快速添加", capture), ("显示/隐藏窗口", toggle)] {
        let Some(sc) = parse(spec) else {
            failures.push(format!("{label}：无法解析「{spec}」"));
            continue;
        };
        if let Err(e) = gs.register(sc) {
            // 最常见的原因是已被别的程序占用
            failures.push(format!("{label}：{e}"));
        }
    }
    failures
}

/// 把快捷键处理挂到插件上。
///
/// 处理函数里不直接做重活：只发事件/调窗口函数。
/// 快捷键回调运行在系统消息线程上，在里面做数据库操作会卡住整个事件循环。
pub fn build_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            // 只在按下时响应；否则一次按键会触发两次（按下 + 抬起）
            if event.state() != ShortcutState::Pressed {
                return;
            }
            let Some(state) = app.try_state::<AppState>() else {
                return;
            };
            let settings = state.settings();

            if parse(&settings.shortcut_capture).as_ref() == Some(shortcut) {
                let _ = crate::commands::window::open_capture_inner(app);
            } else if parse(&settings.shortcut_toggle).as_ref() == Some(shortcut) {
                let _ = crate::commands::window::toggle_main_inner(app);
            }
        })
        .build()
}
