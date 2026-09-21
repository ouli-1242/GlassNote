/**
 * 快速捕获浮窗。
 *
 * 独立入口（capture.html）：它比主面板小得多，不需要标题栏、列表、设置，
 * 共用一个 bundle 只会让这个高频小窗口加载多余的代码。
 *
 * 交互：输入后回车即保存并关闭；Esc 取消。
 */

import { useEffect, useRef, useState } from 'react';
import ReactDOM from 'react-dom/client';

import { useDragRegion } from './hooks/useDragRegion';
import * as ipc from './lib/ipc';
import { applyBoot, getBoot } from './lib/boot';
import { parseQuick } from './lib/quickparse';
import type { GlassInfo } from './types';
import './styles.css';

applyBoot(getBoot());

function CaptureApp() {
  const [value, setValue] = useState('');
  const [hints, setHints] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [glass, setGlass] = useState<GlassInfo | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useDragRegion(true);

  useEffect(() => {
    inputRef.current?.focus();
    // 拿属于本窗口的玻璃信息：系统玻璃可用时不需要 CSS 模糊兜底，
    // 不可用时才要打开 backdrop-filter（见 styles.css 的 data-effect 规则）
    void ipc
      .applyGlass()
      .then(setGlass)
      .catch(() => setGlass(null));
  }, []);

  const close = () => void ipc.closeCaptureWindow();

  const submit = async () => {
    const parsed = parseQuick(value);
    if (!parsed.input.title || busy) return;
    setBusy(true);
    try {
      await ipc.createTask({ ...parsed.input, title: parsed.input.title });
      close();
    } catch {
      setBusy(false);
    }
  };

  return (
    <div className="glass-root h-full w-full" data-effect={glass?.effect ?? 'css'}>
      <div className="glass-layer glass-tint" />
      <div className="glass-layer glass-edge" />

      <div className="relative z-10 flex h-full flex-col justify-center gap-1 px-3">
        <input
          ref={inputRef}
          data-no-drag
          value={value}
          placeholder="快速添加任务… 回车保存，Esc 取消"
          onChange={(e) => {
            setValue(e.target.value);
            setHints(parseQuick(e.target.value).hints);
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              void submit();
            } else if (e.key === 'Escape') {
              e.preventDefault();
              close();
            }
          }}
          onBlur={() => {
            // 失焦即关闭，符合"随手记一条"的心智模型
            if (!value.trim()) close();
          }}
          className="w-full border-none bg-transparent text-[14px] outline-none placeholder:text-[hsl(var(--muted-foreground)/0.75)]"
        />
        {hints.length > 0 ? (
          <div className="flex gap-2 text-[10px] text-[hsl(var(--muted-foreground))]">
            {hints.map((h) => (
              <span key={h}>{h}</span>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById('root')!).render(<CaptureApp />);
