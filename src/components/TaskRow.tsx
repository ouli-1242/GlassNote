import { Bell, Check, Clock, MoreHorizontal, Pencil, Pin, Repeat, SkipForward, Trash2 } from 'lucide-react';
import { memo, useEffect, useRef, useState } from 'react';

import { cn, describeRepeat, formatDue, isOverdue } from '../lib/utils';
import { useStore } from '../store/useStore';
import type { Task } from '../types';

const PRIORITY_META: Record<number, { label: string; color: string }> = {
  0: { label: '无', color: 'hsl(var(--muted-foreground))' },
  1: { label: '低', color: 'hsl(var(--priority-low))' },
  2: { label: '普通', color: 'hsl(var(--priority-normal))' },
  3: { label: '高', color: 'hsl(var(--priority-high))' },
  4: { label: '紧急', color: 'hsl(var(--priority-urgent))' },
};

export const ROW_HEIGHT = 56;

/**
 * 任务行。
 *
 * 高度固定为 ROW_HEIGHT：虚拟滚动依赖这个不变量，改了这里必须同步改
 * useVirtualList 传入的 itemHeight，否则滚动位置会错乱。
 *
 * 用 memo 包一层：列表里任意一项变化不应该让所有行重渲染，
 * 长列表下这是最直接的性能收益。
 */
