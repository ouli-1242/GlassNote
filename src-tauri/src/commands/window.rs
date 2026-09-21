//! 窗口命令：创建/显示/隐藏、玻璃效果、透明度、置顶、折叠、边缘吸附。
//!
//! ## 为什么主窗口不在 tauri.conf.json 里声明
//!
//! Tauri 会在启动时自动创建配置里声明的窗口 —— 也就是说 WebView2 会被立刻拉起，
//! 立刻吃掉几十 MB 内存。而本应用的首要目标是"空闲时几乎不占内存"，
//! 开机自启场景下更是根本不需要窗口。
//!
//! 所以窗口完全由代码按需创建：
//!   - 正常启动 → 创建并显示；
//!   - 开机自启 → 只建托盘，不建窗口，内存占用只有 Rust 进程本身；
//!   - 开启"隐藏时释放内存" → 隐藏时销毁窗口，再次显示时重建。
//!
//! 重建走 `WebviewWindowBuilder`，参数与首次创建完全一致，因此
//! 位置/尺寸由 tauri-plugin-window-state 自动恢复，用户无感。

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_window_state::AppHandleExt;

use crate::error::Result;
use crate::glass;
use crate::models::GlassInfo;
use crate::settings;
use crate::state::AppState;

pub const MAIN: &str = "main";
pub const CAPTURE: &str = "capture";

/// 默认尺寸与最小尺寸，和规格一致
const W: f64 = 400.0;
const H: f64 = 560.0;
const MIN_W: f64 = 300.0;
const MIN_H: f64 = 400.0;

/// 折叠后只留顶栏，窗口高度收到这个值
const COLLAPSED_H: f64 = 40.0;

/// 边缘吸附的触发距离
const SNAP_PX: i32 = 14;

fn boot_script(app: &AppHandle) -> String {
    // 优先用引导缓存（不依赖数据库），没有再用数据库里的设置
    let payload = settings::load_boot(app).unwrap_or_else(|| {
        let state = app.state::<AppState>();
        settings::BootPayload::from_settings(&state.settings(), state.paths.portable, state.autostart_run)
    });
    payload.to_script()
}

/// WebView2 额外的进程参数。
///
/// 实测结论（见 _setup/ab-memory.py）：
///   多进程（基线）            96.6 MB
///   单进程 + 关后台             68.2 MB   ← 省 28MB / -29%
///
/// 浏览器进程本身在 Chromium 里就是单进程，所以单进程 = 把渲染器/GPU/utility
/// 等子进程都合进同一个 msedgewebview2.exe，省掉进程间的重复分配开销。
/// 风险是单进程下一个渲染器崩溃会带走整个 WebView2（多进程模式只有渲染器死，
/// 浏览器能复活）。本应用界面已静态化（无持续动画），崩溃面已经很小。
///
/// wry 的 `additional_browser_args` 整体替换其默认参数，所以这里必须把
/// wry 默认禁用的小功能（msWebOOUI/msPdfOOUI/msSmartScreenProtection）
/// 一并带上，否则丢失。
const WEBVIEW2_FLAGS: &str = concat!(
    "--single-process",
    " --disable-features=",
    "msWebOOUI,msPdfOOUI,msSmartScreenProtection,",
    "Translate,BackForwardCache,AcceptCHFrame,MediaRouter,OptimizationHints",
);

/// 创建主窗口。已存在则直接返回。
pub fn ensure_main(app: &AppHandle) -> Result<WebviewWindow> {
    if let Some(w) = app.get_webview_window(MAIN) {
        return Ok(w);
    }

    let win = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html".into()))
        .title("GlassNote")
        .inner_size(W, H)
        .min_inner_size(MIN_W, MIN_H)
        // 无边框 + 透明：圆角与阴影交给 DWM（见 glass.rs 的说明），
        // 用 CSS 画阴影需要在窗口里留透明边距，那样 Mica 会在四角透出来变成方块
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .resizable(true)
        .visible(false)
        .center()
        // 关掉 Tauri 的拖放接管：它会在 Windows 上拦截拖拽事件，
        // 导致前端 HTML5 拖拽排序失效。窗口拖动本身走 startDragging()，不受影响。
        .disable_drag_drop_handler()
        // 内存优化：单进程 + 关后台特性，实测从 96MB 降到 68MB。
        // 见 WEBVIEW2_FLAGS 的注释。
        .additional_browser_args(WEBVIEW2_FLAGS)
        .initialization_script(&boot_script(app))
        .build()?;

    apply_glass_to(app, &win);
    Ok(win)
}

