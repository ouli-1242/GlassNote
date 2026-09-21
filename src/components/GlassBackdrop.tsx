import { forwardRef } from 'react';

import type { GlassInfo } from '../types';

/**
 * 面板分层容器。
 *
 * 层次自下而上：模糊兜底 → 着色 → 描边发光 → 内容。
 * 全部是 `pointer-events: none` 的绝对定位层，不参与布局。
 *
 * 刻意**没有**光斑、噪点、鼠标跟随高光：那三层是这套界面里唯一按帧重绘的东西，
 * 去掉后渲染进程在空闲与交互时都不再产生 GPU 合成工作。
 * 现在唯一的"玻璃"来自系统级的 Mica / Acrylic，由 DWM 画在窗口背后，成本可忽略。
 *
 * `data-effect` 是唯一还起作用的属性：系统玻璃全部不可用时（非 Windows，
 * 或用户关掉了系统的透明效果），CSS 才需要打开 backdrop-filter 兜底。
 */
export const GlassBackdrop = forwardRef<
  HTMLDivElement,
  {
    glass: GlassInfo | null;
    children: React.ReactNode;
  }
>(function GlassBackdrop({ glass, children }, ref) {
  return (
    <div ref={ref} className="glass-root" data-effect={glass?.effect ?? 'css'}>
      {/* 1) CSS 模糊兜底层。只在系统级玻璃不可用时才有内容（见 styles.css），
             因此必须排在着色层**下面** —— 排在上面会把配色冲淡。 */}
      <div className="glass-layer glass-blur" />

      {/* 2) 着色层：承担面板底色与透明度调节 */}
      <div className="glass-layer glass-tint" />

      {/* 3) 1px 描边 + 内侧上缘高光 */}
      <div className="glass-layer glass-edge" />

      <div className="relative z-10 flex h-full flex-col">{children}</div>
    </div>
  );
});
