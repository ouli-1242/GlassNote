/**
 * 前后端共享的数据契约。
 *
 * 这里的字段名必须与 src-tauri/src/models.rs 里 `#[serde(rename_all = "camelCase")]`
 * 的输出完全一致。Rust 侧是唯一的事实来源，本文件只是它的 TypeScript 投影。
 *
 * 时间字段一律是 Unix 毫秒（number）。之所以不用 Date：
 * 序列化更省、比较更直观，且避免 JSON 往返时把本地时间误解成 UTC。
 */

// ─────────────────────────────── 任务 ───────────────────────────────

export type RepeatType = 'once' | 'daily' | 'weekly' | 'monthly' | 'weekday' | 'interval' | 'cron';
export type RepeatUnit = 'day' | 'week' | 'month';
export type TaskStatus = 'todo' | 'done';

export interface Task {
  id: number;
  title: string;
  note: string | null;
  status: TaskStatus;
  /** 0=无 1=低 2=普通 3=高 4=紧急 */
  priority: number;
  tags: string[];
  /** 出现/截止时间 */
  dueAt: number | null;
  /** 提醒时间，可独立于 dueAt */
  remindAt: number | null;
  repeatType: RepeatType;
  repeatInterval: number;
  repeatUnit: RepeatUnit;
  /** ISO 星期 1..7 */
  repeatWeekdays: number[];
  /** 1..31；-1 表示沿用 dueAt 的日 */
  repeatMonthday: number | null;
  repeatEndAt: number | null;
  repeatCron: string | null;
  parentId: number | null;
  pinned: boolean;
  sortOrder: number;
  createdAt: number;
  updatedAt: number;
  completedAt: number | null;
  deletedAt: number | null;
  remindFiredAt: number | null;
}

export interface TaskInput {
  id?: number;
  title: string;
  note?: string | null;
  priority?: number;
  tags?: string[];
  dueAt?: number | null;
  remindAt?: number | null;
  repeatType?: RepeatType;
  repeatInterval?: number;
  repeatUnit?: RepeatUnit;
  repeatWeekdays?: number[];
  repeatMonthday?: number | null;
  repeatEndAt?: number | null;
  repeatCron?: string | null;
  parentId?: number | null;
}

export type TaskView = 'today' | 'all' | 'done' | 'trash' | 'upcoming';
export type TaskSort = 'due' | 'priority' | 'created' | 'manual';

export interface TaskQuery {
  view?: TaskView;
  search?: string;
  tag?: string;
  priority?: number;
  sort?: TaskSort;
}

// ─────────────────────────────── 备忘录 ───────────────────────────────

export interface Memo {
  id: number;
  title: string;
  content: string;
  color: string;
  collapsed: boolean;
  pinned: boolean;
  sortOrder: number;
  createdAt: number;
  updatedAt: number;
  deletedAt: number | null;
}

export interface MemoInput {
  id?: number;
  title: string;
  content?: string;
  color?: string;
  collapsed?: boolean;
  pinned?: boolean;
}

// ─────────────────────────────── 历史 / 例外 ───────────────────────────────

export interface CompletedLog {
  id: number;
  taskId: number;
  taskTitle: string;
  completedAt: number;
  action: 'complete' | 'uncomplete' | 'skip';
  prevDueAt: number | null;
}

// ─────────────────────────────── 玻璃效果 ───────────────────────────────

export interface GlassInfo {
  /** 实际生效的效果：mica | tabbed | acrylic | blur | css */
  effect: string;
  /** 由生效的效果推导，而非读取系统版本号 —— 见 models.rs 的说明 */
  isWindows11: boolean;
  rounded: boolean;
  /** 前端应使用的面板圆角半径（px），由 Rust 依据系统能力决定 */
  cornerRadius: number;
}

/**
 * 主题只有白天与黑夜两套。
 *
 * 刻意**没有**"跟随系统"：它会引入一个 matchMedia 监听，
 * 而且系统明暗切换时还要重刷一遍 Mica 与整套变量。
 * 两套固定值更省，也让界面在任何时候都是用户选定的样子。
 */
export type ThemeMode = 'dark' | 'light';

// ─────────────────────────────── 设置 ───────────────────────────────

export interface AppSettings {
  autostart: boolean;
  /** 开机自启后延迟多少秒才初始化（避开开机风暴） */
  autostartDelaySec: number;
  /** 0.2 ~ 1.0 */
  opacity: number;
  theme: ThemeMode;
  fontScale: number;
  defaultRepeatType: RepeatType;
  /** 默认提前多少分钟提醒，0 表示不提醒 */
  defaultRemindOffsetMin: number;
  alwaysOnTop: boolean;
  showTrayIcon: boolean;
  closeToTray: boolean;
  /**
   * 隐藏时销毁主窗口以释放 WebView2 占用的内存。
   * 关掉它换取"显示更快"，代价是常驻内存会明显上升。
   */
  releaseMemoryWhenHidden: boolean;
  notificationsEnabled: boolean;
  /** 免打扰时段，HH:MM */
  dndStart: string;
  dndEnd: string;
  shortcutCapture: string;
  shortcutToggle: string;
  language: 'zh' | 'en';
  lockEnabled: boolean;
  autoBackupEnabled: boolean;
  backupKeep: number;
  /** 回收站自动清理天数 */
  trashKeepDays: number;
  /** 已完成任务自动清理天数，0 表示不清理 */
  doneKeepDays: number;
  /** 主面板折叠状态（只留顶栏） */
  windowCollapsed: boolean;
}

export const DEFAULT_SETTINGS: AppSettings = {
  autostart: false,
  autostartDelaySec: 5,
  opacity: 0.96,
  theme: 'light',
  fontScale: 1,
  defaultRepeatType: 'once',
  defaultRemindOffsetMin: 0,
  alwaysOnTop: false,
  showTrayIcon: true,
  closeToTray: true,
  releaseMemoryWhenHidden: true,
  notificationsEnabled: true,
  dndStart: '22:30',
  dndEnd: '07:30',
  shortcutCapture: 'CommandOrControl+Alt+N',
  shortcutToggle: 'CommandOrControl+Shift+Space',
  language: 'zh',
  lockEnabled: false,
  autoBackupEnabled: true,
  backupKeep: 7,
  trashKeepDays: 30,
  doneKeepDays: 0,
  windowCollapsed: false,
};

/** 引导期由 Rust 通过 initialization_script 注入，React 挂载前就可读 */
export interface BootPayload {
  theme: ThemeMode;
  opacity: number;
  fontScale: number;
  portable: boolean;
  autostartRun: boolean;
  windowCollapsed: boolean;
}

declare global {
  interface Window {
    __GLASSNOTE_BOOT__?: BootPayload;
  }
}
