import { CalendarClock, Repeat, Tag, Bell } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';

import * as ipc from '../lib/ipc';
import {
  cn,
  describeRepeat,
  formatDue,
  fromDatetimeLocal,
  toDatetimeLocal,
} from '../lib/utils';
import { useStore } from '../store/useStore';
import type { RepeatType, RepeatUnit, TaskInput } from '../types';
import { Button } from './ui/Button';
import { Chip, Segmented, Select } from './ui/Controls';
import { Input, Label, Textarea } from './ui/Field';
import { Dialog } from './ui/Overlay';

const REPEAT_OPTIONS: Array<{ value: RepeatType; label: string }> = [
  { value: 'once', label: '不重复' },
  { value: 'daily', label: '每天' },
  { value: 'weekly', label: '每周' },
  { value: 'monthly', label: '每月' },
  { value: 'weekday', label: '每个工作日' },
  { value: 'interval', label: '自定义间隔' },
  { value: 'cron', label: 'Cron 表达式' },
];

const PRIORITIES = [
  { value: 0, label: '无' },
  { value: 1, label: '低' },
  { value: 2, label: '普通' },
  { value: 3, label: '高' },
  { value: 4, label: '紧急' },
];

const DOW = [
  { v: 1, l: '一' },
  { v: 2, l: '二' },
  { v: 3, l: '三' },
  { v: 4, l: '四' },
  { v: 5, l: '五' },
  { v: 6, l: '六' },
  { v: 7, l: '日' },
];

/** 提醒的常用提前量。独立于任务出现时间，规格要求二者可分开设置。 */
const REMIND_PRESETS = [
  { label: '不提醒', minutes: null },
  { label: '准时', minutes: 0 },
  { label: '提前 5 分', minutes: 5 },
  { label: '提前 30 分', minutes: 30 },
  { label: '提前 1 小时', minutes: 60 },
  { label: '提前 1 天', minutes: 1440 },
];

