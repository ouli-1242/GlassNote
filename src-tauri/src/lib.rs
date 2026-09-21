//! GlassNote 应用入口。
//!
//! 启动顺序是有意为之的，每一步都在为"开机自启不卡顿、空闲不占资源"服务：
//!
//!   1. 单实例锁 —— 必须最先注册，重复启动时立刻聚焦已有窗口并退出新进程；
//!   2. 解析数据目录 —— 纯路径计算，不碰数据库；
//!   3. 创建托盘 —— 这是自启场景下唯一的常驻入口，必须最先可用；
//!   4. 按场景决定是否创建窗口：
//!        · 正常启动 → 建窗口并显示；
//!        · 开机自启 → **不建窗口**，只留托盘，后台等够延迟秒数再初始化数据库；
//!   5. 启动调度器线程 —— 唯一的定时源，大部分时间在 sleep。

mod commands;
mod db;
mod error;
mod glass;
mod models;
mod paths;
mod recurrence;
mod scheduler;
mod settings;
mod shortcuts;
mod state;
mod tray;

use std::time::Duration;

use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

use crate::paths::Paths;
use crate::state::AppState;

/// 命令行标记：本次启动是开机自启拉起的。
///
/// 由 autostart 插件注册启动项时附加，应用据此决定"静默驻留、不弹窗、不抢焦点"。
const AUTOSTART_FLAG: &str = "--autostart";

/// 需要记忆的窗口状态。只记位置和尺寸：
/// 最大化/最小化/全屏对便签面板没有意义，记住它们反而会让下次启动出现奇怪的初始状态。
const SAVED_STATE: StateFlags = StateFlags::POSITION.union(StateFlags::SIZE);

