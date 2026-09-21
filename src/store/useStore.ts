/**
 * 全局状态中枢。
 *
 * 分工约定：
 *   - **SQLite 是唯一事实来源**。store 里的 tasks/memos 只是它的当前投影，
 *     任何写操作都先落库、再回读，不做乐观更新 —— 便签应用的数据量小到
 *     回读成本可以忽略，而乐观更新要处理回滚、乱序、失败重试，不划算。
 *   - 唯一例外是"完成任务的动画窗口"：先播动画再落库，因为动画本身就是
 *     用户体验的一部分，等 IPC 回来再动会有明显迟滞。
 */

import { create } from 'zustand';

import * as ipc from '../lib/ipc';
import {
  DEFAULT_SETTINGS,
  type AppSettings,
  type CompletedLog,
  type GlassInfo,
  type Memo,
  type Task,
  type TaskInput,
  type TaskSort,
  type TaskView,
} from '../types';

export interface Toast {
  id: string;
  kind: 'info' | 'success' | 'error';
  message: string;
  /** 可选的操作按钮，例如"撤销" */
  action?: { label: string; run: () => void };
  /** 毫秒；0 表示不自动消失 */
  ttl: number;
}

/** 完成动画时长，必须与 styles.css 里 `.task-done` 的 animation 一致 */
export const DONE_ANIM_MS = 420;
/** 撤销窗口，规格要求 5 秒 */
export const UNDO_WINDOW_MS = 5000;

interface State {
  ready: boolean;
  fatalError: string | null;

  tasks: Task[];
  memos: Memo[];
  logs: CompletedLog[];
  settings: AppSettings;
  glass: GlassInfo | null;

  view: TaskView;
  search: string;
  tagFilter: string | null;
  priorityFilter: number | null;
  sort: TaskSort;

  /** 正在播完成动画的任务 id，这些行仍要渲染但已不可交互 */
  completing: number[];
  editing: TaskInput | null;
  settingsOpen: boolean;
  locked: boolean;

  toasts: Toast[];
  undo: { logId: number; title: string; nextAt: number | null; recurring: boolean } | null;

  init: () => Promise<void>;
  reloadTasks: () => Promise<void>;
  reloadMemos: () => Promise<void>;
  reloadLogs: () => Promise<void>;

  setView: (v: TaskView) => void;
  setSearch: (s: string) => void;
  setTagFilter: (t: string | null) => void;
  setPriorityFilter: (p: number | null) => void;
  setSort: (s: TaskSort) => void;

  openEditor: (input: TaskInput | null) => void;
  closeEditor: () => void;
  setSettingsOpen: (open: boolean) => void;

  saveTask: (input: TaskInput) => Promise<void>;
  complete: (id: number) => Promise<void>;
  undoComplete: () => Promise<void>;
  skip: (id: number) => Promise<void>;
  removeTask: (id: number) => Promise<void>;
  restoreTask: (id: number) => Promise<void>;
  purgeTask: (id: number) => Promise<void>;
  emptyTrash: () => Promise<void>;
  togglePin: (id: number) => Promise<void>;
  snooze: (id: number, minutes: number) => Promise<void>;
  reorder: (ids: number[]) => Promise<void>;

  saveMemo: (input: Parameters<typeof ipc.saveMemo>[0]) => Promise<Memo | null>;
  toggleMemoCollapsed: (id: number, collapsed: boolean) => Promise<void>;
  removeMemo: (id: number) => Promise<void>;
  restoreMemo: (id: number) => Promise<void>;
  toggleMemoPin: (id: number) => Promise<void>;
  reorderMemos: (ids: number[]) => Promise<void>;

  updateSettings: (patch: Partial<AppSettings>) => Promise<void>;
  resetAllSettings: () => Promise<void>;

  pushToast: (t: Omit<Toast, 'id'>) => string;
  dismissToast: (id: string) => void;
}

