/**
 * 极简 Markdown 渲染。
 *
 * 只支持便签里真正会用到的语法：标题、粗体、斜体、行内代码、代码块、
 * 无序/有序列表、引用、分隔线、链接。
 *
 * 为什么手写而不是引 marked / markdown-it：
 *   那两个库加起来 30KB+，而这里需要的子集不到 60 行。
 *   便签不是文档编辑器，把完整 Markdown 规范搬进来是过度设计。
 *
 * 安全性：**先整体转义 HTML，再做替换**。顺序不能反 ——
 * 反过来的话，用户输入里的 `<script>` 会先被当成标签保留下来。
 */

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

/** 行内语法。在块级处理之后、逐行应用。 */
function inline(s: string): string {
  return s
    .replace(/`([^`]+)`/g, '<code class="rounded bg-[hsl(var(--muted)/0.8)] px-1 py-[1px] font-mono text-[0.92em]">$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/(^|[^*])\*([^*]+)\*/g, '$1<em>$2</em>')
    .replace(/~~([^~]+)~~/g, '<del>$1</del>')
    .replace(
      // 只放行 http/https，阻断 javascript: 之类的协议
      /\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g,
      '<a href="$2" target="_blank" rel="noreferrer noopener" class="text-[hsl(var(--accent))] underline underline-offset-2">$1</a>',
    );
}

export function renderMarkdown(src: string): string {
  const lines = escapeHtml(src).split('\n');
  const out: string[] = [];
  let inCode = false;
  let listType: 'ul' | 'ol' | null = null;
  let inQuote = false;

  const closeList = () => {
    if (listType) {
      out.push(`</${listType}>`);
      listType = null;
    }
  };
  const closeQuote = () => {
    if (inQuote) {
      out.push('</blockquote>');
      inQuote = false;
    }
  };

  for (const raw of lines) {
    const line = raw;

    // 代码块围栏：内部一律不做行内替换，避免把代码里的 * 当强调
    if (/^\s*```/.test(line)) {
      closeList();
      closeQuote();
      out.push(inCode ? '</code></pre>' : '<pre class="my-1.5 overflow-x-auto rounded-[8px] bg-[hsl(var(--muted)/0.7)] p-2"><code class="font-mono text-[11px] leading-relaxed">');
      inCode = !inCode;
      continue;
    }
    if (inCode) {
      out.push(line);
      continue;
    }

    if (!line.trim()) {
      closeList();
      closeQuote();
      continue;
    }

    const h = /^(#{1,4})\s+(.*)$/.exec(line);
    if (h) {
      closeList();
      closeQuote();
      const size = ['text-[15px]', 'text-[14px]', 'text-[13px]', 'text-[12px]'][h[1].length - 1];
      out.push(`<div class="mt-1.5 mb-0.5 font-semibold ${size}">${inline(h[2])}</div>`);
      continue;
    }

    if (/^(-{3,}|\*{3,})$/.test(line.trim())) {
      closeList();
      closeQuote();
      out.push('<hr class="my-2 border-[hsl(var(--border)/0.6)]" />');
      continue;
    }

    const ul = /^\s*[-*+]\s+(.*)$/.exec(line);
    const ol = /^\s*\d+\.\s+(.*)$/.exec(line);
    if (ul || ol) {
      closeQuote();
      const want: 'ul' | 'ol' = ul ? 'ul' : 'ol';
      if (listType !== want) {
        closeList();
        out.push(`<${want} class="my-1 ml-4 ${want === 'ul' ? 'list-disc' : 'list-decimal'} space-y-0.5">`);
        listType = want;
      }
      out.push(`<li>${inline((ul ?? ol)![1])}</li>`);
      continue;
    }

    const q = /^&gt;\s?(.*)$/.exec(line);
    if (q) {
      closeList();
      if (!inQuote) {
        out.push('<blockquote class="my-1 border-l-2 border-[hsl(var(--accent)/0.6)] pl-2 text-[hsl(var(--muted-foreground))]">');
        inQuote = true;
      }
      out.push(`<div>${inline(q[1])}</div>`);
      continue;
    }

    closeList();
    closeQuote();
    out.push(`<div>${inline(line)}</div>`);
  }

  closeList();
  closeQuote();
  if (inCode) out.push('</code></pre>');
  return out.join('\n');
}

/** 纯文本摘要，用于折叠态显示一行 */
export function plainSummary(src: string, max = 80): string {
  const text = src
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/[#>*_~`\-]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
  return text.length > max ? `${text.slice(0, max)}…` : text;
}
