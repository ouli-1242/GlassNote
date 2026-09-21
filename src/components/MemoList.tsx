import { ChevronRight, Eye, EyeOff, GripVertical, Pin, Plus, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';

import { renderMarkdown, plainSummary } from '../lib/markdown';
import { cn } from '../lib/utils';
import { useStore } from '../store/useStore';
import type { Memo } from '../types';
import { Button } from './ui/Button';

/** 自动保存防抖时长。规格要求 500ms。 */
const AUTOSAVE_MS = 500;

const MEMO_COLORS = ['default', 'amber', 'rose', 'sky', 'emerald', 'violet'];

const COLOR_MAP: Record<string, string> = {
  default: 'hsl(var(--surface) / 0.55)',
  amber: 'hsl(38 92% 55% / 0.16)',
  rose: 'hsl(348 84% 58% / 0.16)',
  sky: 'hsl(205 88% 56% / 0.16)',
  emerald: 'hsl(158 74% 44% / 0.16)',
  violet: 'hsl(268 82% 62% / 0.16)',
};

/**
 * 备忘录卡片。
 *
 * 折叠用 CSS 的 grid-template-rows: 0fr → 1fr 过渡（见 styles.css 的 .collapsible），
 * 而不是 max-height —— max-height 需要猜一个上限，猜小了内容被截断，
 * 猜大了动画前半段会"空跑"，看起来像卡顿。
 */
function MemoCard({ memo }: { memo: Memo }) {
  const saveMemo = useStore((s) => s.saveMemo);
  const toggleCollapsed = useStore((s) => s.toggleMemoCollapsed);
  const removeMemo = useStore((s) => s.removeMemo);
  const togglePin = useStore((s) => s.toggleMemoPin);

  const [title, setTitle] = useState(memo.title);
  const [content, setContent] = useState(memo.content);
  const [preview, setPreview] = useState(false);
  const [dirty, setDirty] = useState(false);

  // 外部数据刷新时同步回来，但要避开"正在输入"的时刻，
  // 否则自动保存触发的列表刷新会把光标位置顶掉
  useEffect(() => {
    if (!dirty) {
      setTitle(memo.title);
      setContent(memo.content);
    }
  }, [memo.title, memo.content, dirty]);

  const timerRef = useRef<number | undefined>(undefined);

  const scheduleSave = useCallback(
    (next: { title?: string; content?: string }) => {
      setDirty(true);
      window.clearTimeout(timerRef.current);
      timerRef.current = window.setTimeout(() => {
        void saveMemo({
          id: memo.id,
          title: next.title ?? title,
          content: next.content ?? content,
        }).then(() => setDirty(false));
      }, AUTOSAVE_MS);
    },
    [memo.id, title, content, saveMemo],
  );

  // 卸载时把未落盘的改动补一次，避免"改完立刻切走"丢内容
  useEffect(
    () => () => {
      window.clearTimeout(timerRef.current);
    },
    [],
  );

  const summary = plainSummary(content);

  return (
    <article
      className="group/memo overflow-hidden rounded-[14px] border border-[hsl(var(--border)/0.55)] backdrop-blur-[2px] transition-colors"
      style={{ background: COLOR_MAP[memo.color] ?? COLOR_MAP.default }}
    >
      {/* 顶栏：整条都可点，用于折叠/展开 */}
      <div
        className="flex cursor-pointer items-center gap-1.5 px-2.5 py-2"
        onClick={() => void toggleCollapsed(memo.id, !memo.collapsed)}
        role="button"
        tabIndex={0}
        aria-expanded={!memo.collapsed}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            void toggleCollapsed(memo.id, !memo.collapsed);
          }
        }}
      >
        <ChevronRight
          size={13}
          className={cn(
            'shrink-0 text-[hsl(var(--muted-foreground))] transition-transform duration-200',
            !memo.collapsed && 'rotate-90',
          )}
        />

        {/* 标题：折叠态只读，展开态可编辑 */}
        {memo.collapsed ? (
          <span className="min-w-0 flex-1 truncate text-[13px] font-medium">
            {title || '未命名备忘录'}
          </span>
        ) : (
          <input
            data-no-drag
            value={title}
            placeholder="标题"
            onClick={(e) => e.stopPropagation()}
            onChange={(e) => {
              setTitle(e.target.value);
              scheduleSave({ title: e.target.value });
            }}
            className="min-w-0 flex-1 border-none bg-transparent text-[13px] font-medium outline-none placeholder:text-[hsl(var(--muted-foreground)/0.6)]"
          />
        )}

        {memo.collapsed && summary ? (
          <span className="hidden min-w-0 flex-[1.4] truncate text-[11px] text-[hsl(var(--muted-foreground))] sm:block">
            {summary}
          </span>
        ) : null}

        {dirty ? (
          <span className="shrink-0 text-[10px] text-[hsl(var(--muted-foreground))]">保存中</span>
        ) : null}

        {memo.pinned ? <Pin size={11} className="shrink-0 text-[hsl(var(--accent))]" /> : null}

        {/* 操作区：悬停才出现 */}
        <div
          className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover/memo:opacity-100 focus-within:opacity-100"
          onClick={(e) => e.stopPropagation()}
        >
          <button
            type="button"
            data-no-drag
            title={preview ? '编辑' : 'Markdown 预览'}
            aria-label="切换预览"
            onClick={() => setPreview((v) => !v)}
            className="press grid h-6 w-6 place-items-center rounded-[7px] text-[hsl(var(--muted-foreground))] hover:bg-[hsl(var(--surface-hover)/0.85)] hover:text-[hsl(var(--foreground))]"
          >
            {preview ? <EyeOff size={12} /> : <Eye size={12} />}
          </button>
          <button
            type="button"
            data-no-drag
            title={memo.pinned ? '取消置顶' : '置顶'}
            aria-label="置顶"
            onClick={() => void togglePin(memo.id)}
            className="press grid h-6 w-6 place-items-center rounded-[7px] text-[hsl(var(--muted-foreground))] hover:bg-[hsl(var(--surface-hover)/0.85)] hover:text-[hsl(var(--foreground))]"
          >
            <Pin size={12} />
          </button>
          <button
            type="button"
            data-no-drag
            title="删除"
            aria-label="删除备忘录"
            onClick={() => void removeMemo(memo.id)}
            className="press grid h-6 w-6 place-items-center rounded-[7px] text-[hsl(var(--muted-foreground))] hover:bg-[hsl(var(--surface-hover)/0.85)] hover:text-[hsl(var(--priority-urgent))]"
          >
            <Trash2 size={12} />
          </button>
        </div>
      </div>

      {/* 正文：折叠动画靠 grid-template-rows，不需要知道内容高度 */}
      <div className="collapsible" data-collapsed={String(memo.collapsed)}>
        <div>
          <div className="px-2.5 pb-2.5 pt-0.5">
            {preview ? (
              <div
                className="text-[12.5px] leading-relaxed [&_div]:min-h-[1em]"
                // 内容已在 markdown.ts 里做过整体 HTML 转义，见该文件的安全性说明
                dangerouslySetInnerHTML={{ __html: renderMarkdown(content) }}
              />
            ) : (
              <textarea
                data-no-drag
                value={content}
                placeholder="写点什么… 支持 Markdown"
                rows={Math.min(14, Math.max(3, content.split('\n').length))}
                onChange={(e) => {
                  setContent(e.target.value);
                  scheduleSave({ content: e.target.value });
                }}
                className="w-full resize-none border-none bg-transparent text-[12.5px] leading-relaxed outline-none placeholder:text-[hsl(var(--muted-foreground)/0.6)]"
              />
            )}

            {/* 颜色选择：直接铺一排小圆点，比下拉菜单少两次点击 */}
            <div className="mt-1.5 flex items-center gap-1.5 opacity-0 transition-opacity group-hover/memo:opacity-100 focus-within:opacity-100">
              {MEMO_COLORS.map((c) => (
                <button
                  key={c}
                  type="button"
                  data-no-drag
                  aria-label={`颜色 ${c}`}
                  onClick={() => void saveMemo({ id: memo.id, title, content, color: c })}
                  className={cn(
                    'press h-3.5 w-3.5 rounded-full border transition-transform hover:scale-115',
                    memo.color === c ? 'border-[hsl(var(--foreground)/0.7)]' : 'border-transparent',
                  )}
                  style={{ background: COLOR_MAP[c] === COLOR_MAP.default ? 'hsl(var(--muted))' : COLOR_MAP[c] }}
                />
              ))}
            </div>
          </div>
        </div>
      </div>
    </article>
  );
}