/**
 * 把圆角半径写到 CSS 变量。
 *
 * 关键约束：**CSS 半径必须小于等于 DWM 裁切的半径**。
 * DWM 只有固定档位（Win11 的 ROUND 约 8px），没有自定义半径；
 * 若 CSS 画得比它大，两者之间那一圈就会既没有系统背景、也没有面板着色，
 * 四角直接露出桌面。所以这里按探测结果取 8 或 0，而不是写死 20。
 */
function applyRadius(glass: GlassInfo | null) {
  // 半径由 Rust 依据系统能力给出，前端不硬编码 —— 它必须 <= DWM 的裁切半径
  const px = glass?.cornerRadius ?? 0;
  document.documentElement.style.setProperty('--panel-radius', `${px}px`);
}

/** 任务增删改之后同步托盘徽标。只在真正改变待办数量时调用。 */
function syncTray() {
  void ipc.refreshTray().catch(() => {
    /* 托盘不可用时忽略：徽标只是附加信息，不该影响主流程 */
  });
}

/** 把设置镜像到 localStorage，供首帧引导脚本同步读取（Rust 注入失败时兜底）。
 *  这些值必须在 React 挂载前生效，异步 IPC 来不及。 */
function mirrorBoot(settings: AppSettings) {
  try {
    localStorage.setItem(
      'glassnote.boot',
      JSON.stringify({
        theme: settings.theme,
        opacity: settings.opacity,
        fontScale: settings.fontScale,
      }),
    );
  } catch {
    /* 存储被禁用时静默降级：只是会闪一下，不影响功能 */
  }
}

