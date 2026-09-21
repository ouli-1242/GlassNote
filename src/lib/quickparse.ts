/**
 * 快速添加的自然语言解析。
 *
 * 目标只有一个：让"随手记一条"不用打开编辑器。
 * 支持的写法刻意保持极小：
 *
 *   买牛奶 #生活 #采购 !3 明天 18:00
 *   ├─ 标题        买牛奶
 *   ├─ 标签        #生活 #采购
 *   ├─ 优先级      !3（!1 低 … !4 紧急，!0 清除）
 *   └─ 出现时间    明天 18:00（省略时间默认 09:00）
 *
 * 日期只认「今天/明天/后天/周X/下周X」。不引入日期解析库 ——
 * 完整的自然语言日期库体积大、歧义多，而便签场景里这几个词覆盖了绝大多数输入。
 */

import { MS_DAY, startOfDay } from './utils';
import type { TaskInput } from '../types';

const DOW: Record<string, number> = { 一: 1, 二: 2, 三: 3, 四: 4, 五: 5, 六: 6, 日: 7, 天: 7 };

export interface QuickParse {
  input: TaskInput;
  /** 解析出的可读摘要，用于在输入框下方给用户确认 */
  hints: string[];
}

export function parseQuick(raw: string): QuickParse {
  let text = raw.trim();
  const hints: string[] = [];

  // ── 标签 ──
  const tags: string[] = [];
  text = text.replace(/#([^\s#!]+)/g, (_, t: string) => {
    tags.push(t);
    return ' ';
  });
  if (tags.length) hints.push(`标签 ${tags.join('、')}`);

  // ── 优先级 ──
  let priority: number | undefined;
  text = text.replace(/!([0-4])(?=\s|$)/g, (_, d: string) => {
    priority = Number(d);
    return ' ';
  });
  if (priority !== undefined) {
    hints.push(`优先级 ${['无', '低', '普通', '高', '紧急'][priority]}`);
  }

  // ── 日期 ──
  let dueAt: number | null = null;
  const base = startOfDay(Date.now());
  const dateRe = /(今天|明天|后天|下?周([一二三四五六日天]))(?:\s*(\d{1,2})[:：](\d{2}))?/;

  const m = dateRe.exec(text);
  if (m) {
    let dayOffset = 0;
    if (m[1] === '明天') dayOffset = 1;
    else if (m[1] === '后天') dayOffset = 2;
    else if (m[2]) {
      // 周X：算出到下一个该星期几的天数；"下周X" 则再推一周
      const target = DOW[m[2]];
      const cur = new Date(base).getDay() || 7; // 周日按 7 算
      dayOffset = ((target - cur + 7) % 7) || 7;
      if (m[1].startsWith('下周')) dayOffset += 7;
    }

    const hh = m[3] ? Number(m[3]) : 9;
    const mm = m[4] ? Number(m[4]) : 0;
    const d = new Date(base + dayOffset * MS_DAY);
    d.setHours(hh, mm, 0, 0);
    dueAt = d.getTime();
    hints.push(`出现 ${m[0].trim()}`);
    text = text.replace(m[0], ' ');
  }

  return {
    input: {
      title: text.replace(/\s+/g, ' ').trim(),
      tags: tags.length ? tags : undefined,
      priority,
      dueAt,
    },
    hints,
  };
}