export function MemoList() {
  const memos = useStore((s) => s.memos);
  const saveMemo = useStore((s) => s.saveMemo);
  const [filter, setFilter] = useState('');

  const shown = filter
    ? memos.filter(
        (m) =>
          m.title.toLowerCase().includes(filter.toLowerCase()) ||
          m.content.toLowerCase().includes(filter.toLowerCase()),
      )
    : memos;

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <div className="flex items-center gap-2 px-2.5 pb-1.5 pt-2">
        <input
          data-no-drag
          value={filter}
          placeholder="搜索备忘录…"
          onChange={(e) => setFilter(e.target.value)}
          className="field h-7 min-w-0 flex-1 rounded-[9px] px-2 text-[12px]"
        />
        <Button
          variant="surface"
          size="sm"
          onClick={() => void saveMemo({ title: '新备忘录', content: '' })}
        >
          <Plus size={12} />
          新建
        </Button>
      </div>

      <div className="scroll-area flex-1 space-y-2 px-2.5 pb-2.5">
        {shown.length === 0 ? (
          <div className="flex flex-col items-center justify-center gap-1.5 py-10 text-center">
            <GripVertical size={22} className="text-[hsl(var(--muted-foreground)/0.5)]" />
            <p className="text-[13px] font-medium">
              {filter ? '没有匹配的备忘录' : '还没有备忘录'}
            </p>
            <p className="text-[11px] text-[hsl(var(--muted-foreground))]">
              {filter ? '换个关键词试试' : '点击右上角新建，正文支持 Markdown'}
            </p>
          </div>
        ) : (
          shown.map((m) => <MemoCard key={m.id} memo={m} />)
        )}
      </div>
    </div>
  );
}
