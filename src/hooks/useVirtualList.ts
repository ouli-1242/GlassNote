/**
 * 极简虚拟滚动。
 *
 * 为什么手写而不引 @tanstack/react-virtual：
 *   任务行是**固定高度**的（--row-h = 56px，见 styles.css），
 *   固定高度下虚拟化的数学只有一次除法和一次取整，不需要测量、不需要
 *   ResizeObserver、不需要处理动态尺寸的边界情况。为此多引一个依赖不划算。
 *
 * 为什么要有阈值：
 *   列表短的时候全部渲染更快（省掉一次 scroll 监听与重排），
 *   而且几十个 DOM 节点的开销本来就可以忽略。只有长列表才值得虚拟化。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

interface Options {
  /** 单行高度（px），必须与实际 CSS 高度严格一致，否则会出现滚动跳动 */
  itemHeight: number;
  /** 上下各多渲染几行，避免快速滚动时露白 */
  overscan?: number;
  /** 少于此数量时不做虚拟化 */
  threshold?: number;
}

export function useVirtualList<T>(items: T[], { itemHeight, overscan = 6, threshold = 80 }: Options) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportH, setViewportH] = useState(0);

  const virtual = items.length > threshold;

  // 只在需要虚拟化时才挂 scroll 监听
  useEffect(() => {
    const el = containerRef.current;
    if (!el || !virtual) return;

    let raf = 0;
    const onScroll = () => {
      if (raf) return; // 用 rAF 合帧：一次滚动会触发几十个 scroll 事件，不能逐个 setState
      raf = requestAnimationFrame(() => {
        raf = 0;
        setScrollTop(el.scrollTop);
      });
    };

    const ro = new ResizeObserver(() => setViewportH(el.clientHeight));
    ro.observe(el);
    setViewportH(el.clientHeight);
    setScrollTop(el.scrollTop);

    el.addEventListener('scroll', onScroll, { passive: true });
    return () => {
      el.removeEventListener('scroll', onScroll);
      ro.disconnect();
      if (raf) cancelAnimationFrame(raf);
    };
  }, [virtual]);

  const { start, end, offsetY, totalHeight } = useMemo(() => {
    if (!virtual) {
      return { start: 0, end: items.length, offsetY: 0, totalHeight: 0 };
    }
    const total = items.length * itemHeight;
    const visible = Math.ceil((viewportH || 0) / itemHeight);
    const rawStart = Math.floor(scrollTop / itemHeight) - overscan;
    const s = Math.max(0, rawStart);
    const e = Math.min(items.length, s + visible + overscan * 2);
    return { start: s, end: e, offsetY: s * itemHeight, totalHeight: total };
  }, [virtual, items.length, itemHeight, scrollTop, viewportH, overscan]);

  const scrollToIndex = useCallback(
    (index: number) => {
      const el = containerRef.current;
      if (!el || !virtual) return;
      el.scrollTop = index * itemHeight;
    },
    [virtual, itemHeight],
  );

  return {
    containerRef,
    virtual,
    start,
    end,
    offsetY,
    totalHeight,
    /** 传给容器内层做占位撑高 */
    spacerStyle: virtual ? { height: totalHeight, position: 'relative' as const } : undefined,
    innerStyle: virtual ? { transform: `translateY(${offsetY}px)` } : undefined,
    visible: virtual ? items.slice(start, end) : items,
    scrollToIndex,
  };
}
