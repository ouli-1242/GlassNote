import { type ReactNode } from 'react';

import { cn } from '../../lib/utils';

/** 开关。用 button + role=switch 而不是 checkbox，样式完全可控且保持无障碍语义。 */
export function Switch({
  checked,
  onChange,
  disabled,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
  label?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      data-no-drag
      onClick={() => onChange(!checked)}
      className={cn(
        'relative h-[22px] w-[38px] shrink-0 rounded-full transition-colors duration-200',
        'disabled:opacity-45',
        checked ? 'bg-[hsl(var(--accent))]' : 'bg-[hsl(var(--muted))]',
      )}
    >
      <span
        className={cn(
          'absolute top-[3px] h-4 w-4 rounded-full bg-white shadow-sm transition-transform duration-200',
          checked ? 'translate-x-[19px]' : 'translate-x-[3px]',
        )}
      />
    </button>
  );
}

/** 滑块。用原生 input[type=range]，键盘操作与无障碍直接继承，无需额外代码。 */
export function Slider({
  value,
  min,
  max,
  step = 1,
  onChange,
  className,
  label,
}: {
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
  className?: string;
  label?: string;
}) {
  const pct = ((value - min) / (max - min)) * 100;
  return (
    <input
      type="range"
      aria-label={label}
      data-no-drag
      min={min}
      max={max}
      step={step}
      value={value}
      onChange={(e) => onChange(Number(e.target.value))}
      style={{
        // 用背景渐变画已填充部分，避免再叠一层 DOM
        background: `linear-gradient(to right, hsl(var(--accent)) 0%, hsl(var(--accent)) ${pct}%, hsl(var(--muted)) ${pct}%, hsl(var(--muted)) 100%)`,
      }}
      className={cn(
        'h-1.5 w-[112px] cursor-pointer appearance-none rounded-full outline-none',
        '[&::-webkit-slider-thumb]:h-3.5 [&::-webkit-slider-thumb]:w-3.5 [&::-webkit-slider-thumb]:appearance-none',
        '[&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-white',
        '[&::-webkit-slider-thumb]:shadow-[0_1px_4px_rgba(0,0,0,0.4)] [&::-webkit-slider-thumb]:transition-transform',
        '[&::-webkit-slider-thumb]:hover:scale-110',
        className,
      )}
    />
  );
}

/** 下拉选择。用原生 select：在桌面端它自带系统级弹层，比自绘下拉更稳也更省。 */
export function Select<T extends string | number>({
  value,
  options,
  onChange,
  className,
  label,
}: {
  value: T;
  options: Array<{ value: T; label: string }>;
  onChange: (v: T) => void;
  className?: string;
  label?: string;
}) {
  return (
    <select
      aria-label={label}
      data-no-drag
      value={value}
      onChange={(e) => {
        const raw = e.target.value;
        const found = options.find((o) => String(o.value) === raw);
        if (found) onChange(found.value);
      }}
      className={cn(
        'field h-7 cursor-pointer rounded-[9px] pl-2 pr-6 text-[12px] text-[hsl(var(--foreground))]',
        // 自绘箭头：原生箭头在深色玻璃上几乎看不见
        'appearance-none bg-[length:10px] bg-[right_6px_center] bg-no-repeat',
        className,
      )}
      style={{
        backgroundImage:
          "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 6'%3E%3Cpath d='M1 1l4 4 4-4' fill='none' stroke='%23888' stroke-width='1.6' stroke-linecap='round'/%3E%3C/svg%3E\")",
      }}
    >
      {options.map((o) => (
        <option key={String(o.value)} value={String(o.value)} className="bg-[hsl(var(--surface))]">
          {o.label}
        </option>
      ))}
    </select>
  );
}

/** 分段控件，用于主题这类少量互斥选项 */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  className,
}: {
  value: T;
  options: Array<{ value: T; label: string; title?: string }>;
  onChange: (v: T) => void;
  className?: string;
}) {
  return (
    <div
      data-no-drag
      role="tablist"
      className={cn(
        'inline-flex items-center gap-0.5 rounded-[10px] bg-[hsl(var(--muted)/0.5)] p-0.5',
        className,
      )}
    >
      {options.map((o) => {
        const active = o.value === value;
        return (
          <button
            key={o.value}
            type="button"
            role="tab"
            aria-selected={active}
            title={o.title}
            data-no-drag
            onClick={() => onChange(o.value)}
            className={cn(
              'rounded-[8px] px-2 py-1 text-[12px] transition-colors',
              active
                ? 'bg-[hsl(var(--surface))] text-[hsl(var(--foreground))] shadow-sm'
                : 'text-[hsl(var(--muted-foreground))] hover:text-[hsl(var(--foreground))]',
            )}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

/** 带色块的标签 */
export function Chip({
  children,
  color,
  onClick,
  active,
  title,
}: {
  children: ReactNode;
  color?: string;
  onClick?: () => void;
  active?: boolean;
  title?: string;
}) {
  const Comp = onClick ? 'button' : 'span';
  return (
    <Comp
      title={title}
      data-no-drag
      onClick={onClick}
      className={cn(
        'inline-flex items-center gap-1 rounded-full px-2 py-[2px] text-[11px] leading-none transition-colors',
        active
          ? 'bg-[hsl(var(--accent)/0.22)] text-[hsl(var(--accent))]'
          : 'bg-[hsl(var(--muted)/0.7)] text-[hsl(var(--muted-foreground))]',
        onClick && 'hover:bg-[hsl(var(--muted))]',
      )}
    >
      {color ? (
        <span className="h-1.5 w-1.5 shrink-0 rounded-full" style={{ background: color }} />
      ) : null}
      {children}
    </Comp>
  );
}
