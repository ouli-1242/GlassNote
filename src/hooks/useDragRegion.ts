/**
 * 窗口原生拖动。
 *
 * 为什么不用 `data-tauri-drag-region`：
 *   Tauri 内建的拖动区只能标在具体元素上，而规格要求"任意空白处都能拖"。
 *   如果给整个根元素打上属性，又会把按钮、输入框、滚动条一起圈进去。
 *   更关键的是内建机制无法排除"按在滚动条上"这种场景 ——
 *   在 360px 宽的窄面板里误触概率很高。
 *
 * 为什么不用 JS 逐帧 setPosition：
 *   那是用 JS 模拟拖动，每帧一次 IPC，必然掉帧且吃 CPU。
 *   这里走的是 `startDragging()`，底层是 ReleaseCapture + WM_NCLBUTTONDOWN，
 *   由系统窗口管理器接管移动循环 —— 跟手程度与拖动原生标题栏完全一致，JS 全程零参与。
 *
 * 代价与取舍：`startDragging()` 会阻塞 JS 线程直到拖动结束。
 * 所以这里**不**在调用前等 rAF 让样式生效 —— 那会给每次拖动开始加约 32ms 延迟，
 * 与"跟手无延迟"直接冲突。暂停动态层的样式变更照常设置，
 * 浏览器会在拖动过程中自行完成重算。
 */

import { useEffect } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';

import { snapAndClamp } from '../lib/ipc';

/**
 * 这些元素上按下鼠标时**不**启动窗口拖动。
 *
 * 用角色选择器而不是类名：任何语义上的可交互控件都会被覆盖，
 * 新写的组件只要用了正确的语义标签就自动安全，不需要记得加标记。
 */
const INTERACTIVE = [
  'button',
  'a',
  'input',
  'textarea',
  'select',
  'label',
  '[contenteditable="true"]',
  '[role="button"]',
  '[role="switch"]',
  '[role="slider"]',
  '[role="tab"]',
  '[role="checkbox"]',
  '[role="menuitem"]',
  '[role="dialog"]',
  '[data-no-drag]',
  // 任务/备忘录的拖拽排序手柄：这里的拖动属于排序，不是移动窗口
  '[data-drag-handle]',
].join(',');

/**
 * 判断指针是否落在滚动条上。
 *
 * 在 Chromium 里按滚动条同样会触发元素上的 mousedown，
 * 若不排除，用户想滚动列表却把整个窗口拖走了 —— 这是这类无边框面板最常见的抱怨。
 */
function isOnScrollbar(e: MouseEvent): boolean {
  // closest 的返回类型是 Element，但滚动条尺寸只在 HTMLElement 上才有
  const el = (e.target as HTMLElement | null)?.closest<HTMLElement>('.scroll-area');
  if (!el) return false;
  const rect = el.getBoundingClientRect();
  const sbWidth = el.offsetWidth - el.clientWidth;
  const sbHeight = el.offsetHeight - el.clientHeight;
  if (sbWidth > 0 && e.clientX >= rect.left + el.clientWidth) return true;
  if (sbHeight > 0 && e.clientY >= rect.top + el.clientHeight) return true;
  return false;
}

export function useDragRegion(enabled = true) {
  useEffect(() => {
    if (!enabled) return;

    const onMouseDown = (e: MouseEvent) => {
      // 只响应左键；右键/中键交给系统菜单与默认行为
      if (e.button !== 0) return;
      if (e.detail > 1) return; // 双击由标题栏自己处理折叠，不要拖

      const target = e.target as HTMLElement | null;
      if (!target) return;
      if (target.closest(INTERACTIVE)) return;
      if (isOnScrollbar(e)) return;

      document.body.classList.add('is-dragging');

      // 立刻调用，不做任何 await —— 任何延迟都会体现在拖动手感上
      const win = getCurrentWindow();
      void win
        .startDragging()
        .catch(() => {
          /* 拖动被系统拒绝（例如窗口已销毁）时静默忽略 */
        })
        .finally(() => {
          document.body.classList.remove('is-dragging');
          // 松手后做一次边缘吸附与多显示器位置校正
          void snapAndClamp().catch(() => {});
        });
    };

    // 用捕获阶段：即使某个组件在冒泡阶段 stopPropagation，窗口仍然可拖
    document.addEventListener('mousedown', onMouseDown, true);
    return () => document.removeEventListener('mousedown', onMouseDown, true);
  }, [enabled]);
}
