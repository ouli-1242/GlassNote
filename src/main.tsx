import ReactDOM from 'react-dom/client';

import App from './App';
import { applyBoot, getBoot } from './lib/boot';
import './styles.css';

// 先把引导数据落到 DOM，再挂载 React —— 顺序不能反，
// 否则会先按默认样式画一帧，再被改回来，视觉上就是"闪一下"。
applyBoot(getBoot());

// 刻意不使用 StrictMode：它会在开发模式下重复执行 effect，
// 而这个应用的首屏初始化有真实副作用（应用玻璃效果、注册关闭监听、拉取列表），
// 重复执行会产生两倍的 IPC 与两次窗口效果应用，掩盖真正的性能问题。
ReactDOM.createRoot(document.getElementById('root')!).render(<App />);
