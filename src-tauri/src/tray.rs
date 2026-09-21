//! 系统托盘。
//!
//! 托盘在开机自启场景下是**唯一**的常驻入口，所以它在窗口之前创建，
//! 而且创建过程很轻（一张 16/32px 位图 + 一个菜单），不会拖慢开机。
//!
//! 徽标是运行时把数字画进图标位图得到的，没有引入图像库：
//! 只需要一个 3x5 的点阵字模，够画 0-99。这比拉一个 `image` + 字体渲染栈
//! 进来省掉了几百 KB 依赖。

use tauri::image::Image;
use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::window;
use crate::error::Result;
use crate::state::AppState;

pub const TRAY_ID: &str = "glassnote-tray";

const MENU_TOGGLE: &str = "toggle";
const MENU_CAPTURE: &str = "capture";
const MENU_COUNT: &str = "count";
const MENU_SETTINGS: &str = "settings";
const MENU_AUTOSTART: &str = "autostart";
const MENU_QUIT: &str = "quit";

/// 3x5 点阵字模，每行 3 位（bit2..bit0）。够画 0-9。
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b111, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b001, 0b001, 0b001], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
];

/// 在右下角画一个带数字的徽标。
///
/// 直接把数字画进图标位图，而不是用第二张图标叠加 ——
/// Windows 托盘不支持多图层，只有一张位图可用。
fn badge_icon(base: &Image<'_>, count: i64) -> Image<'static> {
    let (w, h) = (base.width() as i32, base.height() as i32);
    let mut px = base.rgba().to_vec();

    let text = if count > 99 { "99".to_string() } else { count.to_string() };
    let chars: Vec<usize> = text.chars().filter_map(|c| c.to_digit(10)).map(|d| d as usize).collect();
    if chars.is_empty() {
        return Image::new_owned(px, base.width(), base.height());
    }

    // 徽标圆：直径约占图标 62%，贴右下角
    let d = ((w.min(h) as f32) * 0.62).round() as i32;
    let (cx, cy) = (w - d / 2 - 1, h - d / 2 - 1);
    let r = d as f32 / 2.0;

    for y in 0..h {
        for x in 0..w {
            let dx = (x - cx) as f32;
            let dy = (y - cy) as f32;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > r {
                continue;
            }
            let idx = ((y * w + x) * 4) as usize;
            if idx + 3 >= px.len() {
                continue;
            }
            // 边缘一像素做半透明，避免锯齿过于生硬
            let alpha = if dist > r - 1.0 { 170u8 } else { 255u8 };
            // 红色徽标在任何主题下都清晰可辨
            let (br, bg, bb) = (235u8, 68u8, 72u8);
            let a = alpha as f32 / 255.0;
            px[idx] = (px[idx] as f32 * (1.0 - a) + br as f32 * a) as u8;
            px[idx + 1] = (px[idx + 1] as f32 * (1.0 - a) + bg as f32 * a) as u8;
            px[idx + 2] = (px[idx + 2] as f32 * (1.0 - a) + bb as f32 * a) as u8;
            px[idx + 3] = px[idx + 3].max(alpha);
        }
    }

    // 数字：每个字符 3x5 点，按图标尺寸等比放大
    let scale = (d as f32 / (chars.len() as f32 * 4.0 + 1.0)).floor().max(1.0) as i32;
    let glyph_w = scale * 3;
    let gap = scale;
    let total_w = glyph_w * chars.len() as i32 + gap * (chars.len() as i32 - 1);
    let start_x = cx - total_w / 2;
    let start_y = cy - (scale * 5) / 2;

    for (i, digit) in chars.iter().enumerate() {
        let ox = start_x + i as i32 * (glyph_w + gap);
        for (row, bits) in DIGITS[*digit].iter().enumerate() {
            for col in 0..3 {
                // bit2 是最左列
                if bits & (1 << (2 - col)) == 0 {
                    continue;
                }
                for sy in 0..scale {
                    for sx in 0..scale {
                        let x = ox + col * scale + sx;
                        let y = start_y + row as i32 * scale + sy;
                        if x < 0 || y < 0 || x >= w || y >= h {
                            continue;
                        }
                        let idx = ((y * w + x) * 4) as usize;
                        if idx + 3 >= px.len() {
                            continue;
                        }
                        px[idx] = 255;
                        px[idx + 1] = 255;
                        px[idx + 2] = 255;
                        px[idx + 3] = 255;
                    }
                }
            }
        }
    }

    Image::new_owned(px, base.width(), base.height())
}

/// 未完成任务数（今日视图口径：已到点或没有时间限制的）。
fn pending_count(app: &AppHandle) -> i64 {
    let state = app.state::<AppState>();
    let Ok(db) = state.db() else {
        return 0;
    };
    let now = crate::db::now_ms();
    db.with(|c| {
        Ok(c.query_row(
            "SELECT COUNT(*) FROM tasks
             WHERE deleted_at IS NULL AND status='todo' AND (due_at IS NULL OR due_at <= ?1)",
            rusqlite::params![now],
            |r| r.get(0),
        )?)
    })
    .unwrap_or(0)
}

fn refresh_menu(app: &AppHandle) -> Result<()> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    let state = app.state::<AppState>();
    let settings = state.settings();
    let count = pending_count(app);

    let toggle = MenuItemBuilder::with_id(MENU_TOGGLE, "显示 / 隐藏窗口").build(app)?;
    let capture = MenuItemBuilder::with_id(MENU_CAPTURE, "快速添加任务").build(app)?;
    let count_item = MenuItemBuilder::with_id(MENU_COUNT, format!("今日待办 {count} 项"))
        .enabled(false)
        .build(app)?;
    let settings_item = MenuItemBuilder::with_id(MENU_SETTINGS, "设置…").build(app)?;
    let autostart = CheckMenuItemBuilder::with_id(MENU_AUTOSTART, "开机自动启动")
        .checked(settings.autostart)
        .build(app)?;
    let quit = MenuItemBuilder::with_id(MENU_QUIT, "退出 GlassNote").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&toggle)
        .item(&capture)
        .separator()
        .item(&count_item)
        .separator()
        .item(&settings_item)
        .item(&autostart)
        .separator()
        .item(&quit)
        .build()?;

    tray.set_menu(Some(menu))?;

    // 图标徽标与提示文字同步更新
    if let Some(base) = app.default_window_icon() {
        let icon = if count > 0 { badge_icon(base, count) } else { base.clone() };
        let _ = tray.set_icon(Some(icon));
    }
    let _ = tray.set_tooltip(Some(&format!("GlassNote · 今日待办 {count} 项")));
    Ok(())
}