pub fn run() {
    let autostart_run = std::env::args().any(|a| a == AUTOSTART_FLAG);

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // 用户重复启动时把已有实例的窗口叫出来，而不是开第二个进程
            let _ = commands::window::show_main_inner(app);
        }))
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(SAVED_STATE)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            // 自启时附加标记，让应用知道"这次不该弹窗"
            Some(vec![AUTOSTART_FLAG]),
        ))
        .plugin(shortcuts::build_plugin())
        .setup(move |app| {
            let handle = app.handle().clone();

            let paths = Paths::resolve(&handle)?;
            let state = AppState::new(paths, autostart_run);
            app.manage(state);

            // 托盘先于窗口创建：自启场景下窗口根本不会出现，
            // 托盘是用户唯一的交互入口。
            if let Err(e) = tray::create(&handle) {
                eprintln!("[glassnote] 托盘创建失败：{e}");
            }

            if autostart_run {
                spawn_delayed_init(handle.clone());
            } else {
                // 正常启动：立刻建窗口并显示
                match commands::window::ensure_main(&handle) {
                    Ok(win) => {
                        let _ = win.show();
                        let _ = win.set_focus();
                    }
                    Err(e) => eprintln!("[glassnote] 主窗口创建失败：{e}"),
                }
                if let Err(e) = state_ready(&handle) {
                    eprintln!("[glassnote] 初始化失败：{e}");
                }
            }

            scheduler::spawn(handle.clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            let app = window.app_handle();
            match event {
                // 关闭按钮的行为由设置决定：默认最小化到托盘而不是退出应用。
                // 放在 Rust 侧而不是前端：即使渲染进程卡死或崩溃，
                // 关闭按钮的行为依然可靠。
                WindowEvent::CloseRequested { api, .. } => {
                    let state = app.state::<AppState>();
                    if state.is_quitting() {
                        return; // 确实要退出，放行
                    }
                    let s = state.settings();
                    if s.close_to_tray {
                        api.prevent_close();
                        let _ = commands::window::hide_main_inner(app);
                    } else {
                        state.begin_quit();
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            // 任务
            commands::tasks::list_tasks,
            commands::tasks::create_task,
            commands::tasks::update_task,
            commands::tasks::complete_task,
            commands::tasks::uncomplete_task,
            commands::tasks::skip_task_occurrence,
            commands::tasks::delete_task,
            commands::tasks::restore_task,
            commands::tasks::purge_task,
            commands::tasks::empty_trash,
            commands::tasks::snooze_task,
            commands::tasks::reorder_tasks,
            commands::tasks::set_task_pinned,
            commands::tasks::list_completed_logs,
            commands::tasks::preview_next_occurrence,
            // 备忘录
            commands::memos::list_memos,
            commands::memos::save_memo,
            commands::memos::set_memo_collapsed,
            commands::memos::set_memo_pinned,
            commands::memos::delete_memo,
            commands::memos::restore_memo,
            commands::memos::purge_memo,
            commands::memos::reorder_memos,
            // 设置
            commands::settings::get_settings,
            commands::settings::set_settings,
            commands::settings::get_boot_payload,
            commands::settings::reset_settings,
            commands::settings::set_autostart,
            commands::settings::is_autostart_enabled,
            commands::settings::set_shortcuts,
            commands::settings::set_lock_pin,
            commands::settings::verify_lock_pin,
            commands::settings::relaunch_app,
            // 窗口与玻璃
            commands::window::detect_glass,
            commands::window::apply_glass,
            commands::window::set_opacity,
            commands::window::set_always_on_top,
            commands::window::show_main_window,
            commands::window::hide_main_window,
            commands::window::toggle_main_window,
            commands::window::set_window_collapsed,
            commands::window::open_capture_window,
            commands::window::close_capture_window,
            commands::window::snap_and_clamp,
            commands::window::clamp_to_screen,
            // 数据
            commands::data::data_dir,
            commands::data::open_data_dir,
            commands::data::backup_now,
            commands::data::list_backups,
            commands::data::restore_backup,
            commands::data::export_data,
            commands::data::import_data,
            commands::data::clear_all_data,
            commands::data::run_maintenance,
            // 托盘
            tray::refresh_tray,
        ])
        .build(tauri::generate_context!())
        .expect("GlassNote 初始化失败")
        .run(|app, event| match event {
            // code 为 None 表示是"窗口全关了"触发的退出，而不是显式退出。
            // 开启关闭到托盘时，这种情况不该真的退出应用。
            RunEvent::ExitRequested { api, code, .. } => {
                if code.is_none() {
                    let state = app.state::<AppState>();
                    if state.settings().close_to_tray && !state.is_quitting() {
                        api.prevent_exit();
                    }
                }
            }
            // 退出前把窗口状态落盘：窗口可能早已被销毁（"隐藏时释放内存"），
            // 此时插件的自动保存拿不到窗口，必须在这里显式保存一次，
            // 否则下次启动会回到默认位置。
            RunEvent::Exit => {
                if let Err(e) = app.save_window_state(SAVED_STATE) {
                    eprintln!("[glassnote] 保存窗口状态失败：{e}");
                }
            }
            _ => {}
        });
}

/// 开机自启路径下的延迟初始化。
///
/// 为什么要延迟：开机瞬间磁盘和 CPU 都被系统占满，这时候去开数据库、
/// 跑迁移、查提醒只会让开机更慢，而用户根本感知不到好处。
/// 托盘已经在前面建好了，所以延迟期间应用看起来"已经启动完成"。
fn spawn_delayed_init(app: tauri::AppHandle) {
    std::thread::Builder::new()
        .name("glassnote-delayed-init".into())
        .spawn(move || {
            let delay = {
                let state = app.state::<AppState>();
                state.settings().autostart_delay_sec.max(0) as u64
            };
            if delay > 0 {
                std::thread::sleep(Duration::from_secs(delay));
            }
            if app.state::<AppState>().is_quitting() {
                return;
            }
            if let Err(e) = state_ready(&app) {
                eprintln!("[glassnote] 延迟初始化失败：{e}");
            }
        })
        .expect("无法创建延迟初始化线程");
}

/// 数据库就绪之后的收尾工作：注册快捷键、同步自启状态、刷新托盘。
///
/// 幂等：正常启动与延迟初始化都可能调用，重复执行不产生副作用。
fn state_ready(app: &tauri::AppHandle) -> crate::error::Result<()> {
    let state = app.state::<AppState>();
    state.ensure_db()?;
    let s = state.settings();

    // 全局快捷键必须等数据库就绪后再注册：键位存在设置里
    for f in shortcuts::reregister(app, &s.shortcut_capture, &s.shortcut_toggle) {
        eprintln!("[glassnote] 快捷键注册失败 {f}");
    }

    // 用户可能通过系统"启动应用"设置或安装器改过自启项，
    // 这里把系统真实状态回读一次，避免界面显示与实际不一致
    {
        use tauri_plugin_autostart::ManagerExt;
        let manager = app.autolaunch();
        if let Ok(actual) = manager.is_enabled() {
            if actual != s.autostart {
                if let Ok(db) = state.db() {
                    if let Ok(merged) = db.with(|c| {
                        settings::save_patch(c, &serde_json::json!({ "autostart": actual }))
                    }) {
                        state.set_settings(merged);
                    }
                }
            }
        }
    }

    tray::sync_visibility(app, s.show_tray_icon);
    Ok(())
}
