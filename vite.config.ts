import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri 会以固定端口加载前端，端口被占用时必须直接失败而不是静默换端口，
// 否则 devUrl 与实际端口不一致，窗口会白屏。
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // 必须显式绑 IPv4。
    // Vite 默认的 host 是 "localhost"，在 Node 17+ 上会解析成 ::1 并**只**监听 IPv6 回环，
    // 此时 127.0.0.1:1420 完全不可达。而 WebView2 里的 localhost 优先走 IPv4，
    // 于是导航直接失败、窗口停在 about:blank —— 表现是一片空白，且没有任何报错。
    // 这里和 tauri.conf.json 的 devUrl 都写死 127.0.0.1，两边不留解析歧义。
    host: "127.0.0.1",
    // src-tauri 由 cargo 自己监听，Vite 再监听一遍会导致 Rust 侧反复重启
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    // WebView2 在 Win10/11 上等价于较新的 Chromium，可以直接用现代语法，避免多余降级代码
    target: "chrome105",
    minify: "esbuild",
    sourcemap: false,
    cssMinify: true,
    rollupOptions: {
      // 两个独立窗口：主面板 + 快速捕获浮窗，各自一个入口，互不加载对方的代码
      input: {
        main: "index.html",
        capture: "capture.html",
      },
      output: {
        manualChunks: undefined,
      },
    },
  },
});