/// 按当前设置把玻璃效果与置顶状态应用到某个窗口。
pub fn apply_glass_to(app: &AppHandle, win: &WebviewWindow) {
    let state = app.state::<AppState>();
    let s = state.settings();
    match glass::apply(win, s.theme == "dark") {
        Ok(info) => {
            let _ = app.emit("glassnote://glass-changed", info);
        }
        Err(e) => eprintln!("[glassnote] 应用玻璃效果失败：{e}"),
    }
    let _ = win.set_always_on_top(s.always_on_top);
}

// ─────────────────────────────── 显示 / 隐藏 ───────────────────────────────

/// 显示主窗口。必要时先创建。
pub fn show_main_inner(app: &AppHandle) -> Result<()> {
    let win = ensure_main(app)?;
    win.show()?;
    win.unminimize().ok();
    win.set_focus()?;
    Ok(())
}

pub fn hide_main_inner(app: &AppHandle) -> Result<()> {
    let Some(win) = app.get_webview_window(MAIN) else {
        return Ok(());
    };

    let s = app.state::<AppState>().settings();
    if s.release_memory_when_hidden {
        // 销毁而不是隐藏：WebView2 的内存只有在窗口真正关闭后才会释放。
        //
        // 销毁前必须显式保存窗口状态 —— window-state 插件是在"应用退出"和
        // "窗口关闭事件"时保存的，而 destroy() 绕过常规关闭流程，
        // 不手动保存的话下次启动会回到默认位置。
        let _ = app.save_window_state(tauri_plugin_window_state::StateFlags::POSITION.union(
            tauri_plugin_window_state::StateFlags::SIZE,
        ));
        win.destroy()?;
    } else {
        win.hide()?;
    }
    Ok(())
}

pub fn toggle_main_inner(app: &AppHandle) -> Result<()> {
    match app.get_webview_window(MAIN) {
        Some(w) if w.is_visible().unwrap_or(false) => hide_main_inner(app),
        _ => show_main_inner(app),
    }
}

/// 快速捕获浮窗：一个独立的、置顶的、无边框小窗口。
pub fn open_capture_inner(app: &AppHandle) -> Result<()> {
    if let Some(w) = app.get_webview_window(CAPTURE) {
        w.show()?;
        w.set_focus()?;
        return Ok(());
    }

    let win = WebviewWindowBuilder::new(app, CAPTURE, WebviewUrl::App("capture.html".into()))
        .title("快速添加")
        .inner_size(420.0, 76.0)
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .center()
        .disable_drag_drop_handler()
        // 与主窗口用同一组进程参数：避免另起一套多进程环境
        // —— 那里为了"省内存 + 一致性"已经统一为单进程。
        .additional_browser_args(WEBVIEW2_FLAGS)
        .initialization_script(&boot_script(app))
        .build()?;

    apply_glass_to(app, &win);
    win.show()?;
    win.set_focus()?;
    Ok(())
}

pub fn close_capture_inner(app: &AppHandle) -> Result<()> {
    if let Some(w) = app.get_webview_window(CAPTURE) {
        w.destroy()?;
    }
    Ok(())
}

// ─────────────────────────────── 边缘吸附与位置校正 ───────────────────────────────

