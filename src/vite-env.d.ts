/// <reference types="vite/client" />

// 让 TypeScript 认识 `import './styles.css'` 这类副作用式样式导入。
// Vite 在构建时会真正处理它们，但 TS 本身不知道 .css 是模块。
declare module '*.css';
