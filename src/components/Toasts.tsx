import { AlertCircle, CheckCircle2, Info, X } from 'lucide-react';

import { cn } from '../lib/utils';
import { useStore } from '../store/useStore';

const ICONS = {
  info: Info,
  success: CheckCircle2,
  error: AlertCircle,
};

/**
 * 轻提示。
 *
 * 撤销提示就长在这里（kind=success 带 action），而不是单独做一个撤销条 ——
 * 完成任务的反馈和撤销入口是同一件事，分成两个 UI 反而要在两处维护"5 秒后消失"的逻辑。
 */
export function Toasts() {
  const toasts = useStore((s) => s.toasts);
  const dismiss = useStore((s) => s.dismissToast);

  if (toasts.length === 0) return null;

  return (
    <div className="pointer-events-none absolute bottom-3 left-1/2 z-40 flex w-[min(92%,320px)] -translate-x-1/2 flex-col gap-1.5">
      {toasts.map((t) => {
        const Icon = ICONS[t.kind];
        return (
          <div
            key={t.id}
            data-no-drag
            role="status"
            className={cn(
              'pointer-events-auto flex anim-slide-up items-center gap-2 rounded-[11px] px-2.5 py-2',
              'border border-[hsl(var(--border)/0.6)] bg-[hsl(var(--surface)/0.97)] shadow-xl backdrop-blur-xl',
            )}
          >
            <Icon
              size={13}
              className={cn(
                'shrink-0',
                t.kind === 'error'
                  ? 'text-[hsl(var(--priority-urgent))]'
                  : t.kind === 'success'
                    ? 'text-[hsl(var(--accent))]'
                    : 'text-[hsl(var(--muted-foreground))]',
              )}
            />
            <span className="min-w-0 flex-1 truncate text-[12px]">{t.message}</span>

            {t.action ? (
              <button
                type="button"
                data-no-drag
                onClick={t.action.run}
                className="press shrink-0 rounded-[7px] px-1.5 py-0.5 text-[11px] font-medium text-[hsl(var(--accent))] hover:bg-[hsl(var(--accent)/0.14)]"
              >
                {t.action.label}
              </button>
            ) : null}

            <button
              type="button"
              data-no-drag
              aria-label="关闭"
              onClick={() => dismiss(t.id)}
              className="press grid h-5 w-5 shrink-0 place-items-center rounded-[6px] text-[hsl(var(--muted-foreground))] hover:bg-[hsl(var(--surface-hover))]"
            >
              <X size={11} />
            </button>
          </div>
        );
      })}
    </div>
  );
}
