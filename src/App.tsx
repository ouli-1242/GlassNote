import { CheckSquare, ListTodo, NotebookPen, Search, Trash2 } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';

import { GlassBackdrop } from './components/GlassBackdrop';
import { MemoList } from './components/MemoList';
import { QuickAdd } from './components/QuickAdd';
import { SettingsPanel } from './components/SettingsPanel';
import { TaskEditor } from './components/TaskEditor';
import { TaskList } from './components/TaskList';
import { TitleBar } from './components/TitleBar';
import { Toasts } from './components/Toasts';
import { Button } from './components/ui/Button';
import { useBackendEvents } from './hooks/useBackendEvents';
import { useDragRegion } from './hooks/useDragRegion';
import * as ipc from './lib/ipc';
import { cn } from './lib/utils';
import { useStore } from './store/useStore';
import type { TaskView } from './types';

const TABS: Array<{ id: TaskView | 'memos'; label: string; icon: typeof ListTodo }> = [
  { id: 'today', label: '今日', icon: ListTodo },
  { id: 'all', label: '全部', icon: CheckSquare },
  { id: 'memos', label: '备忘录', icon: NotebookPen },
  { id: 'done', label: '已完成', icon: CheckSquare },
  { id: 'trash', label: '回收站', icon: Trash2 },
];

export default function App() {
  const rootRef = useRef<HTMLDivElement>(null);
  const ready = useStore((s) => s.ready);
  const fatal = useStore((s) => s.fatalError);
  const settings = useStore((s) => s.settings);
  const glass = useStore((s) => s.glass);
  const init = useStore((s) => s.init);
  const locked = useStore((s) => s.locked);
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  const search = useStore((s) => s.search);
  const setSearch = useStore((s) => s.setSearch);
  const openEditor = useStore((s) => s.openEditor);
  const setSettingsOpen = useStore((s) => s.setSettingsOpen);

  // 备忘录是独立视图，不占用 TaskView 的语义，所以单独记一个标志
  const [memoTab, setMemoTab] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);

  useDragRegion(ready);
  useBackendEvents();

  useEffect(() => {
    void init();
  }, [init]);

  // 窗口内快捷键。全局快捷键由 Rust 注册，不在这里处理。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const typing =
        target &&
        (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable);

      if ((e.ctrlKey || e.metaKey) && e.key === ',') {
        e.preventDefault();
        setSettingsOpen(true);
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        setSearchOpen(true);
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'n') {
        e.preventDefault();
        openEditor({ title: '' });
      } else if (e.key === 'Escape' && !typing) {
        setSearchOpen(false);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [openEditor, setSettingsOpen]);

  const activeTab: TaskView | 'memos' = memoTab ? 'memos' : view;

  const switchTab = (id: TaskView | 'memos') => {
    if (id === 'memos') {
      setMemoTab(true);
    } else {
      setMemoTab(false);
      setView(id);
    }
  };

  const body = useMemo(() => {
    if (!ready) {
      return (
        <div className="flex flex-1 items-center justify-center text-[12px] text-[hsl(var(--muted-foreground))]">
          正在加载…
        </div>
      );
    }
    if (fatal) {
      return (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
          <p className="text-[13px] font-medium text-[hsl(var(--priority-urgent))]">初始化失败</p>
          <p className="text-[11px] leading-relaxed text-[hsl(var(--muted-foreground))]">{fatal}</p>
        </div>
      );
    }
    if (locked) return <LockScreen />;
    if (memoTab) return <MemoList />;
    return <TaskList />;
  }, [ready, fatal, locked, memoTab]);

  return (
    <GlassBackdrop ref={rootRef} glass={glass}>
      <TitleBar />

      {/* 折叠态：只留顶栏，窗口由 Rust 侧同步收缩 */}
      {settings.windowCollapsed ? null : (
        <>
          <nav className="flex shrink-0 items-center gap-0.5 px-2 pb-1 pt-1.5">
            {TABS.map((t) => {
              const Icon = t.icon;
              const active = activeTab === t.id;
              return (
                <button
                  key={t.id}
                  type="button"
                  data-no-drag
                  onClick={() => switchTab(t.id)}
                  className={cn(
                    'press flex items-center gap-1 rounded-[9px] px-2 py-1 text-[12px] transition-colors',
                    active
                      ? 'bg-[hsl(var(--surface)/0.85)] font-medium text-[hsl(var(--foreground))]'
                      : 'text-[hsl(var(--muted-foreground))] hover:bg-[hsl(var(--surface)/0.5)] hover:text-[hsl(var(--foreground))]',
                  )}
                >
                  <Icon size={12} />
                  {t.label}
                </button>
              );
            })}

            <div className="min-w-0 flex-1" />

            <Button
              variant="ghost"
              size="icon-sm"
              title="搜索 (Ctrl+F)"
              onClick={() => setSearchOpen((v) => !v)}
              className={cn(searchOpen && 'bg-[hsl(var(--surface)/0.7)]')}
            >
              <Search size={13} />
            </Button>
          </nav>

          {searchOpen && activeTab !== 'memos' ? (
            <div className="shrink-0 px-2.5 pb-1.5">
              <input
                autoFocus
                data-no-drag
                value={search}
                placeholder="搜索任务…"
                onChange={(e) => setSearch(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Escape') {
                    setSearch('');
                    setSearchOpen(false);
                  }
                }}
                className="field h-7 w-full rounded-[9px] px-2.5 text-[12px]"
              />
            </div>
          ) : null}

          {body}

          {/* 回收站和已完成视图不提供快速添加：往历史里加新任务没有意义 */}
          {activeTab !== 'memos' && activeTab !== 'trash' ? <QuickAdd /> : null}
        </>
      )}

      <Toasts />
      <TaskEditor />
      <SettingsPanel />
    </GlassBackdrop>
  );
}

/** PIN 锁屏。锁定状态下不渲染任何数据，避免"锁了但内容还在 DOM 里"。 */
function LockScreen() {
  const [pin, setPin] = useState('');
  const [error, setError] = useState(false);
  const [checking, setChecking] = useState(false);

  const submit = async () => {
    if (pin.length < 4 || checking) return;
    setChecking(true);
    try {
      const ok = await ipc.verifyLockPin(pin);
      if (ok) {
        useStore.setState({ locked: false });
      } else {
        setError(true);
        setPin('');
        setTimeout(() => setError(false), 1200);
      }
    } finally {
      setChecking(false);
    }
  };

  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 px-8">
      <div className="text-[hsl(var(--muted-foreground))]">
        <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6">
          <rect x="4" y="10" width="16" height="10" rx="2" />
          <path d="M8 10V7a4 4 0 1 1 8 0v3" />
        </svg>
      </div>
      <p className="text-[13px] font-medium">已锁定</p>
      <input
        autoFocus
        type="password"
        inputMode="numeric"
        value={pin}
        placeholder="输入 PIN"
        onChange={(e) => setPin(e.target.value.replace(/\D/g, '').slice(0, 8))}
        onKeyDown={(e) => {
          if (e.key === 'Enter') void submit();
        }}
        className={cn(
          'field h-8 w-[130px] rounded-[10px] text-center text-[15px] tracking-[0.3em]',
          error && 'border-[hsl(var(--priority-urgent))]',
        )}
      />
      {error ? (
        <p className="text-[11px] text-[hsl(var(--priority-urgent))]">PIN 不正确</p>
      ) : (
        <p className="text-[11px] text-[hsl(var(--muted-foreground))]">回车确认</p>
      )}
    </div>
  );
}
