//! 玻璃效果（系统级）。
//!
//! 界面里的"玻璃"只有这一层：Mica / Tabbed / Acrylic / Blur，由 DWM 绘制在窗口背后，
//! 并负责把窗口裁成圆角。它几乎不耗 GPU，是静态的。
//!
//! 前端只保留一个平面着色层（见 styles.css）。**刻意没有**流动光斑、噪点、
//! 鼠标跟随高光那几层 —— 它们是这套界面里唯一按帧重绘的东西，
//! 去掉之后渲染进程在空闲与交互时都不再产生 GPU 合成工作。
//!
//! ## 为什么不用 Tauri 的 `WebviewWindow::set_effects`
//!
//! 看起来那是"官方推荐"的入口，但读一下 tauri 2.11 的 `src/vibrancy/windows.rs`
//! 就会发现它在 Windows 上不可用：
//!
//! ```ignore
//! pub fn apply_effects(window: impl HasWindowHandle, effects: WindowEffectsConfig) {
//!   let WindowEffectsConfig { effects, color, .. } = effects;   // radius 被丢弃
//!   let effect = effects.iter().find(|e| matches!(e, Effect::Mica | ...)).unwrap();
//!   match effect {
//!     Effect::Mica => window_vibrancy::apply_mica(window, None),  // 返回值被丢弃
//!     ...
//!   };
//! }
//! ```
//!
//! 三个后果：
//!   1. 返回值是 `()`，外层永远 `Ok(())` —— **无法知道哪个效果真的生效了**；
//!   2. 它只取候选列表里第一个匹配项，失败就什么都不做，**没有降级链**。
//!      Windows 10 上 Mica 失败 = 完全没有玻璃效果，而不是退到 Acrylic；
//!   3. `radius` 被解构丢弃，**不做 DWM 圆角**。
//!
//! 所以这里直接调 `window-vibrancy`，靠 `Err` 判断来自己实现降级链。

use tauri::WebviewWindow;

use crate::error::Result;
use crate::models::GlassInfo;

/// 窗口圆角由 DWM 负责，**不是**前端 CSS。
///
/// ## 为什么不用 CSS 画 20px 圆角
///
/// Windows 的窗口圆角只有 `DWMWA_WINDOW_CORNER_PREFERENCE` 一个入口，
/// 而它只提供固定档位（DEFAULT / DONOTROUND / ROUND / ROUNDSMALL），
/// **没有自定义半径**。Win11 的 ROUND 约 8px、ROUNDSMALL 约 4px。
///
/// 如果 CSS 画 20px、DWM 裁 8px，那么 8~20px 这一圈：DWM 认为在窗口外（不画背景），
/// CSS 认为在面板外（不画着色）—— 结果就是四角露出一圈桌面，比圆角小更难看。
///
/// 反过来，只要 **CSS 半径 ≤ DWM 半径**，CSS 的圆角就完全落在 DWM 裁切区内，
/// 不会产生任何缝隙。所以这里让前端按探测结果取 8px（DWM 已裁剪）或 0（未裁剪）。
///
/// 用 `SetWindowRgn` 可以做出任意半径，但代价是：每次窗口尺寸变化都要重设区域，
/// 而且区域是 1 位掩码、圆角没有抗锯齿。为了一个半径数字引入这两个问题不划算。
pub const DWM_ROUNDED_RADIUS_PX: f64 = 8.0;

/// 未裁剪时前端应使用的半径。
pub const NO_ROUNDING_RADIUS_PX: f64 = 0.0;

// ─────────────────────────────── DWM 圆角 ───────────────────────────────
//
// Mica/Acrylic 与"圆角"是两件独立的事：
//   - 前者由 window-vibrancy 设置 backdrop 类型；
//   - 后者由 DWM 的 DWMWA_WINDOW_CORNER_PREFERENCE 控制。
//
// 不设圆角的话，系统背景会填满整个**矩形**窗口，而 CSS 画的圆角之外是透明的，
// 于是四角透出方块，圆角面板直接废掉。
//
// 这里用极小的 FFI 直调 dwmapi，不引入 windows / windows-sys 依赖：
// 需要的只有一个函数和一个常量。
#[cfg(windows)]
mod dwm {
    use std::ffi::c_void;

    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: *mut c_void,
            attr: u32,
            value: *const c_void,
            size: u32,
        ) -> i32;
    }

    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWCP_ROUND: u32 = 2;

    /// 返回是否成功。Windows 10 不认这个属性会返回非 0，调用方忽略即可。
    pub fn set_round_corners(hwnd: *mut c_void) -> bool {
        let pref: u32 = DWMWCP_ROUND;
        unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &pref as *const u32 as *const c_void,
                4,
            ) == 0
        }
    }
}

#[cfg(windows)]
fn apply_rounding(window: &WebviewWindow) -> bool {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(h)) => dwm::set_round_corners(h.hwnd.get() as *mut std::ffi::c_void),
        _ => false,
    }
}

#[cfg(not(windows))]
fn apply_rounding(_window: &WebviewWindow) -> bool {
    false
}

// ─────────────────────────────── 效果应用 ───────────────────────────────

/// 清掉所有可能残留的效果。
///
/// 必须在套新效果前调用：Mica 与 Acrylic 同时开着会让背景明显发浑，
/// 而且切档位时会看到上一次的残留。
fn clear_all(window: &WebviewWindow) {
    let _ = window_vibrancy::clear_mica(window);
    let _ = window_vibrancy::clear_tabbed(window);
    let _ = window_vibrancy::clear_acrylic(window);
    let _ = window_vibrancy::clear_blur(window);
}

/// 按 Mica → Tabbed → Acrylic → Blur 的顺序探测并应用，返回实际生效的那个。
///
/// `dark` 只影响 Mica/Tabbed 的着色方向（它们有明暗两种变体）；
/// Acrylic/Blur 传 `None` 让系统用默认染色，整体色调交给前端的着色层统一控制，
/// 避免同一套配色在 Win10/Win11 上出现两种观感。
pub fn apply(window: &WebviewWindow, dark: bool) -> Result<GlassInfo> {
    clear_all(window);
    let rounded = apply_rounding(window);

    // 逐个试，靠 Err 判断 —— 这正是 Tauri 的 set_effects 做不到的事。
    // window-vibrancy 内部会检查系统版本，Win10 上调 apply_mica 必然返回 Err，
    // 于是自然落到 Acrylic。
    let effect = if window_vibrancy::apply_mica(window, Some(dark)).is_ok() {
        "mica"
    } else if window_vibrancy::apply_tabbed(window, Some(dark)).is_ok() {
        "tabbed"
    } else if window_vibrancy::apply_acrylic(window, None).is_ok() {
        "acrylic"
    } else if window_vibrancy::apply_blur(window, None).is_ok() {
        "blur"
    } else {
        // 全部失败：交给前端用纯 CSS 半透明兜底。
        // 会走到这里的情况：非 Windows 平台，或用户关掉了系统的"透明效果"。
        "css"
    };

    Ok(GlassInfo {
        effect: effect.to_string(),
        // 只有 Mica 系列是 Windows 11 独有；Acrylic/Blur 在 Win10 v1809+ 也有。
        // 判定依据是"哪个效果真的成功了"，而不是去读系统版本号 ——
        // Win11 上关掉透明效果后 Mica 同样会失败，此时报"降级"才是对用户可见结果的准确描述。
        is_windows_11: matches!(effect, "mica" | "tabbed"),
        rounded,
        corner_radius: if rounded { DWM_ROUNDED_RADIUS_PX } else { NO_ROUNDING_RADIUS_PX },
    })
}
