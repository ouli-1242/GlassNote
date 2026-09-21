import {
  Database,
  Download,
  FolderOpen,
  HardDriveDownload,
  Lock,
  RefreshCw,
  Trash2,
  Upload,
} from 'lucide-react';
// 起别名：组件内已有同名的 open 变量（表示设置面板是否展开），直接导入会被遮蔽
import { open as pickFile, save as pickSavePath } from '@tauri-apps/plugin-dialog';
import { useEffect, useState } from 'react';

import * as ipc from '../lib/ipc';
import { cn, formatBytes } from '../lib/utils';
import { useStore } from '../store/useStore';
import type { RepeatType, ThemeMode } from '../types';
import { Button } from './ui/Button';
import { Segmented, Select, Slider, Switch } from './ui/Controls';
import { Input, Row, Section } from './ui/Field';
import { ConfirmDialog, Dialog } from './ui/Overlay';

const EFFECT_LABEL: Record<string, string> = {
  mica: 'Mica（Windows 11 原生）',
  tabbed: 'Mica Tabbed',
  acrylic: 'Acrylic（Windows 10 降级）',
  blur: 'Blur（兼容模式）',
  css: 'CSS 半透明（系统玻璃不可用）',
};

export function SettingsPanel() {
  const open = useStore((s) => s.settingsOpen);
  const setOpen = useStore((s) => s.setSettingsOpen);
  const settings = useStore((s) => s.settings);
  const update = useStore((s) => s.updateSettings);
  const resetAll = useStore((s) => s.resetAllSettings);
  const glass = useStore((s) => s.glass);
  const pushToast = useStore((s) => s.pushToast);

  const [dir, setDir] = useState('');
  const [backups, setBackups] = useState<ipc.BackupInfo[]>([]);
  const [confirm, setConfirm] = useState<null | 'reset' | 'clear'>(null);
  const [pin, setPin] = useState('');

  useEffect(() => {
    if (!open) return;
    void ipc.dataDir().then(setDir).catch(() => setDir('未知'));
    void ipc.listBackups().then(setBackups).catch(() => setBackups([]));
  }, [open]);

  /**
   * 导出。
   *
   * 路径选择走前端 dialog 插件、文件写入走 Rust 命令：
   * 系统文件对话框是异步回调式的，放在 Rust 命令里会变成同步阻塞，
   * 而前端本来就有 dialog 权限，分工更自然。
   */
  const doExport = async (format: 'json' | 'markdown' | 'csv') => {
    try {
      const ext = format === 'markdown' ? 'md' : format;
      const stamp = new Date().toISOString().slice(0, 10);
      const target = await pickSavePath({
        defaultPath: `glassnote-${stamp}.${ext}`,
        filters: [{ name: format.toUpperCase(), extensions: [ext] }],
      });
      if (!target) return; // 用户取消
      const r = await ipc.exportData(format, target);
      pushToast({ kind: 'success', message: `已导出 ${formatBytes(r.bytes)} 到 ${r.path}`, ttl: 4000 });
    } catch (e) {
      pushToast({ kind: 'error', message: `导出失败：${e}`, ttl: 4000 });
    }
  };

  const doImport = async () => {
    try {
      const picked = await pickFile({
        multiple: false,
        directory: false,
        filters: [{ name: 'GlassNote JSON', extensions: ['json'] }],
      });
      if (!picked || Array.isArray(picked)) return;
      const r = await ipc.importData(picked, 'merge');
      await useStore.getState().reloadTasks();
      await useStore.getState().reloadMemos();
      pushToast({
        kind: 'success',
        message: `已导入 ${r.tasks} 条任务、${r.memos} 条备忘录${r.skipped ? `，跳过重复 ${r.skipped} 条` : ''}`,
        ttl: 4500,
      });
    } catch (e) {
      pushToast({ kind: 'error', message: `导入失败：${e}`, ttl: 4000 });
    }
  };

  return (
    <>
      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title="设置"
        width="min(94%, 400px)"
        footer={
          <Button variant="ghost" size="sm" onClick={() => setOpen(false)}>
            完成
          </Button>
        }
      >
        {/* ── 外观 ── */}
        <Section title="外观">
          <Row label="主题">
            <Segmented
              value={settings.theme}
              onChange={(v) => void update({ theme: v as ThemeMode })}
              options={[
                { value: 'light' as ThemeMode, label: '白天' },
                { value: 'dark' as ThemeMode, label: '黑夜' },
              ]}
            />
          </Row>

          <Row label="面板透明度" hint={`${Math.round(settings.opacity * 100)}%`}>
            <Slider
              min={20}
              max={100}
              value={Math.round(settings.opacity * 100)}
              onChange={(v) => void update({ opacity: v / 100 })}
            />
          </Row>

          <Row label="字体大小" hint={`${Math.round(settings.fontScale * 100)}%`}>
            <Slider
              min={85}
              max={130}
              value={Math.round(settings.fontScale * 100)}
              onChange={(v) => void update({ fontScale: v / 100 })}
            />
          </Row>
        </Section>

        {/* ── 窗口与托盘 ── */}
        <Section title="窗口与托盘">
          <Row label="窗口置顶">
            <Switch
              checked={settings.alwaysOnTop}
              onChange={(v) => void update({ alwaysOnTop: v })}
              label="置顶"
            />
          </Row>
          <Row label="显示托盘图标">
            <Switch
              checked={settings.showTrayIcon}
              onChange={(v) => void update({ showTrayIcon: v })}
              label="托盘图标"
            />
          </Row>
          <Row label="关闭窗口时最小化到托盘" hint="关闭它则关闭窗口即退出应用">
            <Switch
              checked={settings.closeToTray}
              onChange={(v) => void update({ closeToTray: v })}
              label="关闭到托盘"
            />
          </Row>
          <Row
            label="隐藏时释放内存"
            hint="开启后隐藏会销毁窗口，内存降到最低；代价是再次显示要多等约 200ms"
          >
            <Switch
              checked={settings.releaseMemoryWhenHidden}
              onChange={(v) => void update({ releaseMemoryWhenHidden: v })}
              label="释放内存"
            />
          </Row>
        </Section>

        {/* ── 启动 ── */}
        <Section title="启动">
          <Row label="开机自动启动">
            <Switch
              checked={settings.autostart}
              onChange={(v) => void update({ autostart: v })}
              label="开机自启"
            />
          </Row>
          <Row label="自启延迟" hint="避开开机资源争抢；启动后静默驻留托盘，不弹窗不抢焦点">
            <Select
              value={String(settings.autostartDelaySec)}
              onChange={(v) => void update({ autostartDelaySec: Number(v) })}
              options={[
                { value: '0', label: '不延迟' },
                { value: '3', label: '3 秒' },
                { value: '5', label: '5 秒（默认）' },
                { value: '10', label: '10 秒' },
                { value: '30', label: '30 秒' },
              ]}
            />
          </Row>
        </Section>

        {/* ── 提醒 ── */}
        <Section title="提醒">
          <Row label="系统通知">
            <Switch
              checked={settings.notificationsEnabled}
              onChange={(v) => void update({ notificationsEnabled: v })}
              label="通知"
            />
          </Row>
          <Row label="免打扰开始">
            <Input
              type="time"
              value={settings.dndStart}
              onChange={(e) => void update({ dndStart: e.target.value })}
              className="h-7 w-[92px] text-[12px]"
            />
          </Row>
          <Row label="免打扰结束">
            <Input
              type="time"
              value={settings.dndEnd}
              onChange={(e) => void update({ dndEnd: e.target.value })}
              className="h-7 w-[92px] text-[12px]"
            />
          </Row>
          <Row label="默认重复规则" hint="新建任务时的预设值">
            <Select
              value={settings.defaultRepeatType}
              onChange={(v) => void update({ defaultRepeatType: v as RepeatType })}
              options={[
                { value: 'once' as RepeatType, label: '不重复' },
                { value: 'daily' as RepeatType, label: '每天' },
                { value: 'weekly' as RepeatType, label: '每周' },
                { value: 'monthly' as RepeatType, label: '每月' },
                { value: 'weekday' as RepeatType, label: '每个工作日' },
              ]}
            />
          </Row>
        </Section>

        {/* ── 快捷键 ── */}
        <Section title="快捷键">
          <Row label="快速添加任务" hint="系统级全局快捷键">
            <ShortcutInput
              value={settings.shortcutCapture}
              onChange={(v) => void update({ shortcutCapture: v })}
            />
          </Row>
          <Row label="显示 / 隐藏窗口" hint="系统级全局快捷键">
            <ShortcutInput
              value={settings.shortcutToggle}
              onChange={(v) => void update({ shortcutToggle: v })}
            />
          </Row>
          <p className="pt-1.5 text-[10px] leading-snug text-[hsl(var(--muted-foreground))]">
            窗口内：Enter 保存 · Esc 取消 · Ctrl+F 搜索 · Ctrl+, 打开设置 · Ctrl+N 新建任务
          </p>
        </Section>

        {/* ── 数据 ── */}
        <Section title="数据">
          <Row label="数据目录" hint={dir}>
            <Button variant="surface" size="sm" onClick={() => void ipc.openDataDir()}>
              <FolderOpen size={12} />
              打开
            </Button>
          </Row>

          <Row label="自动备份" hint={`保留最近 ${settings.backupKeep} 份`}>
            <Switch
              checked={settings.autoBackupEnabled}
              onChange={(v) => void update({ autoBackupEnabled: v })}
              label="自动备份"
            />
          </Row>

          <Row label="回收站保留" hint="天后自动清理">
            <Select
              value={String(settings.trashKeepDays)}
              onChange={(v) => void update({ trashKeepDays: Number(v) })}
              options={[
                { value: '7', label: '7 天' },
                { value: '30', label: '30 天（默认）' },
                { value: '90', label: '90 天' },
                { value: '0', label: '永不清理' },
              ]}
            />
          </Row>

          <div className="flex flex-wrap gap-1.5 py-2">
            <Button variant="surface" size="sm" onClick={() => void ipc.backupNow().then(() => ipc.listBackups().then(setBackups))}>
              <HardDriveDownload size={12} />
              立即备份
            </Button>
            <Button variant="surface" size="sm" onClick={() => void doExport('json')}>
              <Download size={12} />
              导出 JSON
            </Button>
            <Button variant="surface" size="sm" onClick={() => void doExport('markdown')}>
              <Download size={12} />
              导出 Markdown
            </Button>
            <Button variant="surface" size="sm" onClick={() => void doExport('csv')}>
              <Download size={12} />
              导出 CSV
            </Button>
            <Button variant="surface" size="sm" onClick={() => void doImport()}>
              <Upload size={12} />
              导入
            </Button>
            <Button variant="surface" size="sm" onClick={() => void ipc.runMaintenance().then(() => useStore.getState().reloadTasks())}>
              <RefreshCw size={12} />
              立即清理
            </Button>
          </div>

          {backups.length > 0 ? (
            <div className="max-h-24 space-y-0.5 overflow-y-auto scroll-area py-1">
              {backups.slice(0, 7).map((b) => (
                <div key={b.path} className="flex items-center justify-between gap-2 text-[11px]">
                  <span className="truncate text-[hsl(var(--muted-foreground))]" title={b.path}>
                    {b.name}
                  </span>
                  <span className="shrink-0 tabular-nums text-[hsl(var(--muted-foreground))]">
                    {formatBytes(b.sizeBytes)}
                  </span>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-5 shrink-0 px-1.5 text-[10px]"
                    onClick={() => void ipc.restoreBackup(b.path).then(() => ipc.relaunchApp())}
                  >
                    恢复
                  </Button>
                </div>
              ))}
            </div>
          ) : null}
        </Section>

        {/* ── 隐私 ── */}
        <Section title="隐私锁">
          <Row label="启用 PIN 锁" hint="启用后打开窗口需要输入 PIN">
            <Switch
              checked={settings.lockEnabled}
              onChange={(v) => {
                if (!v) {
                  void ipc.setLockPin(null);
                  void update({ lockEnabled: false });
                } else {
                  void update({ lockEnabled: true });
                }
              }}
              label="PIN 锁"
            />
          </Row>
          {settings.lockEnabled ? (
            <Row label="设置 PIN">
              <div className="flex items-center gap-1.5">
                <Input
                  type="password"
                  value={pin}
                  placeholder="4-8 位数字"
                  inputMode="numeric"
                  onChange={(e) => setPin(e.target.value.replace(/\D/g, '').slice(0, 8))}
                  className="h-7 w-[100px] text-[12px]"
                />
                <Button
                  variant="surface"
                  size="sm"
                  disabled={pin.length < 4}
                  onClick={() =>
                    void ipc.setLockPin(pin).then(() => {
                      setPin('');
                      pushToast({ kind: 'success', message: 'PIN 已更新', ttl: 2500 });
                    })
                  }
                >
                  <Lock size={12} />
                  保存
                </Button>
              </div>
            </Row>
          ) : null}
        </Section>

        {/* ── 关于 ── */}
        <Section title="关于">
          <Row label="玻璃效果" hint={glass ? EFFECT_LABEL[glass.effect] ?? glass.effect : '检测中…'}>
            <span
              className={cn(
                'rounded-full px-2 py-[2px] text-[10px]',
                glass?.isWindows11
                  ? 'bg-[hsl(var(--accent)/0.18)] text-[hsl(var(--accent))]'
                  : 'bg-[hsl(var(--muted))] text-[hsl(var(--muted-foreground))]',
              )}
            >
              {glass?.isWindows11 ? 'Windows 11' : '兼容模式'}
            </span>
          </Row>
          <Row label="版本">
            <span className="text-[12px] text-[hsl(var(--muted-foreground))]">0.1.0</span>
          </Row>
        </Section>

        <div className="flex flex-wrap gap-1.5 pb-1">
          <Button variant="ghost" size="sm" onClick={() => setConfirm('reset')}>
            <RefreshCw size={12} />
            重置设置
          </Button>
          <Button variant="danger" size="sm" onClick={() => setConfirm('clear')}>
            <Trash2 size={12} />
            清除全部数据
          </Button>
        </div>

        <p className="pt-2 text-[10px] leading-relaxed text-[hsl(var(--muted-foreground))]">
          <Database size={9} className="mr-0.5 inline" />
          所有数据仅保存在本机 SQLite 数据库，不联网、不上传。
        </p>
      </Dialog>

      <ConfirmDialog
        open={confirm === 'reset'}
        title="重置设置"
        message="所有设置项将恢复默认值。任务、备忘录和备份不受影响。"
        confirmLabel="重置"
        onConfirm={() => {
          void resetAll();
          setConfirm(null);
        }}
        onCancel={() => setConfirm(null)}
      />

      <ConfirmDialog
        open={confirm === 'clear'}
        title="清除全部数据"
        message="将永久删除所有任务、备忘录、历史记录与设置，且无法恢复。建议先做一次备份。"
        confirmLabel="确认清除"
        danger
        onConfirm={() => {
          void ipc.clearAllData(true).then(() => ipc.relaunchApp());
        }}
        onCancel={() => setConfirm(null)}
      />
    </>
  );
}