export const TaskRow = memo(function TaskRow({ task }: { task: Task }) {
  const completing = useStore((s) => s.completing.includes(task.id));
  const complete = useStore((s) => s.complete);
  const remove = useStore((s) => s.removeTask);
  const restore = useStore((s) => s.restoreTask);
  const purge = useStore((s) => s.purgeTask);
  const togglePin = useStore((s) => s.togglePin);
  const openEditor = useStore((s) => s.openEditor);
  const skip = useStore((s) => s.skip);
  const snooze = useStore((s) => s.snooze);
  const view = useStore((s) => s.view);

  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  // 菜单点外部关闭
  useEffect(() => {
    if (!menuOpen) return;
    const onDown = (e: MouseEvent) => {
      if (!menuRef.current?.contains(e.target as Node)) setMenuOpen(false);
    };
    document.addEventListener('mousedown', onDown);
    return () => document.removeEventListener('mousedown', onDown);
  }, [menuOpen]);

  const prio = PRIORITY_META[task.priority] ?? PRIORITY_META[2];
  const overdue = task.status === 'todo' && isOverdue(task.dueAt);
  const repeating = task.repeatType !== 'once';
  const dueLabel = task.dueAt ? formatDue(task.dueAt) : '';

  return (
    <div
      className={cn(
        'group relative flex items-center gap-2.5 px-3',
        'border-b border-[hsl(var(--border)/0.28)] transition-colors',
        'hover:bg-[hsl(var(--surface)/0.5)]',
        task.pinned && 'bg-[hsl(var(--surface)/0.32)]',
        // 完成动画期间冻结交互，避免连点两次触发两次完成
        completing && 'task-done',
      )}
      style={{ height: ROW_HEIGHT }}
    >
      {/* 左侧优先级色条：比在文字里塞"高/紧急"更省空间，扫一眼就能看出轻重 */}
      <span
        aria-hidden
        className="absolute left-0 top-1/2 h-5 w-[3px] -translate-y-1/2 rounded-r-full transition-opacity"
        style={{ background: prio.color, opacity: task.priority >= 3 ? 1 : 0.35 }}
      />

      {/* 打勾 */}
      <button
        type="button"
        data-no-drag
        aria-label={task.status === 'done' ? '标记为未完成' : '完成任务'}
        title="完成"
        disabled={completing}
        onClick={() => {
          if (task.status === 'done') void restore(task.id);
          else void complete(task.id);
        }}
        className={cn(
          'press grid h-[19px] w-[19px] shrink-0 place-items-center rounded-full border transition-all',
          task.status === 'done'
            ? 'border-[hsl(var(--accent))] bg-[hsl(var(--accent))] text-white'
            : 'border-[hsl(var(--muted-foreground)/0.55)] hover:border-[hsl(var(--accent))] hover:bg-[hsl(var(--accent)/0.12)]',
        )}
      >
        {task.status === 'done' || completing ? (
          <Check size={12} strokeWidth={3.2} className="check-pop" />
        ) : null}
      </button>

      {/* 主体 */}
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5">
          <span
            className={cn(
              'truncate text-[13px] leading-tight',
              task.status === 'done' && 'text-[hsl(var(--muted-foreground))] line-through',
            )}
          >
            {task.title}
          </span>
          {task.pinned ? (
            <Pin size={10} className="shrink-0 text-[hsl(var(--accent))]" />
          ) : null}
        </div>

        {/* 元信息行：只在有内容时才占位，避免每行都留一条空白 */}
        {(dueLabel || repeating || task.tags.length > 0 || task.note) && (
          <div className="mt-[3px] flex items-center gap-2 text-[11px] leading-none">
            {dueLabel ? (
              <span
                className={cn(
                  'inline-flex items-center gap-0.5 tabular-nums',
                  overdue ? 'text-[hsl(var(--priority-urgent))]' : 'text-[hsl(var(--muted-foreground))]',
                )}
              >
                <Clock size={9.5} />
                {dueLabel}
              </span>
            ) : null}

            {repeating ? (
              <span
                className="inline-flex items-center gap-0.5 text-[hsl(var(--muted-foreground))]"
                title={describeRepeat(task)}
              >
                <Repeat size={9.5} />
                {describeRepeat(task)}
              </span>
            ) : null}

            {task.remindAt ? (
              <span className="inline-flex items-center gap-0.5 text-[hsl(var(--muted-foreground))]">
                <Bell size={9.5} />
                {formatDue(task.remindAt)}
              </span>
            ) : null}

            {task.tags.slice(0, 2).map((t) => (
              <span key={t} className="rounded-full bg-[hsl(var(--muted)/0.8)] px-1.5 py-[1px]">
                {t}
              </span>
            ))}
            {task.tags.length > 2 ? (
              <span className="text-[hsl(var(--muted-foreground))]">+{task.tags.length - 2}</span>
            ) : null}

            {task.note ? (
              <span className="truncate text-[hsl(var(--muted-foreground)/0.8)]">{task.note}</span>
            ) : null}
          </div>
        )}
      </div>

      {/* 悬停操作区 */}
      <div
        className="relative flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100"
        ref={menuRef}
      >
        {view === 'trash' ? (
          <>
            <IconBtn title="恢复" onClick={() => void restore(task.id)}>
              <Check size={13} />
            </IconBtn>
            <IconBtn title="彻底删除" danger onClick={() => void purge(task.id)}>
              <Trash2 size={13} />
            </IconBtn>
          </>
        ) : (
          <>
            <IconBtn title="编辑" onClick={() => openEditor({ ...task })}>
              <Pencil size={12.5} />
            </IconBtn>
            {repeating ? (
              <IconBtn title="跳过本次" onClick={() => void skip(task.id)}>
                <SkipForward size={12.5} />
              </IconBtn>
            ) : null}
            <IconBtn title="更多" onClick={() => setMenuOpen((v) => !v)}>
              <MoreHorizontal size={13} />
            </IconBtn>

            {menuOpen ? (
              <div className="absolute right-0 top-6 z-30 w-40 anim-scale-in overflow-hidden rounded-[10px] border border-[hsl(var(--border)/0.6)] bg-[hsl(var(--surface)/0.97)] py-1 shadow-xl backdrop-blur-xl">
                <MenuItem
                  onClick={() => {
                    setMenuOpen(false);
                    void togglePin(task.id);
                  }}
                >
                  {task.pinned ? '取消置顶' : '置顶'}
                </MenuItem>
                <MenuItem
                  onClick={() => {
                    setMenuOpen(false);
                    void snooze(task.id, 5);
                  }}
                >
                  稍后提醒 5 分钟
                </MenuItem>
                <MenuItem
                  onClick={() => {
                    setMenuOpen(false);
                    void snooze(task.id, 60);
                  }}
                >
                  稍后提醒 1 小时
                </MenuItem>
                <MenuItem
                  onClick={() => {
                    setMenuOpen(false);
                    void snooze(task.id, 24 * 60);
                  }}
                >
                  明天提醒
                </MenuItem>
                <div className="my-1 h-px bg-[hsl(var(--border)/0.5)]" />
                <MenuItem
                  danger
                  onClick={() => {
                    setMenuOpen(false);
                    void remove(task.id);
                  }}
                >
                  删除
                </MenuItem>
              </div>
            ) : null}
          </>
        )}
      </div>
    </div>
  );
});

function IconBtn({
  children,
  title,
  onClick,
  danger,
}: {
  children: React.ReactNode;
  title: string;
  onClick: () => void;
  danger?: boolean;
}) {
  return (
    <button
      type="button"
      data-no-drag
      title={title}
      aria-label={title}
      onClick={onClick}
      className={cn(
        'press grid h-6 w-6 place-items-center rounded-[7px] text-[hsl(var(--muted-foreground))] transition-colors',
        'hover:bg-[hsl(var(--surface-hover)/0.85)] hover:text-[hsl(var(--foreground))]',
        danger && 'hover:text-[hsl(var(--priority-urgent))]',
      )}
    >
      {children}
    </button>
  );
}

function MenuItem({
  children,
  onClick,
  danger,
}: {
  children: React.ReactNode;
  onClick: () => void;
  danger?: boolean;
}) {
  return (
    <button
      type="button"
      data-no-drag
      onClick={onClick}
      className={cn(
        'block w-full px-2.5 py-1.5 text-left text-[12px] transition-colors',
        'hover:bg-[hsl(var(--surface-hover)/0.9)]',
        danger && 'text-[hsl(var(--priority-urgent))]',
      )}
    >
      {children}
    </button>
  );
}
