import { cva, type VariantProps } from 'class-variance-authority';
import { forwardRef, type ButtonHTMLAttributes } from 'react';

import { cn } from '../../lib/utils';

/**
 * 按 shadcn/ui 的约定组织：变体用 cva 声明、类名走 cn 合并、
 * 主题色全部引用 CSS 变量。这样后续若要换成 shadcn 官方 registry 的组件，
 * 调用处的写法不用动。
 *
 * 与官方实现的差异：没有引入 Radix。这个应用只需要按钮/开关/滑块这类
 * 基础控件，Radix 的定位（无障碍弹层、焦点陷阱）在这里用不上，
 * 而规格明确要求"不要引入大型 UI 框架"。
 */
const buttonVariants = cva(
  'inline-flex items-center justify-center gap-1.5 rounded-[10px] font-medium transition-colors ' +
    'disabled:pointer-events-none disabled:opacity-45 select-none whitespace-nowrap ' +
    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-0',
  {
    variants: {
      variant: {
        primary:
          'bg-[hsl(var(--accent))] text-[hsl(var(--accent-foreground))] hover:brightness-110 active:brightness-95',
        surface:
          'bg-[hsl(var(--surface)/0.62)] text-[hsl(var(--foreground))] border border-[hsl(var(--border)/0.7)] ' +
          'hover:bg-[hsl(var(--surface-hover)/0.8)]',
        ghost: 'text-[hsl(var(--muted-foreground))] hover:bg-[hsl(var(--surface)/0.6)] hover:text-[hsl(var(--foreground))]',
        danger: 'bg-[hsl(var(--priority-urgent)/0.16)] text-[hsl(var(--priority-urgent))] hover:bg-[hsl(var(--priority-urgent)/0.26)]',
      },
      size: {
        sm: 'h-7 px-2 text-[12px]',
        md: 'h-8 px-3 text-[13px]',
        lg: 'h-9 px-4 text-[13px]',
        icon: 'h-7 w-7 p-0',
        'icon-sm': 'h-6 w-6 p-0',
      },
    },
    defaultVariants: { variant: 'surface', size: 'md' },
  },
);

export interface ButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, ...props }, ref) => (
    <button
      ref={ref}
      type="button"
      // 所有按钮都显式排除在窗口拖动区之外，避免"点按钮变成拖窗口"
      data-no-drag
      className={cn('press', buttonVariants({ variant, size }), className)}
      {...props}
    />
  ),
);
Button.displayName = 'Button';
