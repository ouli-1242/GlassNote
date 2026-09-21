/** @type {import('tailwindcss').Config} */
export default {
  darkMode: ["class"],
  content: ["./index.html", "./capture.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // 全部走 CSS 变量，深浅主题与强调色切换只改变量，不重编译
        border: "hsl(var(--border) / <alpha-value>)",
        ring: "hsl(var(--ring) / <alpha-value>)",
        background: "hsl(var(--background) / <alpha-value>)",
        foreground: "hsl(var(--foreground) / <alpha-value>)",
        muted: {
          DEFAULT: "hsl(var(--muted) / <alpha-value>)",
          foreground: "hsl(var(--muted-foreground) / <alpha-value>)",
        },
        accent: {
          DEFAULT: "hsl(var(--accent) / <alpha-value>)",
          foreground: "hsl(var(--accent-foreground) / <alpha-value>)",
        },
        surface: {
          DEFAULT: "hsl(var(--surface) / <alpha-value>)",
          hover: "hsl(var(--surface-hover) / <alpha-value>)",
        },
        priority: {
          low: "hsl(var(--priority-low) / <alpha-value>)",
          normal: "hsl(var(--priority-normal) / <alpha-value>)",
          high: "hsl(var(--priority-high) / <alpha-value>)",
          urgent: "hsl(var(--priority-urgent) / <alpha-value>)",
        },
      },
      fontFamily: {
        sans: [
          "system-ui",
          "-apple-system",
          "Segoe UI Variable Text",
          "Segoe UI",
          "Microsoft YaHei UI",
          "sans-serif",
        ],
        mono: ["Cascadia Mono", "Consolas", "monospace"],
      },
      // 刻意不在这里声明 keyframes / animation。
      // Tailwind 只在对应的 `animate-*` 工具类**被用到**时才产出 @keyframes，
      // 而 styles.css 里的 .task-done / .check-pop 这类普通类是直接引用 keyframes 的 ——
      // 放在这里会依赖"某个组件恰好用了 animate-xxx"，漏掉一个就静默失效。
      // 所有 keyframes 统一放在 styles.css，与使用它们的类待在一起。
    },
  },
  plugins: [],
};