/** 快捷键录制框：按下组合键即记录，Esc 取消录制。 */
function ShortcutInput({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [recording, setRecording] = useState(false);
  const [draft, setDraft] = useState(value);

  useEffect(() => setDraft(value), [value]);

  return (
    <button
      type="button"
      data-no-drag
      onClick={() => setRecording(true)}
      onBlur={() => setRecording(false)}
      onKeyDown={(e) => {
        if (!recording) return;
        e.preventDefault();
        e.stopPropagation();
        if (e.key === 'Escape') {
          setRecording(false);
          setDraft(value);
          return;
        }
        // 只按修饰键不算一个完整组合，等主键按下再提交
        if (['Control', 'Shift', 'Alt', 'Meta'].includes(e.key)) return;

        const parts: string[] = [];
        if (e.ctrlKey) parts.push('CommandOrControl');
        if (e.shiftKey) parts.push('Shift');
        if (e.altKey) parts.push('Alt');
        const key = e.key.length === 1 ? e.key.toUpperCase() : e.key;
        parts.push(key);
        const combo = parts.join('+');
        setDraft(combo);
        setRecording(false);
        onChange(combo);
      }}
      className={cn(
        'field h-7 min-w-[132px] rounded-[9px] px-2 text-[12px] tabular-nums',
        recording && 'border-[hsl(var(--accent))] text-[hsl(var(--accent))]',
      )}
    >
      {recording ? '按下组合键…' : draft}
    </button>
  );
}