/// 拖动结束后调用：先做边缘吸附，再把窗口拉回可见区域。
///
/// 吸附只吸附到**当前显示器工作区**的边，而不是虚拟桌面的边 ——
/// 多显示器时按虚拟桌面吸附会让窗口跳到别的屏幕去。
pub fn snap_and_clamp_inner(app: &AppHandle) -> Result<()> {
    let Some(win) = app.get_webview_window(MAIN) else {
        return Ok(());
    };

    let pos = win.outer_position()?;
    let size = win.outer_size()?;
    let (mut x, mut y) = (pos.x, pos.y);
    let (mut w, mut h) = (size.width as i32, size.height as i32);

    // 找窗口中心所在的显示器；找不到就退回主显示器
    let monitor = win
        .monitor_from_point(x as f64 + w as f64 / 2.0, y as f64 + h as f64 / 2.0)?
        .or(win.current_monitor()?)
        .or(win.primary_monitor()?);

    let Some(monitor) = monitor else {
        return Ok(());
    };

    // work_area 已排除任务栏，所以吸附后不会被任务栏盖住
    let area = monitor.work_area();
    let (ax, ay) = (area.position.x, area.position.y);
    let (aw, ah) = (area.size.width as i32, area.size.height as i32);

    // 吸附
    if (x - ax).abs() < SNAP_PX {
        x = ax;
    } else if ((ax + aw) - (x + w)).abs() < SNAP_PX {
        x = ax + aw - w;
    }
    if (y - ay).abs() < SNAP_PX {
        y = ay;
    } else if ((ay + ah) - (y + h)).abs() < SNAP_PX {
        y = ay + ah - h;
    }

    // 校正：窗口不能比工作区大，也不能完全跑到屏幕外
    if w > aw {
        w = aw;
    }
    if h > ah {
        h = ah;
    }
    x = x.clamp(ax, ax + aw - w);
    y = y.clamp(ay, ay + ah - h);

    if (x, y) != (pos.x, pos.y) {
        win.set_position(tauri::PhysicalPosition::new(x, y))?;
    }
    if (w as u32, h as u32) != (size.width, size.height) {
        win.set_size(tauri::PhysicalSize::new(w as u32, h as u32))?;
    }
    Ok(())
}

// ─────────────────────────────── 命令封装 ───────────────────────────────

/// 探测并应用玻璃效果。
///
/// 作用于**调用它的那个窗口**（Tauri 会把调用方的 WebviewWindow 注入进来），
/// 而不是写死主窗口 —— 快速捕获浮窗需要拿到属于它自己的效果信息，
/// 否则系统玻璃不可用时它会退化成一块没有任何模糊的透明板。
#[tauri::command]
pub fn detect_glass(app: AppHandle, window: WebviewWindow) -> Result<GlassInfo> {
    let dark = is_dark(&app);
    glass::apply(&window, dark)
}

/// 重新应用玻璃效果。主题改变后必须调一次 ——
/// Mica 的明暗是窗口属性，不会因为前端换了 CSS 变量就跟着变。
#[tauri::command]
pub fn apply_glass(app: AppHandle, window: WebviewWindow) -> Result<GlassInfo> {
    let dark = is_dark(&app);
    glass::apply(&window, dark)
}

/// 当前是否暗色。设置里的主题已经收敛到 "dark" / "light" 两值，
/// 这里只做一次字符串比较，不再有"跟随系统"的分支。
fn is_dark(app: &AppHandle) -> bool {
    app.state::<AppState>().settings().theme == "dark"
}

/// 透明度只改 CSS 变量，不动窗口本身的 alpha。
///
/// 用 `set_opacity` 调整个窗口会让文字一起变淡，20% 时根本没法读；
/// 改 CSS 变量则只影响背景着色层，文字始终保持满不透明度。
#[tauri::command]
pub fn set_opacity(app: AppHandle, value: f64) -> Result<()> {
    if let Some(win) = app.get_webview_window(MAIN) {
        let _ = win.emit("glassnote://opacity", value.clamp(0.2, 1.0));
    }
    Ok(())
}

