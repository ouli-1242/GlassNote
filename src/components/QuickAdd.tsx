import { CornerDownLeft, Plus } from 'lucide-react';
import { useMemo, useState } from 'react';

import { parseQuick } from '../lib/quickparse';
import { cn } from '../lib/utils';
import { useStore } from '../store/useStore';
import { Button } from './ui/Button';

/**
 * 底部快速添加。
 *
 * 交互约定：回车保存、Esc 清空。
 * 输入时实时解析 #标签 / !优先级 / 日期词，并在上方显示一行提示 ——
 * 解析是"可见"的，用户能立刻发现自己写错了，而不是保存完才意外。
 */
export function QuickAdd() {
  const createTask = useStore((s) => s.saveTask);
  const openEditor = useStore((s) => s.openEditor);
  const [value, setValue] = useState('');
  const [focused, setFocused] = useState(false);

  const parsed = useMemo(() => parseQuick(value), [value]);
  const showHints = focused && value.trim().length > 0 && parsed.hints.length > 0;

  const submit = () => {
    const title = parsed.input.title;
    if (!title) return;
    void createTask({ ...parsed.input, title });
    setValue('');
  };

  return (
    <div className="relative shrink-0 border-t border-[hsl(var(--border)/0.4)] px-2.5 py-2">
      {showHints ? (
        <div className="absolute -top-6 left-2.5 flex items-center gap-1.5 rounded-[8px] border border-[hsl(var(--border)/0.5)] bg-[hsl(var(--surface)/0.95)] px-2 py-[3px] text-[10px] text-[hsl(var(--muted-foreground))] shadow-lg backdrop-blur-xl">
          {parsed.hints.map((h) => (
            <span key={h} className="whitespace-nowrap">
              {h}
            </span>
          ))}
        </div>
      ) : null}

      <div className="flex items-center gap-2">
        <div className="field flex h-8 min-w-0 flex-1 items-center rounded-[10px] px-2.5">
          <Plus size={13} className="mr-1.5 shrink-0 text-[hsl(var(--muted-foreground))]" />
          <input
            data-no-drag
            value={value}
            placeholder="添加任务… 回车保存，Esc 取消"
            onChange={(e) => setValue(e.target.value)}
            onFocus={() => setFocused(true)}
            onBlur={() => setFocused(false)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                submit();
              } else if (e.key === 'Escape') {
                e.preventDefault();
                setValue('');
                (e.target as HTMLInputElement).blur();
              }
            }}
            className="min-w-0 flex-1 border-none bg-transparent text-[13px] outline-none placeholder:text-[hsl(var(--muted-foreground)/0.7)]"
          />
          {value.trim() ? (
            <span className="ml-1 hidden shrink-0 items-center gap-0.5 text-[10px] text-[hsl(var(--muted-foreground))] sm:flex">
              <CornerDownLeft size={9} />
              保存
            </span>
          ) : null}
        </div>

        {/* 需要更多字段时走完整编辑器；日常记录只用上面的输入框 */}
        <Button
          variant="ghost"
          size="icon"
          title="详细编辑"
          onClick={() => openEditor({ title: parsed.input.title, tags: parsed.input.tags })}
          className={cn('shrink-0')}
        >
          <Plus size={14} />
        </Button>
      </div>
    </div>
  );
}