export const useStore = create<State>((set, get) => ({
  ready: false,
  fatalError: null,

  tasks: [],
  memos: [],
  logs: [],
  settings: DEFAULT_SETTINGS,
  glass: null,

  view: 'today',
  search: '',
  tagFilter: null,
  priorityFilter: null,
  sort: 'due',

  completing: [],
  editing: null,
  settingsOpen: false,
  locked: false,

  toasts: [],
  undo: null,

  // ─────────────────────────────── 初始化 ───────────────────────────────

  async init() {
    try {
      const settings = await ipc.getSettings();
      set({ settings });
      mirrorBoot(settings);
      // 首帧的主题来自 Rust 注入的引导缓存，那份缓存有可能落后于库
      // （比如上次是异常退出）。这里按库里的值再对齐一次，
      // 让缓存不一致能自愈，而不是一直显示错误主题直到用户手动切换。
      applyThemeClass(settings.theme);

      const glass = await ipc.applyGlass();
      set({ glass });
      applyRadius(glass);

      await Promise.all([get().reloadTasks(), get().reloadMemos(), get().reloadLogs()]);
      set({ ready: true });

      if (settings.lockEnabled) set({ locked: true });
    } catch (e) {
      set({ fatalError: String(e), ready: true });
    }
  },

  async reloadTasks() {
    const { view, search, tagFilter, priorityFilter, sort } = get();
    const tasks = await ipc.listTasks({
      view,
      search: search || undefined,
      tag: tagFilter ?? undefined,
      priority: priorityFilter ?? undefined,
      sort,
    });
    set({ tasks });
  },

  async reloadMemos() {
    set({ memos: await ipc.listMemos() });
  },

  async reloadLogs() {
    set({ logs: await ipc.listCompletedLogs(200) });
  },

  // ─────────────────────────────── 筛选 ───────────────────────────────

  setView(v) {
    set({ view: v });
    void get().reloadTasks();
  },
  setSearch(s) {
    set({ search: s });
    void get().reloadTasks();
  },
  setTagFilter(t) {
    set({ tagFilter: t });
    void get().reloadTasks();
  },
  setPriorityFilter(p) {
    set({ priorityFilter: p });
    void get().reloadTasks();
  },
  setSort(s) {
    set({ sort: s });
    void get().reloadTasks();
  },

  openEditor(input) {
    set({ editing: input ?? { title: '' } });
  },
  closeEditor() {
    set({ editing: null });
  },
  setSettingsOpen(open) {
    set({ settingsOpen: open });
  },

  // ─────────────────────────────── 任务 ───────────────────────────────

  async saveTask(input) {
    try {
      if (input.id) await ipc.updateTask(input);
      else await ipc.createTask(input);
      set({ editing: null });
      await get().reloadTasks();
      syncTray();
    } catch (e) {
      get().pushToast({ kind: 'error', message: `保存失败：${e}`, ttl: 4000 });
    }
  },

  /**
   * 完成一个任务。
   *
   * 顺序很关键：先标记 completing 触发动画，同时立刻落库（保证数据一致性），
   * 动画结束后才把行从列表移除。这样既没有视觉迟滞，也不会出现
   * "动画播完了但数据库还没写"的窗口期。
   */
  async complete(id) {
    const task = get().tasks.find((t) => t.id === id);
    set({ completing: [...get().completing, id] });

    try {
      const result = await ipc.completeTask(id);
      await new Promise((r) => setTimeout(r, DONE_ANIM_MS));

      set({
        completing: get().completing.filter((x) => x !== id),
        tasks: get().tasks.filter((t) => t.id !== id),
        undo: {
          logId: result.logId,
          title: task?.title ?? '',
          nextAt: result.nextAt,
          recurring: result.recurring,
        },
      });

      const nextLabel = result.nextAt
        ? `，下次 ${new Date(result.nextAt).toLocaleString('zh-CN', {
            month: 'numeric',
            day: 'numeric',
            hour: '2-digit',
            minute: '2-digit',
          })}`
        : '';
      const toastId = get().pushToast({
        kind: 'success',
        message: result.recurring ? `已完成${nextLabel}` : '已完成',
        action: { label: '撤销', run: () => void get().undoComplete() },
        ttl: UNDO_WINDOW_MS,
      });

      // 撤销窗口过期后清掉 undo 状态，避免"过了十分钟还能撤销"
      setTimeout(() => {
        const cur = get().undo;
        if (cur && cur.logId === result.logId) set({ undo: null });
        get().dismissToast(toastId);
      }, UNDO_WINDOW_MS);

      await get().reloadTasks();
      syncTray();
    } catch (e) {
      set({ completing: get().completing.filter((x) => x !== id) });
      get().pushToast({ kind: 'error', message: `操作失败：${e}`, ttl: 4000 });
    }
  },

  async undoComplete() {
    const undo = get().undo;
    if (!undo) return;
    try {
      await ipc.uncompleteTask(undo.logId);
      set({ undo: null });
      await get().reloadTasks();
      syncTray();
      get().pushToast({ kind: 'info', message: '已撤销', ttl: 2000 });
    } catch (e) {
      get().pushToast({ kind: 'error', message: `撤销失败：${e}`, ttl: 4000 });
    }
  },

  async skip(id) {
    try {
      await ipc.skipTaskOccurrence(id);
      await get().reloadTasks();
      syncTray();
      get().pushToast({ kind: 'info', message: '已跳过本次', ttl: 2000 });
    } catch (e) {
      get().pushToast({ kind: 'error', message: `跳过失败：${e}`, ttl: 4000 });
    }
  },

  async removeTask(id) {
    try {
      await ipc.deleteTask(id);
      await get().reloadTasks();
      syncTray();
    } catch (e) {
      get().pushToast({ kind: 'error', message: `删除失败：${e}`, ttl: 4000 });
    }
  },

  async restoreTask(id) {
    await ipc.restoreTask(id);
    await get().reloadTasks();
    syncTray();
  },

  async purgeTask(id) {
    await ipc.purgeTask(id);
    await get().reloadTasks();
    syncTray();
  },

  async emptyTrash() {
    const n = await ipc.emptyTrash();
    await get().reloadTasks();
    syncTray();
    get().pushToast({ kind: 'info', message: `已清空回收站（${n} 项）`, ttl: 2500 });
  },

  async togglePin(id) {
    const task = get().tasks.find((t) => t.id === id);
    if (!task) return;
    await ipc.setTaskPinned(id, !task.pinned);
    await get().reloadTasks();
  },

  async snooze(id, minutes) {
    await ipc.snoozeTask(id, minutes);
    await get().reloadTasks();
    get().pushToast({ kind: 'info', message: `已推迟 ${minutes} 分钟提醒`, ttl: 2500 });
  },

  async reorder(ids) {
    await ipc.reorderTasks(ids);
    await get().reloadTasks();
  },

  // ─────────────────────────────── 备忘录 ───────────────────────────────

  async saveMemo(input) {
    try {
      const memo = await ipc.saveMemo(input);
      await get().reloadMemos();
      return memo;
    } catch (e) {
      get().pushToast({ kind: 'error', message: `保存失败：${e}`, ttl: 4000 });
      return null;
    }
  },

  async toggleMemoCollapsed(id, collapsed) {
    // 折叠状态必须持久化（规格要求重启后保持），所以走一次 IPC 而不是纯前端状态
    await ipc.setMemoCollapsed(id, collapsed);
    set({ memos: get().memos.map((m) => (m.id === id ? { ...m, collapsed } : m)) });
  },

  async removeMemo(id) {
    await ipc.deleteMemo(id);
    await get().reloadMemos();
  },

  async restoreMemo(id) {
    await ipc.restoreMemo(id);
    await get().reloadMemos();
  },

  async toggleMemoPin(id) {
    const memo = get().memos.find((m) => m.id === id);
    if (!memo) return;
    await ipc.setMemoPinned(id, !memo.pinned);
    await get().reloadMemos();
  },

  async reorderMemos(ids) {
    await ipc.reorderMemos(ids);
    await get().reloadMemos();
  },

  // ─────────────────────────────── 设置 ───────────────────────────────

  async updateSettings(patch) {
    const next = await ipc.setSettings(patch);
    set({ settings: next });
    mirrorBoot(next);

    // 需要立刻反映到系统层面的几项
    if (patch.opacity !== undefined) {
      document.documentElement.style.setProperty('--panel-opacity', String(next.opacity));
    }
    if (patch.fontScale !== undefined) {
      document.documentElement.style.setProperty('--font-scale', String(next.fontScale));
    }
    if (patch.theme !== undefined) {
      // Mica 的明暗由系统按窗口设置绘制，换主题必须让 Rust 重新应用一次效果，
      // 否则暗色主题会配上一块浅色 Mica 背景。
      applyThemeClass(next.theme);
      const info = await ipc.applyGlass();
      set({ glass: info });
      applyRadius(info);
    }
    if (patch.alwaysOnTop !== undefined) await ipc.setAlwaysOnTop(next.alwaysOnTop);
    if (patch.autostart !== undefined) await ipc.setAutostart(next.autostart);
    if (patch.shortcutCapture !== undefined || patch.shortcutToggle !== undefined) {
      const errs = await ipc.setShortcuts(next.shortcutCapture, next.shortcutToggle);
      if (errs.length) {
        get().pushToast({ kind: 'error', message: `快捷键注册失败：${errs.join('；')}`, ttl: 5000 });
      }
    }
  },

  async resetAllSettings() {
    const next = await ipc.resetSettings();
    set({ settings: next });
    mirrorBoot(next);
    applyThemeClass(next.theme);
    const info = await ipc.applyGlass();
    set({ glass: info });
    applyRadius(info);
    get().pushToast({ kind: 'info', message: '设置已重置', ttl: 2500 });
  },

  // ─────────────────────────────── Toast ───────────────────────────────

  pushToast(t) {
    const id = Math.random().toString(36).slice(2, 10);
    set({ toasts: [...get().toasts, { ...t, id }] });
    if (t.ttl > 0) {
      setTimeout(() => get().dismissToast(id), t.ttl);
    }
    return id;
  },

  dismissToast(id) {
    set({ toasts: get().toasts.filter((x) => x.id !== id) });
  },
}));

/** 把主题写到 <html> 上。用 class 而不是 media query，因为主题是用户显式选定的。 */
export function applyThemeClass(theme: AppSettings['theme']) {
  const dark = theme === 'dark';
  document.documentElement.classList.toggle('dark', dark);
  document.documentElement.classList.toggle('light', !dark);
}
