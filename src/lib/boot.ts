/**
 * 引导数据。
 *
 * Rust 在创建窗口时通过 `initialization_script` 注入 `window.__GLASSNOTE_BOOT__`，
 * 它在页面任何脚本之前执行，所以这里读到的是**同步**可用的值 ——
 * 不需要等 IPC，因此首帧就能用正确的主题与透明度渲染，不会闪白。
 *
 * 为什么不用 tauri-plugin-store 在 JS 侧读：那是异步的，
 * 等它回来时窗口已经用默认样式画过一帧了。
 */

import { DEFAULT_SETTINGS, type BootPayload, type ThemeMode } from '../types';

/**
 * 把任意来源的主题值收敛到两套之一。
 *
 * 需要它是因为两处输入都不可信：
 *   - localStorage 里的旧版本镜像可能还留着 `"system"`；
 *   - 引导缓存文件在版本升级后不会自动清理。
 * 少了这一步，旧值会让 `classList` 两边的 class 都没加上，
 * 整套 CSS 变量取不到值 —— 表现是窗口一片透明。
 */
function normalizeTheme(v: unknown): ThemeMode {
  return v === 'dark' ? 'dark' : 'light';
}

export function getBoot(): BootPayload {
  const injected = window.__GLASSNOTE_BOOT__;
  if (injected) return { ...injected, theme: normalizeTheme(injected.theme) };

  // 兜底：直接打开 vite dev server 调试时没有注入，用 localStorage 或默认值
  try {
    const raw = localStorage.getItem('glassnote.boot');
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<BootPayload>;
      return {
        ...DEFAULT_SETTINGS,
        portable: false,
        autostartRun: false,
        windowCollapsed: false,
        ...parsed,
        theme: normalizeTheme(parsed.theme),
      } as BootPayload;
    }
  } catch {
    /* 忽略 */
  }

  return {
    theme: DEFAULT_SETTINGS.theme,
    opacity: DEFAULT_SETTINGS.opacity,
    fontScale: DEFAULT_SETTINGS.fontScale,
    portable: false,
    autostartRun: false,
    windowCollapsed: false,
  };
}

/**
 * 把引导数据落到 DOM。
 *
 * 必须早于 React 挂载执行：这些是纯 CSS 变量与 class，
 * 直接改 DOM 比等 React 渲染一棵树再应用要快一帧。
 */
export function applyBoot(boot: BootPayload) {
  const root = document.documentElement;
  const dark = boot.theme === 'dark';
  root.classList.toggle('dark', dark);
  root.classList.toggle('light', !dark);

  root.style.setProperty('--panel-opacity', String(boot.opacity));
  root.style.setProperty('--font-scale', String(boot.fontScale));
}
