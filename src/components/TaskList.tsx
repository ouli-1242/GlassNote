import { ClipboardList, Inbox, Trash2 } from 'lucide-react';
import { useCallback, useMemo, useRef, useState } from 'react';

import { cn } from '../lib/utils';
import { useStore } from '../store/useStore';
import type { Task } from '../types';
import { useVirtualList } from '../hooks/useVirtualList';
import { ROW_HEIGHT, TaskRow } from './TaskRow';
import { Button } from './ui/Button';

interface DragState {
  id: number;
  fromIdx: number;
  toIdx: number;
  /** 指针相对起始位置的位移，用于让被拖行跟手 */
  dy: number;
}

/**
 * 任务列表。
 *
 * 两个容易被忽略但必须处理的点：
 *   1. **虚拟滚动与拖拽排序会互相干扰**。虚拟化只渲染可视区，
 *      而拖拽需要知道"所有行"的顺序来计算落点。所以一旦进入拖拽，
 *      就临时关掉虚拟化 —— 正在手工排序的列表规模通常不大，代价可接受。
 *   2. **拖拽行必须用 transform 而不是改 DOM 顺序**。改顺序会触发整列重排，
 *      每一帧都要重新布局；transform 只走合成层，不重排不重绘。
 */
export function TaskList() {
  const tasks = useStore((s) => s.tasks);
  const view = useStore((s) => s.view);
  const reorder = useStore((s) => s.reorder);
  const emptyTrash = useStore((s) => s.emptyTrash);
  const search = useStore((s) => s.search);

  const [drag, setDrag] = useState<DragState | null>(null);
  const startYRef = useRef(0);
  const tasksRef = useRef<Task[]>(tasks);
  tasksRef.current = tasks;

  const virt = useVirtualList(tasks, { itemHeight: ROW_HEIGHT });

  // 拖拽期间强制全量渲染，保证落点计算覆盖整个列表
  const list = drag ? tasks : virt.visible;
  const offsetIndex = drag ? 0 : virt.start;

  const onHandleDown = useCallback(
    (e: React.PointerEvent, task: Task, idx: number) => {
      e.preventDefault();
      e.stopPropagation();
      (e.target as HTMLElement).setPointerCapture(e.pointerId);
      startYRef.current = e.clientY;
      setDrag({ id: task.id, fromIdx: idx, toIdx: idx, dy: 0 });
    },
    [],
  );

  const onHandleMove = useCallback(
    (e: React.PointerEvent) => {
      setDrag((d) => {
        if (!d) return d;
        const dy = e.clientY - startYRef.current;
        const n = tasksRef.current.length;
        const to = Math.max(0, Math.min(n - 1, d.fromIdx + Math.round(dy / ROW_HEIGHT)));
        return { ...d, dy, toIdx: to };
      });
    },
    [],
  );

  const onHandleUp = useCallback(
    (e: React.PointerEvent) => {
      const d = drag;
      setDrag(null);
      if (!d) return;
      try {
        (e.target as HTMLElement).releasePointerCapture(e.pointerId);
      } catch {
        /* 指针可能已经释放 */
      }
      if (d.toIdx === d.fromIdx) return;

      const ids = tasksRef.current.map((t) => t.id);
      const [moved] = ids.splice(d.fromIdx, 1);
      ids.splice(d.toIdx, 0, moved);
      void reorder(ids);
    },
    [drag, reorder],
  );

  const isEmpty = tasks.length === 0;

  const emptyState = useMemo(() => {
    if (search) {
      return { icon: <Inbox size={26} />, title: '没有匹配的任务', hint: `没有找到包含「${search}」的任务` };
    }
    switch (view) {
      case 'done':
        return { icon: <ClipboardList size={26} />, title: '还没有已完成的任务', hint: '完成的记录会出现在这里，可随时恢复' };
      case 'trash':
        return { icon: <Trash2 size={26} />, title: '回收站是空的', hint: '删除的任务会先放进回收站，30 天后自动清理' };
      case 'upcoming':
        return { icon: <ClipboardList size={26} />, title: '没有待开始的任务', hint: '给任务设置未来的时间就会出现在这里' };
      default:
        return { icon: <ClipboardList size={26} />, title: '今天没有待办', hint: '在下方输入框添加一条，回车即可保存' };
    }
  }, [view, search]);

  if (isEmpty) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 px-8 text-center">
        <div className="mb-1 text-[hsl(var(--muted-foreground)/0.55)]">{emptyState.icon}</div>
        <p className="text-[13px] font-medium">{emptyState.title}</p>
        <p className="text-[11px] leading-relaxed text-[hsl(var(--muted-foreground))]">{emptyState.hint}</p>
        {view === 'trash' ? (
          <Button variant="surface" size="sm" className="mt-2" onClick={() => void emptyTrash()}>
            清空回收站
          </Button>
        ) : null}
      </div>
    );
  }

  return (
    <div ref={virt.containerRef} className="scroll-area relative flex-1">
      <div style={drag ? undefined : virt.spacerStyle}>
        <div style={drag ? undefined : virt.innerStyle}>
          {list.map((task, i) => {
            const idx = offsetIndex + i;
            const isDragged = drag?.id === task.id;

            // 被拖行之外的其它行，按落点让出空间
            let shift = 0;
            if (drag && !isDragged) {
              if (drag.fromIdx < drag.toIdx && idx > drag.fromIdx && idx <= drag.toIdx) shift = -ROW_HEIGHT;
              else if (drag.fromIdx > drag.toIdx && idx < drag.fromIdx && idx >= drag.toIdx) shift = ROW_HEIGHT;
            }

            return (
              <div
                key={task.id}
                style={{
                  transform: isDragged
                    ? `translateY(${drag!.dy}px)`
                    : shift
                      ? `translateY(${shift}px)`
                      : undefined,
                  transition: isDragged ? 'none' : 'transform 160ms cubic-bezier(0.22,1,0.36,1)',
                  position: 'relative',
                  zIndex: isDragged ? 30 : undefined,
                  // 被拖行浮起来：加阴影与轻微放大，明确"正在搬动它"
                  boxShadow: isDragged ? '0 10px 26px -10px rgba(0,0,0,0.6)' : undefined,
                  background: isDragged ? 'hsl(var(--surface) / 0.96)' : undefined,
                  borderRadius: isDragged ? 10 : undefined,
                  opacity: isDragged ? 0.96 : 1,
                }}
                className={cn('group/row')}
              >
                {/* 拖拽手柄：只在悬停时出现，避免日常视觉噪音 */}
                <button
                  type="button"
                  data-drag-handle
                  data-no-drag
                  aria-label="拖动排序"
                  title="按住拖动排序"
                  onPointerDown={(e) => onHandleDown(e, task, idx)}
                  onPointerMove={onHandleMove}
                  onPointerUp={onHandleUp}
                  onPointerCancel={onHandleUp}
                  className={cn(
                    'absolute left-0 top-0 z-20 h-full w-[5px] cursor-grab opacity-0 transition-opacity',
                    'group-hover/row:opacity-100 active:cursor-grabbing',
                    'bg-[hsl(var(--accent)/0.5)]',
                  )}
                />
                <TaskRow task={task} />
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
