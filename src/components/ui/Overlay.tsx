import { useEffect, useRef, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

import { cn } from '../../lib/utils';

/**
 * 弹层。
 *
 * 用 Portal + 手写焦点管理，而不是引入 Radix Dialog：
 * 需要的只有三件事 —— 遮罩、Esc 关闭、焦点不跑出弹层。
 * 这三件事各十来行，换掉一个依赖是划算的（规格要求不引入大型 UI 框架）。
 *
 * 注意遮罩本身标了 data-no-drag：弹层打开时按住空白处不应该把窗口拖走。
 */
export function Dialog({
  open,
  onClose,
  title,
  children,
  footer,
  width = 'min(92%, 340px)',
}: {
  open: boolean;
  onClose: () => void;
  title?: string;
  children: ReactNode;
  footer?: ReactNode;
  width?: string;
}) {
  const panelRef = useRef<HTMLDivElement>(null);
  const restoreFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!open) return;

    // 记录打开前的焦点，关闭时还回去 —— 否则键盘用户会丢失位置
    restoreFocusRef.current = document.activeElement as HTMLElement | null;

    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onClose();
        return;
      }
      if (e.key !== 'Tab') return;

      // 焦点环：把 Tab 限制在弹层内部
      const focusables = panelRef.current?.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      );
      if (!focusables || focusables.length === 0) return;
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    };

    document.addEventListener('keydown', onKey, true);
    // 打开后把焦点送进弹层，键盘用户不必先 Tab 一圈
    const t = window.setTimeout(() => {
      panelRef.current
        ?.querySelector<HTMLElement>('input, textarea, button')
        ?.focus({ preventScroll: true });
    }, 30);

    return () => {
      document.removeEventListener('keydown', onKey, true);
      window.clearTimeout(t);
      restoreFocusRef.current?.focus?.({ preventScroll: true });
    };
  }, [open, onClose]);

  if (!open) return null;

  return createPortal(
    <div
      data-no-drag
      className="fixed inset-0 z-50 flex items-center justify-center p-3"
      // 点遮罩关闭，但点面板内部不关
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="absolute inset-0 anim-fade-in bg-black/35" />
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        style={{ width }}
        className={cn(
          'relative z-10 anim-scale-in overflow-hidden rounded-[16px]',
          'border border-[hsl(var(--border)/0.6)] bg-[hsl(var(--surface)/0.96)]',
          'shadow-[0_18px_50px_-16px_rgba(0,0,0,0.6)] backdrop-blur-xl',
        )}
      >
        {title ? (
          <div className="flex items-center justify-between border-b border-[hsl(var(--border)/0.5)] px-3.5 py-2.5">
            <h2 className="text-[13px] font-semibold">{title}</h2>
          </div>
        ) : null}

        <div className="max-h-[62vh] overflow-y-auto scroll-area px-3.5 py-3">{children}</div>

        {footer ? (
          <div className="flex items-center justify-end gap-2 border-t border-[hsl(var(--border)/0.5)] px-3.5 py-2.5">
            {footer}
          </div>
        ) : null}
      </div>
    </div>,
    document.body,
  );
}

/** 二次确认。用于删除、清空回收站这类不可逆操作。 */
export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel = '确认',
  danger,
  onConfirm,
  onCancel,
}: {
  open: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <Dialog
      open={open}
      onClose={onCancel}
      title={title}
      footer={
        <>
          <button
            type="button"
            data-no-drag
            onClick={onCancel}
            className="press h-8 rounded-[10px] border border-[hsl(var(--border)/0.7)] bg-[hsl(var(--surface)/0.6)] px-3 text-[13px] hover:bg-[hsl(var(--surface-hover)/0.8)]"
          >
            取消
          </button>
          <button
            type="button"
            data-no-drag
            onClick={onConfirm}
            className={cn(
              'press h-8 rounded-[10px] px-3 text-[13px] font-medium',
              danger
                ? 'bg-[hsl(var(--priority-urgent))] text-white hover:brightness-110'
                : 'bg-[hsl(var(--accent))] text-[hsl(var(--accent-foreground))] hover:brightness-110',
            )}
          >
            {confirmLabel}
          </button>
        </>
      }
    >
      <p className="text-[13px] leading-relaxed text-[hsl(var(--muted-foreground))]">{message}</p>
    </Dialog>
  );
}
