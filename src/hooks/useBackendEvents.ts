/**
 * 后端事件订阅。
 *
 * Rust 侧的调度器是唯一的定时源（单线程、分钟级、按需唤醒），
 * 前端**不跑任何轮询**。到点提醒、数据变化都由后端主动推事件，
 * 前端只负责响应。这是"空闲 CPU ≈ 0"的前提：没有定时器就没有周期性唤醒。
 */

import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';

import * as ipc from '../lib/ipc';
import { useStore } from '../store/useStore';

interface ReminderPayload {
  taskId: number;
  title: string;
  note: string | null;
  dueAt: number | null;
}

export function useBackendEvents() {
  const pushToast = useStore((s) => s.pushToast);
  const reloadTasks = useStore((s) => s.reloadTasks);

  useEffect(() => {
    const disposers: Array<() => void> = [];
    let cancelled = false;

    const track = (p: Promise<() => void>) => {
      void p.then((un) => {
        if (cancelled) un();
        else disposers.push(un);
      });
    };

    // 到点提醒：后端已经发过系统通知，这里补一条应用内提示
    track(
      listen<ReminderPayload>('glassnote://reminder', (e) => {
        const p = e.payload;
        pushToast({
          kind: 'info',
          message: p.title,
          action: { label: '稍后 5 分钟', run: () => void ipc.snoozeTask(p.taskId, 5) },
          ttl: 8000,
        });
      }),
    );

    // 调度器推进了重复任务 / 清理了回收站 → 列表需要刷新
    track(
      listen('glassnote://tasks-changed', () => {
        void reloadTasks();
      }),
    );

    // 托盘菜单里的"设置…"会推这个事件，让前端打开设置面板
    track(listen('glassnote://open-settings', () => useStore.getState().setSettingsOpen(true)));

    // 注意：关闭按钮的行为**不在前端处理**。
    // Rust 侧监听 WindowEvent::CloseRequested 并决定"隐藏到托盘"还是"退出"，
    // 这样即使渲染进程卡死或崩溃，关闭按钮依然可靠。
    // 前端再插一手会变成两处逻辑互相打架。

    return () => {
      cancelled = true;
      disposers.forEach((d) => d());
    };
  }, [pushToast, reloadTasks]);
}