export function TaskEditor() {
  const editing = useStore((s) => s.editing);
  const closeEditor = useStore((s) => s.closeEditor);
  const saveTask = useStore((s) => s.saveTask);
  const removeTask = useStore((s) => s.removeTask);
  const settings = useStore((s) => s.settings);

  const [form, setForm] = useState<TaskInput>({ title: '' });
  const [tagText, setTagText] = useState('');
  const [preview, setPreview] = useState<{ nextAt: number | null; label: string } | null>(null);

  const open = editing !== null;

  // 打开时初始化表单。默认值取自设置（默认重复规则、默认提醒时间）。
  useEffect(() => {
    if (!editing) return;
    setForm({
      ...editing,
      repeatType: editing.repeatType ?? settings.defaultRepeatType,
      repeatInterval: editing.repeatInterval ?? 1,
      repeatUnit: editing.repeatUnit ?? 'day',
      repeatWeekdays: editing.repeatWeekdays ?? [],
      priority: editing.priority ?? 2,
      tags: editing.tags ?? [],
    });
    setTagText((editing.tags ?? []).join(', '));
    setPreview(null);
  }, [editing, settings.defaultRepeatType]);

  /**
   * 下一次出现时间由 Rust 计算，不在前端重算。
   *
   * 重复规则的语义（短月收敛、逾期不补实例、工作日跳过周末）只应该实现一遍，
   * 前端再写一份必然漂移。这里防抖 300ms，避免每敲一个字符就跨一次 IPC。
   */
  useEffect(() => {
    if (!open || !form.title.trim()) return;
    const t = window.setTimeout(() => {
      void ipc
        .previewNextOccurrence(form)
        .then(setPreview)
        .catch(() => setPreview(null));
    }, 300);
    return () => window.clearTimeout(t);
  }, [open, form]);

  const patch = (p: Partial<TaskInput>) => setForm((f) => ({ ...f, ...p }));

  /** 选择提醒提前量时，以 dueAt 为基准算出绝对提醒时间 */
  const applyRemindPreset = (minutes: number | null) => {
    if (minutes === null) {
      patch({ remindAt: null });
      return;
    }
    const base = form.dueAt ?? Date.now() + 3600_000;
    patch({ remindAt: base - minutes * 60_000 });
  };

  const activePreset = useMemo(() => {
    if (form.remindAt == null) return null;
    if (form.dueAt == null) return undefined;
    const diffMin = Math.round((form.dueAt - form.remindAt) / 60_000);
    return REMIND_PRESETS.find((p) => p.minutes === diffMin)?.minutes ?? undefined;
  }, [form.remindAt, form.dueAt]);

  const submit = () => {
    if (!form.title.trim()) return;
    void saveTask({ ...form, title: form.title.trim(), tags: parseTags(tagText) });
  };

  return (
    <Dialog
      open={open}
      onClose={closeEditor}
      title={form.id ? '编辑任务' : '新建任务'}
      width="min(94%, 380px)"
      footer={
        <>
          {form.id ? (
            <Button
              variant="danger"
              size="sm"
              className="mr-auto"
              onClick={() => {
                void removeTask(form.id!);
                closeEditor();
              }}
            >
              删除
            </Button>
          ) : null}
          <Button variant="ghost" size="sm" onClick={closeEditor}>
            取消
          </Button>
          <Button variant="primary" size="sm" onClick={submit} disabled={!form.title.trim()}>
            保存
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <div>
          <Label>标题</Label>
          <Input
            autoFocus
            value={form.title}
            placeholder="要做什么？"
            onChange={(e) => patch({ title: e.target.value })}
            onKeyDown={(e) => {
              // 标题框里回车直接保存：这是最高频的操作，不该逼用户去点按钮
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                submit();
              }
            }}
            className="mt-1"
          />
        </div>

        <div>
          <Label>备注</Label>
          <Textarea
            rows={2}
            value={form.note ?? ''}
            placeholder="补充说明（可选）"
            onChange={(e) => patch({ note: e.target.value })}
            className="mt-1"
          />
        </div>

        <div className="flex items-center gap-2">
          <div className="flex items-center gap-1.5 text-[hsl(var(--muted-foreground))]">
            <Tag size={12} />
            <Label>标签</Label>
          </div>
          <Input
            value={tagText}
            placeholder="用逗号分隔，如 工作, 生活"
            onChange={(e) => setTagText(e.target.value)}
            className="h-7 flex-1 text-[12px]"
          />
        </div>

        <div className="flex items-center justify-between">
          <Label>优先级</Label>
          <Segmented
            value={String(form.priority ?? 2)}
            onChange={(v) => patch({ priority: Number(v) })}
            options={PRIORITIES.map((p) => ({ value: String(p.value), label: p.label }))}
          />
        </div>

        <div className="h-px bg-[hsl(var(--border)/0.5)]" />

        {/* ── 时间 ── */}
        <div className="flex items-center gap-1.5 text-[hsl(var(--muted-foreground))]">
          <CalendarClock size={12} />
          <Label>出现时间</Label>
        </div>
        <div className="flex items-center gap-2">
          <Input
            type="datetime-local"
            value={toDatetimeLocal(form.dueAt ?? null)}
            onChange={(e) => patch({ dueAt: fromDatetimeLocal(e.target.value) })}
            className="h-7 flex-1 text-[12px]"
          />
          {form.dueAt ? (
            <Button variant="ghost" size="icon-sm" title="清除" onClick={() => patch({ dueAt: null })}>
              ✕
            </Button>
          ) : null}
        </div>

        <div className="flex items-center gap-1.5 text-[hsl(var(--muted-foreground))]">
          <Bell size={12} />
          <Label>提醒</Label>
        </div>
        <div className="flex flex-wrap gap-1.5">
          {REMIND_PRESETS.map((p) => (
            <Chip
              key={p.label}
              active={activePreset === p.minutes || (p.minutes === null && form.remindAt == null)}
              onClick={() => applyRemindPreset(p.minutes)}
            >
              {p.label}
            </Chip>
          ))}
        </div>
        <Input
          type="datetime-local"
          value={toDatetimeLocal(form.remindAt ?? null)}
          onChange={(e) => patch({ remindAt: fromDatetimeLocal(e.target.value) })}
          className="h-7 text-[12px]"
        />

        <div className="h-px bg-[hsl(var(--border)/0.5)]" />

        {/* ── 重复 ── */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-1.5 text-[hsl(var(--muted-foreground))]">
            <Repeat size={12} />
            <Label>重复</Label>
          </div>
          <Select
            value={form.repeatType ?? 'once'}
            onChange={(v) => patch({ repeatType: v })}
            options={REPEAT_OPTIONS}
            label="重复规则"
          />
        </div>

        {form.repeatType === 'weekly' ? (
          <div className="flex items-center gap-1.5">
            {DOW.map((d) => {
              const on = (form.repeatWeekdays ?? []).includes(d.v);
              return (
                <Chip
                  key={d.v}
                  active={on}
                  onClick={() => {
                    const cur = form.repeatWeekdays ?? [];
                    patch({
                      repeatWeekdays: on ? cur.filter((x) => x !== d.v) : [...cur, d.v].sort((a, b) => a - b),
                    });
                  }}
                >
                  周{d.l}
                </Chip>
              );
            })}
          </div>
        ) : null}

        {form.repeatType === 'monthly' ? (
          <div className="flex items-center gap-2">
            <Label>每月第</Label>
            <Input
              type="number"
              min={1}
              max={31}
              value={form.repeatMonthday ?? -1}
              onChange={(e) => patch({ repeatMonthday: Number(e.target.value) })}
              className="h-7 w-20 text-[12px]"
            />
            <span className="text-[11px] text-[hsl(var(--muted-foreground))]">
              日（填 -1 表示沿用出现时间的日）
            </span>
          </div>
        ) : null}

        {form.repeatType === 'interval' ? (
          <div className="flex items-center gap-2">
            <Label>每</Label>
            <Input
              type="number"
              min={1}
              max={365}
              value={form.repeatInterval ?? 1}
              onChange={(e) => patch({ repeatInterval: Number(e.target.value) })}
              className="h-7 w-16 text-[12px]"
            />
            <Select
              value={(form.repeatUnit ?? 'day') as RepeatUnit}
              onChange={(v) => patch({ repeatUnit: v })}
              options={[
                { value: 'day' as RepeatUnit, label: '天' },
                { value: 'week' as RepeatUnit, label: '周' },
                { value: 'month' as RepeatUnit, label: '月' },
              ]}
              label="间隔单位"
            />
          </div>
        ) : null}

        {form.repeatType === 'cron' ? (
          <div>
            <Input
              value={form.repeatCron ?? ''}
              placeholder="分 时 日 月 周，如 30 8 * * 1-5"
              onChange={(e) => patch({ repeatCron: e.target.value })}
              className="h-7 font-mono text-[12px]"
            />
            <p className="mt-1 text-[10px] leading-snug text-[hsl(var(--muted-foreground))]">
              支持 * / a / a-b / */n / 逗号列表。日与周同时限定时取「或」，与标准 cron 一致。
            </p>
          </div>
        ) : null}

        {form.repeatType !== 'once' ? (
          <div className="flex items-center gap-2">
            <Label>结束于</Label>
            <Input
              type="date"
              value={form.repeatEndAt ? toDatetimeLocal(form.repeatEndAt).slice(0, 10) : ''}
              onChange={(e) => {
                const v = e.target.value;
                patch({ repeatEndAt: v ? new Date(`${v}T23:59:59`).getTime() : null });
              }}
              className="h-7 w-40 text-[12px]"
            />
            <span className="text-[11px] text-[hsl(var(--muted-foreground))]">留空表示一直重复</span>
          </div>
        ) : null}

        {/* 下一次出现时间预览：由 Rust 计算，前端只展示 */}
        <div
          className={cn(
            'rounded-[10px] border border-[hsl(var(--border)/0.5)] bg-[hsl(var(--surface)/0.4)] px-2.5 py-2',
            'text-[11px] leading-relaxed text-[hsl(var(--muted-foreground))]',
          )}
        >
          <div>
            规则：<span className="text-[hsl(var(--foreground))]">{describeRepeat({
              repeatType: form.repeatType ?? 'once',
              repeatInterval: form.repeatInterval ?? 1,
              repeatUnit: form.repeatUnit ?? 'day',
              repeatWeekdays: form.repeatWeekdays ?? [],
              repeatMonthday: form.repeatMonthday ?? null,
              repeatCron: form.repeatCron ?? null,
            })}</span>
          </div>
          {form.repeatType !== 'once' ? (
            <div>
              下次出现：
              <span className="text-[hsl(var(--foreground))]">
                {preview ? (preview.nextAt ? formatDue(preview.nextAt) : '不再出现') : '计算中…'}
              </span>
            </div>
          ) : null}
        </div>
      </div>
    </Dialog>
  );
}

function parseTags(text: string): string[] {
  return Array.from(
    new Set(
      text
        .split(/[,，]/)
        .map((s) => s.trim())
        .filter(Boolean),
    ),
  );
}
