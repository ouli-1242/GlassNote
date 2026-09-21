import {
  ChevronDown,
  ChevronUp,
  Minus,
  Moon,
  Pin,
  PinOff,
  Settings as SettingsIcon,
  Sun,
} from 'lucide-react';
import { useEffect, useState } from 'react';

import * as ipc from '../lib/ipc';
import { cn } from '../lib/utils';
import { useStore } from '../store/useStore';
import type { ThemeMode } from '../types';
import { Button } from './ui/Button';
import { Segmented, Slider } from './ui/Controls';

/**
 * 顶栏。
 *
 * 拖动：整条顶栏本身不标 data-no-drag，所以按住它任意位置都能拖窗口
 * （由 useDragRegion 统一接管）。里面的每个按钮都通过 Button 组件自动带上
 * data-no-drag，因此点按钮不会变成拖窗口。
 *
 * 双击折叠：规格要求"双击顶栏折叠/展开"。折叠的是**整个面板**——
 * 只留顶栏高度，窗口也随之收缩，等价于一个迷你状态栏。
 */
export function TitleBar() {
  const settings = useStore((s) => s.settings);
  const updateSettings = useStore((s) => s.updateSettings);
  const setSettingsOpen = useStore((s) => s.setSettingsOpen);
  const tasks = useStore((s) => s.tasks);
  const view = useStore((s) => s.view);

  const [showQuick, setShowQuick] = useState(false);

  // 折叠状态同时要落库并通知 Rust 调整窗口高度，否则窗口还是原尺寸、下面留一片空白
  useEffect(() => {
    void ipc.setWindowCollapsed(settings.windowCollapsed).catch(() => {});
  }, [settings.windowCollapsed]);

  const pendingCount = tasks.filter((t) => t.status === 'todo').length;

  return (
    <header
      className={cn(
        'relative z-20 flex h-10 shrink-0 items-center gap-1 px-2.5',
        'border-b border-[hsl(var(--border)/0.4)]',
        // 顶栏空白处双击折叠；按钮上不会触发，因为它们的点击被 no-drag 与 stopPropagation 拦下
        'cursor-default',
      )}
      onDoubleClick={(e) => {
        // 只在顶栏自身或非交互子元素上响应双击
        if ((e.target as HTMLElement).closest('[data-no-drag]')) return;
        void updateSettings({ windowCollapsed: !settings.windowCollapsed });
      }}
      title="按住拖动窗口 · 双击折叠"
    >
      <img src="/icon.png" alt="" className="h-4 w-4 shrink-0 rounded-[4px]" draggable={false} />

      <span className="ml-0.5 text-[13px] font-semibold tracking-tight">GlassNote</span>

      {pendingCount > 0 && view !== 'done' && view !== 'trash' ? (
        <span className="ml-1 rounded-full bg-[hsl(var(--accent)/0.2)] px-1.5 py-[1px] text-[10px] font-medium text-[hsl(var(--accent))]">
          {pendingCount}
        </span>
      ) : null}

      {/* 中间留白：既是视觉分隔，也是最好用的拖动区 */}
      <div className="min-w-0 flex-1" />

      {/* 外观快捷调节：透明度 + 主题 */}
      <div className="relative" data-no-drag>
        <Button
          variant="ghost"
          size="icon-sm"
          title="外观"
          onClick={() => setShowQuick((v) => !v)}
          className={cn(showQuick && 'bg-[hsl(var(--surface)/0.7)]')}
        >
          {settings.theme === 'dark' ? <Moon size={13} /> : <Sun size={13} />}
        </Button>
        {showQuick ? (
          <div
            className="absolute right-0 top-7 z-30 w-44 anim-scale-in rounded-[12px] border border-[hsl(var(--border)/0.6)] bg-[hsl(var(--surface)/0.97)] p-3 shadow-xl backdrop-blur-xl"
            onMouseLeave={() => setShowQuick(false)}
          >
            <div className="mb-2 flex items-center justify-between">
              <span className="text-[11px] text-[hsl(var(--muted-foreground))]">面板透明度</span>
              <span className="text-[11px] tabular-nums">{Math.round(settings.opacity * 100)}%</span>
            </div>
            <Slider
              min={20}
              max={100}
              value={Math.round(settings.opacity * 100)}
              onChange={(v) => void updateSettings({ opacity: v / 100 })}
              className="w-full"
            />
            <div className="mt-3 mb-1.5 text-[11px] text-[hsl(var(--muted-foreground))]">主题</div>
            <Segmented
              value={settings.theme}
              onChange={(v) => void updateSettings({ theme: v as ThemeMode })}
              options={[
                { value: 'light', label: '白天' },
                { value: 'dark', label: '黑夜' },
              ]}
            />
          </div>
        ) : null}
      </div>

      <Button
        variant="ghost"
        size="icon-sm"
        title={settings.alwaysOnTop ? '取消置顶' : '窗口置顶'}
        onClick={() => void updateSettings({ alwaysOnTop: !settings.alwaysOnTop })}
        className={cn(settings.alwaysOnTop && 'text-[hsl(var(--accent))]')}
      >
        {settings.alwaysOnTop ? <Pin size={13} /> : <PinOff size={13} />}
      </Button>

      <Button
        variant="ghost"
        size="icon-sm"
        title={settings.windowCollapsed ? '展开面板' : '折叠面板'}
        onClick={() => void updateSettings({ windowCollapsed: !settings.windowCollapsed })}
      >
        {settings.windowCollapsed ? <ChevronDown size={14} /> : <ChevronUp size={14} />}
      </Button>

      <Button
        variant="ghost"
        size="icon-sm"
        title="设置 (Ctrl+,)"
        onClick={() => setSettingsOpen(true)}
      >
        <SettingsIcon size={13} />
      </Button>

      <Button
        variant="ghost"
        size="icon-sm"
        title="最小化到托盘"
        onClick={() => void ipc.hideMainWindow()}
      >
        <Minus size={14} />
      </Button>
    </header>
  );
}