/// 创建托盘。窗口还没建时也要能建托盘 —— 开机自启就靠它。
pub fn create(app: &AppHandle) -> Result<()> {
    if app.tray_by_id(TRAY_ID).is_some() {
        return refresh_menu(app);
    }

    let toggle = MenuItemBuilder::with_id(MENU_TOGGLE, "显示 / 隐藏窗口").build(app)?;
    let capture = MenuItemBuilder::with_id(MENU_CAPTURE, "快速添加任务").build(app)?;
    let count_item = MenuItemBuilder::with_id(MENU_COUNT, "今日待办 0 项").enabled(false).build(app)?;
    let settings_item = MenuItemBuilder::with_id(MENU_SETTINGS, "设置…").build(app)?;
    let autostart = CheckMenuItemBuilder::with_id(MENU_AUTOSTART, "开机自动启动").build(app)?;
    let quit = MenuItemBuilder::with_id(MENU_QUIT, "退出 GlassNote").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&toggle)
        .item(&capture)
        .separator()
        .item(&count_item)
        .separator()
        .item(&settings_item)
        .item(&autostart)
        .separator()
        .item(&quit)
        .build()?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        // 左键单击直接切换窗口，不弹菜单 —— 这是最常用的操作，少一步
        .show_menu_on_left_click(false)
        .tooltip("GlassNote")
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                let _ = window::toggle_main_inner(app);
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    refresh_menu(app)?;
    Ok(())
}

fn on_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        MENU_TOGGLE => {
            let _ = window::toggle_main_inner(app);
        }
        MENU_CAPTURE => {
            let _ = window::open_capture_inner(app);
        }
        MENU_SETTINGS => {
            // 先确保窗口存在，再让前端打开设置面板
            if window::show_main_inner(app).is_ok() {
                let _ = app.emit_to(window::MAIN, "glassnote://open-settings", ());
            }
        }
        MENU_AUTOSTART => {
            let state = app.state::<AppState>();
            let next = !state.settings().autostart;
            let manager = {
                use tauri_plugin_autostart::ManagerExt;
                app.autolaunch()
            };
            let ok = if next { manager.enable() } else { manager.disable() };
            if ok.is_ok() {
                let actual = manager.is_enabled().unwrap_or(next);
                if let Ok(db) = state.db() {
                    if let Ok(merged) =
                        db.with(|c| crate::settings::save_patch(c, &serde_json::json!({ "autostart": actual })))
                    {
                        state.set_settings(merged);
                    }
                }
                let _ = refresh_menu(app);
            }
        }
        MENU_QUIT => {
            app.state::<AppState>().begin_quit();
            app.exit(0);
        }
        _ => {}
    }
}

/// 显示或隐藏托盘图标（设置项 `showTrayIcon`）。
pub fn sync_visibility(app: &AppHandle, visible: bool) {
    if !visible {
        if let Some(tray) = app.tray_by_id(TRAY_ID) {
            let _ = tray.set_visible(false);
        }
        return;
    }
    if app.tray_by_id(TRAY_ID).is_none() {
        let _ = create(app);
    } else if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_visible(true);
        let _ = refresh_menu(app);
    }
}

/// 任务数据变化后刷新徽标与"今日待办 N 项"。
///
/// 刻意**不**在这里跑维护（备份、清理回收站）：那是每 6 小时一次的重活，
/// 挂在每次任务增删改后面会让一次简单的打勾也去查一遍全表。
/// 维护由调度器按间隔触发。
#[tauri::command]
pub fn refresh_tray(app: AppHandle) -> Result<()> {
    refresh_menu(&app)
}
