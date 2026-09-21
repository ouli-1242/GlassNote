import { type ClassValue, clsx } from 'clsx';
import { twMerge } from 'tailwind-merge';

/** shadcn/ui 约定的类名合并工具：先 clsx 去假值，再由 tailwind-merge 解冲突 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export const MS_MINUTE = 60_000;
export const MS_HOUR = 3_600_000;
export const MS_DAY = 86_400_000;

export function clamp(v: number, lo: number, hi: number) {
  return Math.min(hi, Math.max(lo, v));
}

export function startOfDay(ms: number) {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

export function endOfDay(ms: number) {
  return startOfDay(ms) + MS_DAY - 1;
}

const WEEKDAYS = ['周日', '周一', '周二', '周三', '周四', '周五', '周六'];

function pad(n: number) {
  return n < 10 ? `0${n}` : String(n);
}

export function formatTime(ms: number) {
  const d = new Date(ms);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/**
 * 把到期时间写成人话。
 *
 * 便签场景里"今天/明天/后天"远比完整日期好读，所以优先用相对说法；
 * 超过一周才退化到日期。同一天内不再显示日期部分，减少视觉噪音。
 */
export function formatDue(ms: number | null, now = Date.now()): string {
  if (ms == null) return '';
  const today = startOfDay(now);
  const target = startOfDay(ms);
  const diffDays = Math.round((target - today) / MS_DAY);

  if (diffDays === 0) return formatTime(ms);
  if (diffDays === 1) return `明天 ${formatTime(ms)}`;
  if (diffDays === 2) return `后天 ${formatTime(ms)}`;
  if (diffDays === -1) return `昨天 ${formatTime(ms)}`;

  const d = new Date(ms);
  if (diffDays > 2 && diffDays < 7) return `${WEEKDAYS[d.getDay()]} ${formatTime(ms)}`;

  const sameYear = d.getFullYear() === new Date(now).getFullYear();
  const datePart = sameYear
    ? `${d.getMonth() + 1}月${d.getDate()}日`
    : `${d.getFullYear()}/${d.getMonth() + 1}/${d.getDate()}`;
  return `${datePart} ${formatTime(ms)}`;
}

/** 逾期与否。无到期时间不算逾期。 */
export function isOverdue(dueAt: number | null, now = Date.now()) {
  return dueAt != null && dueAt < now;
}

/** 是否落在今天（含逾期未完成的情况交给调用方判断） */
export function isToday(ms: number | null, now = Date.now()) {
  return ms != null && startOfDay(ms) === startOfDay(now);
}

// ── <input type="datetime-local"> 的双向转换 ──────────────────────────────
// datetime-local 的值是"本地墙钟"字符串且没有时区，直接喂时间戳会偏 8 小时，
// 必须显式按本地时间拼装。

export function toDatetimeLocal(ms: number | null): string {
  if (ms == null) return '';
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function fromDatetimeLocal(value: string): number | null {
  if (!value) return null;
  const t = new Date(value).getTime();
  return Number.isNaN(t) ? null : t;
}

export function toDateLocal(ms: number | null): string {
  if (ms == null) return '';
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function fromDateLocal(value: string): number | null {
  if (!value) return null;
  const [y, m, d] = value.split('-').map(Number);
  if (!y || !m || !d) return null;
  const dt = new Date(y, m - 1, d, 23, 59, 59, 999); // 结束日期取当天最后一刻
  return dt.getTime();
}

// ── 重复规则的中文描述 ───────────────────────────────────────────────────
// 注意：真正决定"下一次什么时候出现"的是 Rust 侧 recurrence.rs。
// 这里只负责把用户选的规则念出来给 UI 看，不参与任何计算 ——
// 规则语义只实现一遍，避免前后端算法漂移。

const DOW_SHORT = ['一', '二', '三', '四', '五', '六', '日'];

export interface RepeatLike {
  repeatType: string;
  repeatInterval: number;
  repeatUnit: string;
  repeatWeekdays: number[];
  repeatMonthday: number | null;
  repeatCron: string | null;
}

export function describeRepeat(r: RepeatLike): string {
  switch (r.repeatType) {
    case 'daily':
      return '每天';
    case 'weekday':
      return '每个工作日';
    case 'weekly':
      if (!r.repeatWeekdays.length) return '每周';
      return `每${r.repeatWeekdays
        .filter((d) => d >= 1 && d <= 7)
        .sort((a, b) => a - b)
        .map((d) => `周${DOW_SHORT[d - 1]}`)
        .join('、')}`;
    case 'monthly':
      return r.repeatMonthday && r.repeatMonthday >= 1 ? `每月 ${r.repeatMonthday} 日` : '每月';
    case 'interval': {
      const unit = r.repeatUnit === 'week' ? '周' : r.repeatUnit === 'month' ? '月' : '天';
      return `每 ${r.repeatInterval} ${unit}`;
    }
    case 'cron':
      return r.repeatCron ? `Cron：${r.repeatCron}` : 'Cron';
    default:
      return '不重复';
  }
}

export function formatBytes(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

export function formatDateTime(ms: number) {
  const d = new Date(ms);
  return `${d.getFullYear()}/${pad(d.getMonth() + 1)}/${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** 生成一个短 id，仅用于前端列表 key（真实主键由 SQLite 分配） */
export function tempId() {
  return `t${Math.random().toString(36).slice(2, 10)}`;
}