#[tauri::command]
pub fn set_always_on_top(app: AppHandle, on: bool) -> Result<()> {
    if let Some(win) = app.get_webview_window(MAIN) {
        win.set_always_on_top(on)?;
    }
    Ok(())
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) -> Result<()> {
    show_main_inner(&app)
}

#[tauri::command]
pub fn hide_main_window(app: AppHandle) -> Result<()> {
    hide_main_inner(&app)
}

#[tauri::command]
pub fn toggle_main_window(app: AppHandle) -> Result<()> {
    toggle_main_inner(&app)
}

#[tauri::command]
pub fn open_capture_window(app: AppHandle) -> Result<()> {
    open_capture_inner(&app)
}

#[tauri::command]
pub fn close_capture_window(app: AppHandle) -> Result<()> {
    close_capture_inner(&app)
}

#[tauri::command]
pub fn snap_and_clamp(app: AppHandle) -> Result<()> {
    snap_and_clamp_inner(&app)
}

/// 展开态窗口高度的存储键。
///
/// 刻意不放进 `AppSettings`：它是窗口几何状态，与用户可见的"设置项"不是一类东西，
/// 而且 tauri-plugin-window-state 已经在管位置和尺寸了 —— 这里只需要额外记住
/// "折叠之前有多高"，因为折叠会把窗口尺寸改小，插件保存到的就是折叠后的尺寸。
const EXPANDED_H_KEY: &str = "expandedHeight";

fn read_expanded_height(app: &AppHandle) -> f64 {
    app.state::<AppState>()
        .db()
        .ok()
        .and_then(|db| {
            db.with(|c| {
                Ok(c.query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    [EXPANDED_H_KEY],
                    |r| r.get::<_, String>(0),
                )
                .ok())
            })
            .ok()
            .flatten()
        })
        .and_then(|raw| serde_json::from_str::<f64>(&raw).ok())
        .unwrap_or(H)
}

fn write_expanded_height(app: &AppHandle, h: f64) -> Result<()> {
    // 必须先绑定 state：app.state() 返回的是临时值，
    // 直接 `.db()` 会让返回的引用指向一个已经被丢弃的临时对象。
    let state = app.state::<AppState>();
    let db = state.db()?;
    db.with(|c| {
        c.execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![EXPANDED_H_KEY, serde_json::to_string(&h)?],
        )?;
        Ok(())
    })
}

/// 折叠 / 展开面板。
///
/// 折叠时把窗口高度收到只剩顶栏，并记住展开前的高度，
/// 这样展开时能回到用户原本的尺寸，而不是弹回默认的 560。
#[tauri::command]
pub fn set_window_collapsed(app: AppHandle, collapsed: bool) -> Result<()> {
    let Some(win) = app.get_webview_window(MAIN) else {
        return Ok(());
    };
    let scale = win.scale_factor().unwrap_or(1.0);
    let cur_h = win.outer_size()?.height as f64 / scale;

    if collapsed {
        // 只在从"展开态"进入折叠时记录，重复调用不会把折叠后的高度当成展开高度
        if cur_h > COLLAPSED_H + 1.0 {
            let _ = write_expanded_height(&app, cur_h);
        }
        // 折叠态允许窗口比常规最小高度更矮，否则 set_size 会被最小尺寸挡住
        win.set_min_size(Some(tauri::LogicalSize::new(MIN_W, COLLAPSED_H)))?;
        win.set_size(tauri::LogicalSize::new(W, COLLAPSED_H))?;
    } else {
        win.set_min_size(Some(tauri::LogicalSize::new(MIN_W, MIN_H)))?;
        let h = read_expanded_height(&app).max(MIN_H);
        win.set_size(tauri::LogicalSize::new(W, h))?;
    }
    Ok(())
}

/// 把窗口拉回当前显示器可见区域（多显示器拔插后调用）。
#[tauri::command]
pub fn clamp_to_screen(app: AppHandle) -> Result<()> {
    snap_and_clamp_inner(&app)
}
