/**
 * IPC 契约层。
 *
 * 所有 `invoke` 都收敛在这里，组件不直接调用 —— 这样命令名、参数形状、
 * 返回值类型只有一处定义，重命名命令时编译期就能发现所有受影响的位置。
 * 命令名必须与 src-tauri/src/commands/ 里 `#[tauri::command]` 的函数名一致。
 */

import { invoke } from '@tauri-apps/api/core';

import type {
  AppSettings,
  BootPayload,
  CompletedLog,
  GlassInfo,
  Memo,
  MemoInput,
  Task,
  TaskInput,
  TaskQuery,
} from '../types';

// ─────────────────────────────── 任务 ───────────────────────────────

export const listTasks = (query: TaskQuery = {}) => invoke<Task[]>('list_tasks', { query });

export const createTask = (input: TaskInput) => invoke<Task>('create_task', { input });

export const updateTask = (input: TaskInput) => invoke<Task>('update_task', { input });

/** 完成结果里带上"下一次出现时间"，前端据此决定是否提示"明天再来" */
export interface CompleteResult {
  task: Task | null;
  logId: number;
  /** 重复任务推进后的下一次出现时间；一次性任务为 null */
  nextAt: number | null;
  /** 是否为重复任务（决定撤销提示的文案） */
  recurring: boolean;
}

export const completeTask = (id: number) => invoke<CompleteResult>('complete_task', { id });

export const uncompleteTask = (logId: number) => invoke<Task | null>('uncomplete_task', { logId });

export const skipTaskOccurrence = (id: number) => invoke<Task | null>('skip_task_occurrence', { id });

export const deleteTask = (id: number) => invoke<void>('delete_task', { id });

export const restoreTask = (id: number) => invoke<void>('restore_task', { id });

export const purgeTask = (id: number) => invoke<void>('purge_task', { id });

export const emptyTrash = () => invoke<number>('empty_trash');

export const snoozeTask = (id: number, minutes: number) => invoke<Task>('snooze_task', { id, minutes });

export const reorderTasks = (ids: number[]) => invoke<void>('reorder_tasks', { ids });

export const setTaskPinned = (id: number, pinned: boolean) =>
  invoke<void>('set_task_pinned', { id, pinned });

export const listCompletedLogs = (limit = 200) =>
  invoke<CompletedLog[]>('list_completed_logs', { limit });

/** 编辑重复任务时预览下一次出现时间，避免前端重复实现一遍规则语义 */
export const previewNextOccurrence = (input: TaskInput) =>
  invoke<{ nextAt: number | null; label: string }>('preview_next_occurrence', { input });

// ─────────────────────────────── 备忘录 ───────────────────────────────

export const listMemos = (includeTrashed = false) => invoke<Memo[]>('list_memos', { includeTrashed });

export const saveMemo = (input: MemoInput) => invoke<Memo>('save_memo', { input });

export const setMemoCollapsed = (id: number, collapsed: boolean) =>
  invoke<void>('set_memo_collapsed', { id, collapsed });

export const deleteMemo = (id: number) => invoke<void>('delete_memo', { id });

export const restoreMemo = (id: number) => invoke<void>('restore_memo', { id });

export const purgeMemo = (id: number) => invoke<void>('purge_memo', { id });

export const setMemoPinned = (id: number, pinned: boolean) =>
  invoke<void>('set_memo_pinned', { id, pinned });

export const reorderMemos = (ids: number[]) => invoke<void>('reorder_memos', { ids });

// ─────────────────────────────── 设置 ───────────────────────────────

export const getSettings = () => invoke<AppSettings>('get_settings');

export const setSettings = (patch: Partial<AppSettings>) =>
  invoke<AppSettings>('set_settings', { patch });

export const getBootPayload = () => invoke<BootPayload>('get_boot_payload');

export const resetSettings = () => invoke<AppSettings>('reset_settings');

// ─────────────────────────────── 窗口与玻璃 ───────────────────────────────

export const detectGlass = () => invoke<GlassInfo>('detect_glass');

export const applyGlass = () => invoke<GlassInfo>('apply_glass');

export const setOpacity = (value: number) => invoke<void>('set_opacity', { value });

export const setAlwaysOnTop = (on: boolean) => invoke<void>('set_always_on_top', { on });

export const hideMainWindow = () => invoke<void>('hide_main_window');

export const showMainWindow = () => invoke<void>('show_main_window');

export const toggleMainWindow = () => invoke<void>('toggle_main_window');

/** 折叠主面板：只保留顶栏高度，窗口本身也随之收缩 */
export const setWindowCollapsed = (collapsed: boolean) =>
  invoke<void>('set_window_collapsed', { collapsed });

export const openCaptureWindow = () => invoke<void>('open_capture_window');

export const closeCaptureWindow = () => invoke<void>('close_capture_window');

/** 拖动结束后做一次边缘吸附与多显示器位置校正 */
export const snapAndClamp = () => invoke<void>('snap_and_clamp');

// ─────────────────────────────── 快捷键 / 隐私锁 ───────────────────────────────

export const setShortcuts = (capture: string, toggle: string) =>
  invoke<string[]>('set_shortcuts', { capture, toggle });

export const setLockPin = (pin: string | null) => invoke<void>('set_lock_pin', { pin });

export const verifyLockPin = (pin: string) => invoke<boolean>('verify_lock_pin', { pin });

// ─────────────────────────────── 数据 ───────────────────────────────

export interface BackupInfo {
  path: string;
  name: string;
  sizeBytes: number;
  modifiedAt: number;
}

export interface ImportSummary {
  tasks: number;
  memos: number;
  settings: number;
  skipped: number;
}

export const dataDir = () => invoke<string>('data_dir');

export const openDataDir = () => invoke<void>('open_data_dir');

export const backupNow = () => invoke<BackupInfo>('backup_now');

export const listBackups = () => invoke<BackupInfo[]>('list_backups');

export const restoreBackup = (path: string) => invoke<void>('restore_backup', { path });

export const exportData = (format: 'json' | 'markdown' | 'csv', path: string) =>
  invoke<{ path: string; bytes: number }>('export_data', { format, path });

export const importData = (path: string, mode: 'merge' | 'replace') =>
  invoke<ImportSummary>('import_data', { path, mode });

export const clearAllData = (includeSettings: boolean) =>
  invoke<void>('clear_all_data', { includeSettings });

export const runMaintenance = () =>
  invoke<{ trashed: number; purgedTasks: number; purgedMemos: number }>('run_maintenance');

/**
 * 刷新托盘徽标与"今日待办 N 项"。
 *
 * 只在任务发生增删改后调用，不挂在列表刷新上 —— 切换 Tab、输入搜索词
 * 都会触发列表刷新，但那些操作不改变待办数量，没必要重建一次托盘菜单。
 */
export const refreshTray = () => invoke<void>('refresh_tray');

export const relaunchApp = () => invoke<void>('relaunch_app');

export const setAutostart = (enabled: boolean) => invoke<boolean>('set_autostart', { enabled });

export const isAutostartEnabled = () => invoke<boolean>('is_autostart_enabled');
