import {
  forwardRef,
  type InputHTMLAttributes,
  type LabelHTMLAttributes,
  type ReactNode,
  type TextareaHTMLAttributes,
} from 'react';

import { cn } from '../../lib/utils';

/**
 * 输入控件的底衬用 `.field`（见 styles.css）。
 * 面板上直接放透明输入框会让文字和背景糊在一起，
 * 所以输入区必须有一层更实的底 —— 这是可读性要求，不是审美偏好。
 */

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(
  ({ className, ...props }, ref) => (
    <input
      ref={ref}
      data-no-drag
      className={cn(
        'field h-8 w-full rounded-[10px] px-2.5 text-[13px] text-[hsl(var(--foreground))]',
        'placeholder:text-[hsl(var(--muted-foreground)/0.7)]',
        className,
      )}
      {...props}
    />
  ),
);
Input.displayName = 'Input';

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement>>(
  ({ className, ...props }, ref) => (
    <textarea
      ref={ref}
      data-no-drag
      className={cn(
        'field w-full resize-none rounded-[10px] px-2.5 py-2 text-[13px] leading-relaxed',
        'text-[hsl(var(--foreground))] placeholder:text-[hsl(var(--muted-foreground)/0.7)]',
        className,
      )}
      {...props}
    />
  ),
);
Textarea.displayName = 'Textarea';

export function Label({ className, children, ...rest }: LabelHTMLAttributes<HTMLLabelElement>) {
  return (
    <label
      className={cn('text-[11px] font-medium text-[hsl(var(--muted-foreground))]', className)}
      {...rest}
    >
      {children}
    </label>
  );
}

/** 表单行：左标签右控件，设置面板里大量复用 */
export function Row({
  label,
  hint,
  children,
  className,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn('flex items-center justify-between gap-3 py-2', className)}>
      <div className="min-w-0 flex-1">
        <div className="text-[13px] text-[hsl(var(--foreground))]">{label}</div>
        {hint ? (
          <div className="mt-0.5 text-[11px] leading-snug text-[hsl(var(--muted-foreground))]">{hint}</div>
        ) : null}
      </div>
      <div className="shrink-0" data-no-drag>
        {children}
      </div>
    </div>
  );
}

/** 设置面板的分节标题 */
export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="mb-4">
      <h3 className="mb-1 text-[11px] font-semibold uppercase tracking-wider text-[hsl(var(--muted-foreground))]">
        {title}
      </h3>
      <div className="divide-y divide-[hsl(var(--border)/0.45)]">{children}</div>
    </section>
  );
}
