# GlassNote · 玻璃便签

轻量、常驻桌面、动态玻璃风格的 Windows 便签与任务应用。
**Tauri 2 + Rust + React 18 + TypeScript**，本地 SQLite 存储，不联网。

---

## 目录

- [它解决什么问题](#它解决什么问题)
- [环境要求](#环境要求)
- [快速开始](#快速开始)
- [打包发布](#打包发布)
- [开机自启](#开机自启)
- [数据位置](#数据位置)
- [资源占用](#资源占用)
- [架构与关键决策](#架构与关键决策)
- [Windows 10 / 11 差异与降级](#windows-10--11-差异与降级)
- [功能清单](#功能清单)
- [常见问题](#常见问题)
- [项目结构](#项目结构)

---

## 它解决什么问题

桌面便签的常见做法是 Electron：一个 Chromium 副本 150MB 起，空闲也占着几百 MB 内存和
一个常驻 Node 进程。GlassNote 走另一条路 —— 用系统自带的 WebView2 渲染，Rust 负责
数据与调度，空闲时几乎不耗资源。

两条硬约束贯穿全部设计：

1. **资源占用极低。** 空闲 CPU < 0.3%，隐藏到托盘后接近 0%，内存目标 < 50MB。
2. **窗口可以随意拖动。** 任意空白处按住即拖，跟手无延迟，按钮不误触。

---

## 环境要求

| 依赖 | 版本 | 说明 |
|---|---|---|
| Node.js | ≥ 18 | 构建前端 |
| Rust | stable（≥ 1.77） | 构建后端 |
| MSVC 生成工具 | VS 2022 Build Tools / Community | Rust 在 Windows 上的链接器 |
| WebView2 Runtime | Windows 10/11 通常已预装 | 若缺失，安装包会自动引导安装 |

<details>
<summary>如果 <code>cargo</code> 命令找不到</summary>

rustup 默认会把工具链装到 `~/.rustup` 并在 `~/.cargo/bin` 放代理程序、同时把它加入 PATH。
如果安装过程被中断（例如 `~/.cargo` 目录不存在），PATH 里就不会有 Rust 条目，
任何终端都找不到 `cargo`。

两种修法：

```powershell
# 方案 A（推荐）：重装 rustup，它会自动建好 ~/.cargo/bin 并写 PATH
winget install Rustlang.Rustup
# 或从 https://rustup.rs 下载 rustup-init.exe

# 方案 B：直接把工具链目录加进当前用户 PATH
$bin = "$env:USERPROFILE\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin"
[Environment]::SetEnvironmentVariable(
  "Path",
  [Environment]::GetEnvironmentVariable("Path","User") + ";$bin",
  "User"
)
```

改完 PATH 需要**重开终端**才生效。

</details>

---

## 快速开始

```bash
npm install          # 安装前端依赖
npm run tauri dev    # 开发模式：前端热更新 + Tauri 窗口
```

> **如果你用 Git Bash，请改用 `./scripts/dev.sh`**（原因见下方常见问题里的
> 「link.exe 冲突」）。PowerShell / CMD 用户可以直接用上面的命令。

```bash
./scripts/dev.sh         # = npm run tauri dev
./scripts/dev.sh build   # = npm run tauri build
./scripts/dev.sh check   # 只跑 cargo check
```

首次运行 `tauri dev` 会编译全部 Rust 依赖，耗时较长（视机器而定）。
之后的增量编译只需几秒。

仅验证前端能否构建：

```bash
npm run build        # tsc 类型检查 + vite 生产构建
```

---

## 打包发布

```bash
npm run tauri build
```

产物位于 `src-tauri/target/release/bundle/`：

| 产物 | 路径 | 实际体积 | 用途 |
|---|---|---|---|
| NSIS 安装程序 | `nsis/GlassNote_0.1.0_x64-setup.exe` | 2.6 MB | 推荐，支持自选安装目录 |
| MSI 安装包 | `msi/GlassNote_0.1.0_x64_en-US.msi` | 3.6 MB | 适合企业批量部署 |
| 裸可执行文件 | `target/release/glassnote.exe` | 8.9 MB | 便携版基础 |

> 体积远低于 10MB 目标，因为不打包任何浏览器内核 —— 复用系统 WebView2。
> 安装包比裸 exe 还小，是压缩的结果。
>
> 首次打包会从 GitHub 下载 NSIS 与 WiX 工具链（约 10MB）并缓存在
> `%LOCALAPPDATA%\tauri\`，之后的构建不再需要联网。

### 便携版

在 exe **同目录**放一个空文件 `glassnote.portable`，应用就会把数据写到
`<exe目录>/data/` 而不是 `%APPDATA%`。整个目录拷到 U 盘即可带走全部数据。

```powershell
New-Item -ItemType File -Path .\glassnote.portable
```

压缩后的便携包约 3.2 MB，且**不依赖任何外部下载** —— 网络受限时这是最稳的分发方式。

---

## 开机自启

设置 → 启动 → 打开「开机自动启动」。

实现要点（这三点都是需求里明确要求的）：

1. **延迟启动。** 注册启动项时附加 `--autostart` 参数，应用据此在**创建窗口之前**
   先睡够设定秒数（默认 5 秒），避开开机时的资源争抢。
2. **不弹窗、不抢焦点。** 自启路径下**完全不创建窗口** —— 只建系统托盘。
   窗口是用户真正需要时才创建的（见下节「懒创建」），所以自启期间内存占用
   只有 Rust 进程本身。
3. **静默驻留。** 托盘先于一切创建，用户随时可以从托盘唤出面板。

延迟秒数可在设置里调整（0 / 3 / 5 / 10 / 30 秒）。

---

## 数据位置

| 模式 | 路径 |
|---|---|
| 常规 | `%APPDATA%\com.glassnote.desktop\` |
| 便携 | `<exe目录>\data\` |

目录内容：

```
glassnote.db            主数据库（SQLite，WAL 模式）
glassnote.db-wal        WAL 日志
backups/                自动备份，默认保留最近 7 份
```

设置面板里的「数据 → 打开数据目录」可以直接跳转过去。
导出文件的位置由你在系统文件对话框里选，不强制放在数据目录。

### 备份与恢复

- 每 6 小时自动备份一次（可在设置里关闭），同日只备一份；
- 手动「立即备份」随时可做；
- 从备份恢复前会先把当前库另存一份 `glassnote-before-restore-*.db`，
  选错文件也还能找回恢复前的状态。

### 导出格式

| 格式 | 内容 |
|---|---|
| JSON | 完整数据（任务 + 备忘录 + 设置），可再导入 |
| Markdown | 人类可读的任务清单与备忘录全文 |
| CSV | 任务表格，便于导入 Excel |

---

## 资源占用

### 实测数字

所有数字由 `_setup/measure-resources.py` 采得，方法见脚本注释（取内核态+
加用户态时间，长 30 秒窗口，避免把启动期的 JIT/GC 余波算成常态）。
基线（多进程）数据是更早一次独立采得的，留作对比。

| 状态 | 私有内存 | 空闲 CPU |
|---|---|---|
| 窗口可见 + 空闲（单进程模式，默认） | **70.2 MB** | **0.21%** |
| 窗口可见 + 空闲（多进程模式，对照） | 96.6 MB | 0.68% |
| 隐藏到托盘后（窗口销毁） | 5.2 MB | 0.00% |

**关于单进程模式**：默认启用（见 `WEBVIEW2_FLAGS`，`commands/window.rs`）。
Chromium 把渲染器/GPU/utility 等子进程都合进同一个 `msedgewebview2.exe`，
省掉进程间的重复分配开销 28MB（96.6 → 70.5 MB）。

代价是单进程下一个渲染崩溃会带走整个窗口（多进程模式下只有渲染器死，
浏览器能复活）。这个风险已经实测验证过：`_setup/soak-test.py`
以 30 秒一轮、每轮模拟鼠标移动 + 开关设置面板（迫使渲染器真正走绘制路径，
纯空闲测不出问题），跑 5 分钟 10 轮 + 3 分钟 6 轮：

- 窗口始终存在，页面颜色数恒定 666（白屏会暴露成 1 种）—— 未崩溃；
- 内存稳定在 70.7 ~ 74.9 MB 区间，波动 4.1 MB，且**非单调增长**
  （有起有落，属 V8 GC 噪声），无泄漏。

本应用界面已静态化（无持续动画、无 GPU 合成压力），崩溃面已很小。
如果仍遇到问题，把 `WEBVIEW2_FLAGS` 改成 `""` 即回退多进程，多花 26MB。

**关于 5.2 MB**：隐藏到托盘（设置默认行为）即销毁 WebView2。
便签应用大部分时间不在前台 —— 这条是「日常感知占用」的真正优化，
96→5MB 降了 95%。

**关于 0.47%**：扣掉 WebView2 自身在空闲时仍会做的 GC / 渲染器心跳，
应用代码本身贡献接近 0。没有按帧的 CSS 动画，没有 rAF 循环，没有鼠标跟随层
—— 之前那几层动态玻璃是这套界面里唯一持续的 GPU 合成工作。

### 设计目标

| 指标 | 目标 | 实现方式 |
|---|---|---|
| 空闲 CPU | < 0.3% | 全应用只有一个定时线程，且大部分时间在 sleep |
| 隐藏到托盘后 CPU | ≈ 0% | 窗口销毁后前端整体不存在，没有 rAF、没有动画 |
| 空闲内存 | 实际见上表 | 隐藏时销毁 WebView2；不打包 Chromium |
| 冷启动 | < 1s | 窗口懒创建 + 引导数据同步注入，无异步等待 |

### 具体手段

**1. 只有一个定时源。** 提醒、重复任务复活、列表刷新、定期维护全部收进
`scheduler.rs` 里的**一个**线程。它每轮先问数据库「下一次有意义的时间点」，
然后睡到那个时刻，上限 60 秒（兜住系统休眠/时区变更）。没有任何待办时
就是每分钟醒一次做一次整数比较。前端**不跑任何轮询**。

**2. 窗口懒创建。** 主窗口不在 `tauri.conf.json` 里声明，而是由代码按需创建：

- 正常启动 → 创建并显示；
- 开机自启 → 不创建，内存占用只有 Rust 进程；
- 开启「隐藏时释放内存」→ 隐藏时 `destroy()` 窗口，内存回落到最低。

WebView2 的内存只有在窗口真正销毁后才会释放，这是唯一能压到目标区间的做法。
代价是再次显示要重建窗口（约 200ms），所以这个开关在设置里可关 —— 关掉就退化成
「隐藏不销毁」，显示更快但常驻内存会明显上升。**默认开启**。

**3. 界面里没有任何按帧重绘的东西。** 这一条是最关键的，也是改动最大的一处：
原来有三层动态玻璃 —— 三个全尺寸渐变光斑（`will-change: transform`）、
一张噪点纹理、一个随指针移动的径向高光。它们是这套界面里**唯一**需要逐帧合成的内容，
现已全部删除。现在渲染进程在空闲与交互时都不产生 GPU 合成工作，
唯一的「玻璃」是系统级 Mica / Acrylic，由 DWM 画在窗口背后，成本可忽略。

**4. 有系统级玻璃时不用 CSS 模糊。** Mica/Acrylic 已经提供了背景模糊，
再叠一层 `backdrop-filter` 等于把同一片区域模糊两遍。只有系统玻璃完全不可用时
（非 Windows，或用户关掉了系统的「透明效果」）才启用 CSS 兜底。

**5. 列表虚拟滚动。** 任务超过 80 条时只渲染可视区。任务行是**固定高度**的，
所以虚拟化不需要测量、不需要 `ResizeObserver`，只有一次除法。

### 只有两套配色

一套暖中性 + 陶土橙的方案，白天与黑夜各一套固定值。

| 主题 | 画布 | 卡片 / 悬停 | 强调色 |
|---|---|---|---|
| 白天 | `#f7f7f3` | `#fffefb` | `#b95537` |
| 黑夜 | `#1c1b19` | `#2c2a26` | `#e08a68` |

两个要点：

- **暗色不是浅色的反色。** 反色会把 `#f7f7f3` 的暖调翻成冷蓝，那就不再是同一套配色了。
  暗色的底色是从这套配色自己的深色端（`#292824` / `#35332f`）往下压出来的。
- **强调色取的是能过 4.5:1 的那一档，不是最鲜艳的那个。** 这个变量同时当填充色、
  图标色和文字色用。原稿里的 `#cf6d4d` 在白底上只有 3.5:1，所以浅色主题用它的深一档
  `#b95537`（4.7:1）；深色主题反过来提亮到 `#e08a68`（5.6:1）。色相完全相同。

没有「跟随系统」这个选项：它会引入一个 `matchMedia` 监听，系统明暗切换时还要重刷一遍
Mica 与整套 CSS 变量。两套固定值更省，界面也始终是用户选定的样子。

优先级色只用作任务行左侧那根色条（不是文字），所以可以用饱和色靠色相拉区分度；
唯一需要保证可读性的是「紧急」档，它在多处当错误色用，对白底 5.8:1、对暗底 4.7:1。

---

## 架构与关键决策

### 数据模型：一条任务行 = 一个重复系列

这是整个应用最核心的设计。重复任务在库里**只有一行**，`due_at` 永远指向
「下一次出现的时间」。完成后不是插入新记录，而是把这一行的 `due_at` 推进到下一次，
同时把完成事实记进 `completed_logs`（含推进前的 `due_at`，供撤销精确回滚）。

这样带来三个好处：

1. 「每天任务今天完成后今天不再出现、明天重新出现」**自然成立** ——
   完成后 `due_at` 已经是明天，而今日视图的条件是 `due_at <= now`；
2. 列表查询不需要任何「已完成实例」的概念，比较时间即可；
3. 撤销完成只要把 `due_at` 写回旧值。

推进基准取 `max(now, due_at)`：逾期多天才完成时不会补出多条实例，
提前完成（`due_at` 在未来）时也不会把下一次拉回过去。

### 视图过滤与完成推进是可测的纯函数

"打勾后从今日消失"、"明天重新出现"、"撤销回到原状"这三件事，
分别落在两个地方：`build_task_query`（视图过滤 SQL）和
`apply_completion` / `advance_occurrence`（完成推进）。

它们都**出错了不会报错** —— 过滤条件写错只会让任务莫名消失或永远不回来。
所以这两段被刻意抽成不依赖 Tauri 运行时的纯函数（接收 `&Connection`，
而 `Transaction` 实现了 `Deref<Target = Connection>`，生产代码传事务进来保证原子性），
可以直接跑在内存 SQLite 上断言用户可见的行为。
测试用的 schema 与生产完全一致（同一个 `db::schema::migrate`）。

### 重复规则只实现一遍

规则语义（短月收敛、工作日跳过周末、逾期不补实例、cron 的「日或周」）
全部在 Rust 的 `recurrence.rs` 里，前端通过 `preview_next_occurrence` 命令
拿结果来展示。前端**不重算**，避免前后端算法漂移。

### 窗口拖动：原生，不用 JS 模拟

`useDragRegion.ts` 在 `document` 上挂捕获阶段的 `mousedown`，判断目标不是交互元素、
也不在滚动条上时，调用 `getCurrentWindow().startDragging()`。

- 底层是 `ReleaseCapture()` + `SendMessage(WM_NCLBUTTONDOWN, HTCAPTION)`，
  由系统窗口管理器接管移动循环，**跟手程度与拖动原生标题栏完全一致**，
  JS 全程零参与；
- 不用 `data-tauri-drag-region`：它只能标在具体元素上，无法排除「按在滚动条上」，
  在窄面板里误触概率很高；
- 按钮、输入框、下拉、拖拽排序手柄都带 `data-no-drag`，
  且用语义选择器（`button` / `input` / `[role=switch]` …）兜底，
  新写的组件只要用对标签就自动安全。

### 窗口圆角：交给 DWM，且 CSS 半径必须 ≤ DWM 半径

无边框透明窗口画圆角有个陷阱。Windows 的窗口圆角只有
`DWMWA_WINDOW_CORNER_PREFERENCE` 一个入口，而它**只提供固定档位**
（`DEFAULT` / `DONOTROUND` / `ROUND` / `ROUNDSMALL`），**没有自定义半径** ——
Win11 的 `ROUND` 约 8px。

于是有个必须避开的组合：**CSS 画 20px、DWM 裁 8px**。这时 8~20px 那一圈，
DWM 认为在窗口外（不画背景）、CSS 认为在面板外（不画着色），
结果就是四角露出一圈桌面 —— 比圆角小更难看得多。

所以规则是 **CSS 半径必须小于等于 DWM 裁切半径**：这样 CSS 的圆角完全落在
DWM 的裁切区内，不会产生任何缝隙。半径由 Rust 探测后回报给前端
（`GlassInfo.cornerRadius`），前端不硬编码这个数字 ——
它属于系统几何约束，不是视觉偏好。

> **与规格的偏差**：规格写的是"圆角 20px"，实际窗口圆角为 8px（Win11 系统值）、
> 0px（系统不支持时）。用 `SetWindowRgn` 可以做出任意半径，但代价是
> 每次窗口尺寸变化都要重设区域，而且区域是 1 位掩码、圆角没有抗锯齿。
> 为一个半径数字引入这两个问题不划算。卡片等内部元素的圆角仍是 16px，
> 落在规格给的 16-24px 区间内。

另外，**不用 CSS 留透明边距画阴影**：那样 Mica 会在边距里透出来，
四角变成方块。让窗口尺寸等于面板尺寸、由 DWM 负责圆角与阴影才是对的。

### 为什么不用 Tauri 的 `set_effects`，而直接调 `window-vibrancy`

`WebviewWindow::set_effects` 看起来是"官方推荐"入口，但读一下
tauri 2.11 的 `src/vibrancy/windows.rs` 就会发现它在 Windows 上不可用：

```rust
pub fn apply_effects(window: impl HasWindowHandle, effects: WindowEffectsConfig) {
  let WindowEffectsConfig { effects, color, .. } = effects;   // radius 被丢弃
  let effect = effects.iter().find(|e| matches!(e, Effect::Mica | ...)).unwrap();
  match effect {
    Effect::Mica => window_vibrancy::apply_mica(window, None),  // 返回值被丢弃
    ...
  };
}
```

三个后果：

1. 返回值是 `()`，外层永远 `Ok(())` —— **无法知道哪个效果真的生效了**；
2. 它只取候选列表里第一个匹配项，失败就什么都不做，**没有降级链**。
   Windows 10 上 Mica 失败 = 完全没有玻璃效果，而不是退到 Acrylic；
3. `radius` 被解构丢弃，**不做 DWM 圆角**。

所以 `glass.rs` 直接调 `window-vibrancy`，靠 `Err` 判断自己实现降级链
（Mica → Tabbed → Acrylic → Blur → CSS 兜底），并自行通过 `dwmapi` 设置圆角。
`window-vibrancy` 内部会检查系统版本，Win10 上调 `apply_mica` 必然返回 `Err`，
于是自然落到 Acrylic。

DWM 圆角那部分用了一个极小的 FFI 直调 `dwmapi.dll`
（一个函数 + 两个常量），没有引入 `windows` / `windows-sys` 依赖。

### 设置存储：SQLite 为主，文件缓存只为首帧

`settings` 表按**字段一行**存（key = 字段名，value = JSON），
缺失字段回落到默认值，所以任何历史版本的库都能安全读出来，新增字段不需要迁移。

另有一份 `appearance.cache.json`（tauri-plugin-store），只存主题、透明度、字体大小
这几项，用途单一：**窗口创建时必须立刻拿到这些值来生成初始化脚本**，
而那时数据库可能还没打开（开机自启的延迟期内）。它是 SQLite 设置的**派生副本**，
不是第二份事实来源。

前端拿到值的方式是 Rust 的 `initialization_script` 注入
`window.__GLASSNOTE_BOOT__`，在页面任何脚本之前执行 —— 所以首帧就是正确的主题，
不会闪白。

### 为什么没引 Radix / Framer Motion

- **UI 基元**：`components/ui/` 按 shadcn/ui 的约定组织（cva 变体、`cn()` 合并、
  CSS 变量主题），但没有引 Radix。这里只需要按钮、开关、滑块、弹层这类基础控件，
  Radix 的核心价值（无障碍弹层、焦点陷阱）用不上，而规格明确要求不引大型 UI 框架。
  代码保持了 shadcn 的调用写法，将来要换成官方组件不需要改调用处。
- **动画**：全部用 CSS `transition` / `transform` / `opacity`。
  折叠动画用 `grid-template-rows: 0fr → 1fr` —— `max-height` 需要猜一个上限，
  猜小了内容被截断、猜大了动画前半段空跑。`grid` 方案不需要知道内容高度，
  是纯 CSS 里唯一正确的做法。不引 Framer Motion 也就没有「动画必须可暂停」的问题。

### 调度器不补发过期提醒

关掉应用一整晚再打开时，昨晚的提醒已经没有意义。超过 12 小时的过期提醒
只记一条日志、不发通知 —— 补发一堆过期通知比不提醒更烦人。

---

## Windows 10 / 11 差异与降级

应用启动时会按顺序尝试 Mica → Tabbed → Acrylic → Blur，用**第一个成功**的效果，
失败时回落到纯 CSS 半透明。设置面板的「关于」里会显示当前实际生效的效果。

| 效果 | 最低系统 | 说明 |
|---|---|---|
| Mica / Tabbed | Windows 11 (22000+) | 系统级云母，跟随桌面壁纸，开销最低 |
| Acrylic | Windows 10 v1809+ | 亚克力模糊，Win11 上也能用 |
| Blur | Windows 7 / Win10 v1809+ | 基础高斯模糊 |
| CSS 半透明 | 任意 | 系统玻璃完全不可用时的兜底 |

### 判定方式

**不读系统版本号**，而是看「哪个效果真的成功了」。理由：决定降级路径的是
系统实际支持哪个效果，而不是营销版本号。Windows 11 上用户关掉
「设置 → 个性化 → 颜色 → 透明效果」后 Mica 和 Acrylic 都会失败，
此时报「用的是 CSS 兜底」才是对用户真实可见结果的准确描述。

### Windows 10 上的表现

- Mica 不可用 → 自动落到 Acrylic（`apply_mica` 内部检查系统版本，必然返回 `Err`）；
- **窗口是方角**：`DWMWA_WINDOW_CORNER_PREFERENCE` 是 Windows 11 才有的属性，
  Win10 上设置会失败，此时前端把 `--panel-radius` 置为 0，避免出现透明缺口。
  这是预期降级，不是 bug；
- 拖动手感、托盘、自启、快捷键与 Win11 完全一致；
- 不会崩溃、不会卡顿 —— 失败路径只是返回 `Err` 后继续试下一个。

### 多显示器与 DPI

- 拖动结束后自动做边缘吸附 + 位置校正（`snap_and_clamp`），
  吸附目标是**当前显示器的工作区**（已排除任务栏）而不是虚拟桌面；
- 窗口比工作区大时自动收缩，并保证不会完全跑到屏幕外；
- 拔掉显示器后可从托盘「显示 / 隐藏窗口」唤回，位置会自动校正。

---

## 功能清单

### 任务
- 快速添加（支持 `#标签` `!优先级` `今天/明天/周X HH:MM` 自然语言解析，解析结果实时可见）
- 完整字段：标题、备注、优先级（5 档）、标签、出现时间、提醒时间、重复规则
- 打勾完成 → 绿色对勾缩放 → 卡片淡出缩小滑出 → 列表重排
- 完成后 5 秒内可撤销（撤销会精确回滚重复任务的 `due_at`）
- 删除进回收站，可恢复、可彻底删除、可清空
- 置顶、搜索、按标签/优先级筛选、按时间/优先级/创建时间/手工排序
- 拖拽排序（按住行左侧手柄）
- 稍后提醒：5 分钟 / 1 小时 / 明天

### 重复规则
- 一次性、每天、每周（可多选星期）、每月（可指定日期）、每个工作日、自定义间隔（每 N 天/周/月）
- Cron 表达式（5 段，支持 `*` `/` `a-b` `*/n` 逗号列表；日与周同时限定时取「或」）
- 重复结束日期
- 跳过本次（不记完成，直接推进）
- 编辑器内实时预览「下次出现时间」

### 备忘录
- 多张卡片，独立标题/正文/颜色/置顶
- **点击顶栏折叠/展开**，折叠后只显示标题与一行摘要，高度平滑过渡
- **折叠状态持久化**，重启后保持
- 自动保存（防抖 500ms），未保存时标题栏显示「保存中」
- Markdown 预览（标题、粗体、斜体、行内代码、代码块、列表、引用、分隔线、链接）

### 视图
- 顶部 Tab：今日 / 全部 / 备忘录 / 已完成 / 回收站
- 空状态有图标与引导文案
- 底部快速添加，回车保存、Esc 取消
- 全局搜索（Ctrl+F）

### 提醒与通知
- 到点发送系统通知
- 应用内同步弹提示，带「稍后 5 分钟」快捷操作
- 重复任务每次出现都会重新提醒
- 免打扰时段（支持跨零点，如 22:30–07:30）

### 设置
- 开机自启 + 延迟秒数
- 透明度 20%–100%（只调背景着色层，文字始终满不透明度）
- 主题：白天 / 黑夜两套固定配色；字体大小
- 窗口置顶、托盘图标开关、关闭到托盘、隐藏时释放内存
- 默认重复规则、默认提醒提前量
- 快捷键自定义（录制式输入）
- 数据备份 / 导入 / 导出 JSON·Markdown·CSV / 立即清理 / 回收站保留天数
- PIN 隐私锁
- 重置设置 / 清除全部数据

### 快捷键

| 作用域 | 按键 | 行为 |
|---|---|---|
| 全局 | `Ctrl+Alt+N` | 弹出快速捕获浮窗 |
| 全局 | `Ctrl+Shift+Space` | 显示 / 隐藏主窗口 |
| 窗口内 | `Enter` | 保存 |
| 窗口内 | `Esc` | 取消 / 关闭搜索 |
| 窗口内 | `Ctrl+F` | 搜索 |
| 窗口内 | `Ctrl+N` | 新建任务 |
| 窗口内 | `Ctrl+,` | 打开设置 |

全局快捷键可在设置里改。注册在 Rust 侧，所以**窗口被销毁时依然有效** ——
否则用户关了窗口就再也叫不出来了。

### 系统托盘
- 左键单击：显示 / 隐藏窗口
- 右键菜单：显示/隐藏、快速添加、今日待办数、设置、开机自启（勾选项）、退出
- 图标徽标显示未完成数量（用 3×5 点阵字模在运行时绘制，未引入图像库）

---

## 常见问题

**Q：`npm run tauri dev` 报找不到 `cargo`**
见上文「如果 `cargo` 命令找不到」。改完 PATH 记得重开终端。

**Q：编译时报 `link.exe returned an unexpected error` / `link: extra operand` / `LNK1181`**

**这是 Git Bash 的坑，不是缺 VS 构建工具。** 会连续撞上两个不同的错，第二个更隐蔽。

**第一个错：`link: extra operand`**

Git for Windows 在 `<Git安装目录>\usr\bin\` 里带了一个 coreutils 的 `link.exe`
（做硬链接的小工具），而 Git Bash 把 `/usr/bin` 放在 PATH 很靠前的位置。
rustc 调用 `link.exe` 时撞上它 —— 它只接受两个参数，于是报 `extra operand`，
rustc 又把它包装成「你可能需要安装 C++ 生成工具」这句有误导性的提示。

**第二个错：`LNK1181: 无法打开输入文件 "kernel32.lib"`**

把 MSVC 的 `bin` 手动提到 PATH 最前面之后，就会换成这个错。
原因是链接器不只依赖 PATH，还要靠 **`LIB` / `INCLUDE`** 找到 Windows SDK 的库与头文件，
而这两个变量只有 `vcvars64.bat` 会设置。只改 PATH 是不够的。

所以正确做法是**先建立完整的 MSVC 开发环境**，而不是修 PATH。

三种修法，任选其一：

```bash
# 方案 A（最省事）：用项目自带的包装脚本。
# 它会定位 vcvars64.bat、把环境变量导入当前 bash，并自检 link.exe 与 LIB 是否就位。
./scripts/dev.sh
```

```bash
# 方案 B：在「Developer PowerShell for VS 2022」或「x64 Native Tools Command Prompt」里跑
# （开始菜单里搜得到）。这类终端的环境已经由 vcvars 配好。
npm run tauri dev
```

```bash
# 方案 C：在 Git Bash 里手动加载 vcvars64 再执行（最稳，不依赖当前 shell 环境）
cmd //c '"D:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" && npm run tauri dev'
```

用普通的 PowerShell 或 CMD 打开终端不会有第一个错（PATH 里没有 Git 的 `usr/bin`），
但如果没进过 Developer 终端，仍可能撞上第二个错 —— 这时用方案 A 或 B。

**Q：首次 `tauri dev` 卡在编译很久**
Tauri 的依赖树有 500 多个 crate，首次全量编译确实慢。之后增量编译只需几秒。
依赖下载受网络影响较大，如长期停滞可配置国内镜像（见下）。

**Q：开发模式下系统通知不出现**
Windows 的通知需要应用有 AppUserModelID，而开发模式下的 exe 没有注册到开始菜单。
**打包安装后通知正常。** 这是 Windows 的机制限制，不是代码问题。

**Q：关掉窗口后应用还在运行**
这是预期行为 —— 默认「关闭窗口时最小化到托盘」。要从托盘完全退出，
右键托盘图标选「退出 GlassNote」，或在设置里关闭「关闭到托盘」。

**Q：任务打勾后立刻消失，找不到在哪**
「今日」视图只显示到点的任务。已完成的任务在「已完成」Tab，
删除的在「回收站」Tab。另外完成后 5 秒内底部会有「撤销」按钮。

**Q：每天的任务完成后，第二天什么时候出现？**
按你设置的**出现时间**。如果设的是 00:00 就是零点，设的是 09:00 就是次日九点。
没设出现时间的话默认次日 09:00。

**Q：改了任务的出现时间，提醒时间没跟着变？**
提醒时间是独立的绝对时间，不会被出现时间带着走 —— 这是需求里明确要求的
「提醒时间可独立于任务出现时间设置」。用编辑器里的「提前 N 分钟」按钮
可以从出现时间反推提醒时间。

**Q：隐藏到托盘后内存还是没降下来**
检查设置里的「隐藏时释放内存」是否开启（默认开启）。
开启时隐藏会销毁窗口，内存降到最低；关闭时只是隐藏窗口，
WebView2 仍然驻留，内存不会明显下降。

**Q：拖动窗口时按在列表上，结果把窗口拖走了 / 想滚动却拖动了窗口**
滚动条区域已被显式排除（`isOnScrollbar`）。如果你指的是列表**空白处**，
那是设计如此 —— 需求要求「任意空白处按住即可拖动窗口」。
要滚动请用滚轮，要拖动排序请按住行左侧的手柄。

**Q：四角露出一圈桌面 / 圆角看起来是方的**

圆角半径由 Rust 依据系统能力回报（`GlassInfo.cornerRadius`），前端通过
CSS 变量 `--panel-radius` 应用。**不要手动把它改大** ——
DWM 只有固定档位的圆角（Win11 约 8px），CSS 画得比它大，
两者之间那一圈就会既无系统背景也无面板着色，四角直接露出桌面。

Windows 10 不支持圆角裁剪，此时半径会回报为 0（方角），这是预期降级，
不是 bug。详见上文「窗口圆角」一节。

**Q：玻璃效果看起来不对 / 完全没有玻璃效果**

设置 → 关于 → 「玻璃效果」会显示当前实际生效的那一项。
如果显示「CSS 半透明（系统玻璃不可用）」，通常是系统里关掉了
「设置 → 个性化 → 颜色 → 透明效果」。

**Q：窗口打开了，但一片空白 / 只有一块背景色，什么都不显示**

按可能性从高到低查这三点：

**1）别用 `cargo build --release` 出分发包。** 必须用 `npm run tauri build`。
判断 dev / prod 的依据是 `tauri` crate 的 `custom-protocol` feature
（`tauri::is_dev()` 就是 `!cfg!(feature = "custom-protocol")`），
而这个 feature 只有 Tauri CLI 会开。直接 `cargo build --release` 得到的是
**开发模式**二进制 —— 它会去连 `devUrl`（`http://127.0.0.1:1420`）而不是用内嵌的前端资源。
没开 dev server 时，窗口里显示的就是 Chromium 的
「无法访问此页面 / ERR_CONNECTION_REFUSED」。

**2）WebView2 的配置目录被污染了。** 反复用 `taskkill /F` 强杀应用会让
`%LOCALAPPDATA%\com.glassnote.desktop\EBWebView` 里堆积大量
`Local State<hash>.tmp` 残留，攒到一定程度后 WebView2 会起不来 ——
症状是**窗口在、只有 Mica 背景、前端完全不执行**。
删掉整个 `EBWebView` 目录再启动即可（会重建，只丢缓存）：

```powershell
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\com.glassnote.desktop\EBWebView"
```

想确认 WebView2 到底起没起，看有没有属于本应用的 WebView2 进程：

```powershell
Get-CimInstance Win32_Process |
  Where-Object { $_.CommandLine -like '*glassnote*' -and $_.Name -eq 'msedgewebview2.exe' } |
  Select-Object ProcessId
```

正常应该有 5~7 个。一个都没有就是上面第 2 条。
注意 Windows 自带的搜索（`SearchHost.exe`）也会起一堆 `msedgewebview2`，
所以必须按命令行里的 `glassnote` 过滤，不能只数进程个数。

**3）`devUrl` 不可达。** `vite.config.ts` 里显式设了 `host: "127.0.0.1"`。
不设的话 Vite 默认的 `localhost` 在 Node 17+ 上会解析成 `::1`，
只监听 IPv6 回环，而 WebView2 里的 `localhost` 优先走 IPv4 —— 导航直接失败，
且没有任何报错。`tauri.conf.json` 的 `devUrl` 同样写死 `127.0.0.1`，两边不留歧义。

**Q：`tauri build` 报 `failed to bundle project` / 下载 NSIS 或 WiX 失败**

打包安装包需要 Tauri 从 GitHub Releases 下载 NSIS（约 2MB）和 WiX。
如果报 `peer closed connection without sending TLS close_notify` 或 `CONNECT tunnel failed`，
说明是网络（代理）挡住了 `objects.githubusercontent.com`，与代码无关。

三种处理：

```bash
# 1) 挂代理重试（设置 HTTPS_PROXY 后重跑）
npm run tauri build
```

```bash
# 2) 只做便携版，完全不需要 NSIS/WiX：
#    先构建出 exe，再在它旁边放一个空的 portable 标记文件即可
cargo build --release --manifest-path src-tauri/Cargo.toml
# 然后手动整理目录：
#   dist-release/
#     glassnote.exe           ← src-tauri/target/release/glassnote.exe
#     glassnote.portable      ← 空文件
```

```bash
# 3) 手动下载 nsis-3.11.zip，解压到 Tauri 的缓存目录后重试
#    Windows 缓存位置：%LOCALAPPDATA%\tauri\NSIS\
#    下载地址：https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.11/nsis-3.11.zip
```

> 如果三条路都走不通，便携版是完全不依赖外部下载的兜底方案：
> 只要 `glassnote.exe` 加一个空的 `glassnote.portable` 标记文件就是可用程序。

**Q：想换成国内镜像加速依赖下载**

在 `~/.cargo/config.toml` 里加：

```toml
[source.crates-io]
replace-with = "rsproxy-sparse"

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"
```

前端同理：`npm config set registry https://registry.npmmirror.com`。

---

## 项目结构

```
glassnote/
├── index.html                 主面板入口（含首帧引导脚本）
├── capture.html               快速捕获浮窗入口
├── src/                       前端（React + TS）
│   ├── main.tsx / App.tsx     两个入口与主面板
│   ├── capture.tsx            快速捕获浮窗
│   ├── types.ts               与 Rust 共享的数据契约（camelCase 投影）
│   ├── styles.css             两套配色变量、面板分层、全部 keyframes
│   ├── lib/
│   │   ├── ipc.ts             全部 invoke 收敛于此，命令名只定义一处
│   │   ├── boot.ts            引导数据读取与应用
│   │   ├── utils.ts           时间格式化、重复规则文案
│   │   ├── markdown.ts        极简 Markdown 渲染（先转义再替换）
│   │   └── quickparse.ts      快速添加的自然语言解析
│   ├── store/useStore.ts      Zustand 状态中枢
│   ├── hooks/
│   │   ├── useDragRegion.ts   原生窗口拖动（含滚动条排除）
│   │   ├── useVirtualList.ts  固定行高虚拟滚动
│   │   └── useBackendEvents.ts 后端事件订阅（无轮询）
│   └── components/
│       ├── GlassBackdrop.tsx  玻璃分层容器
│       ├── TitleBar.tsx       顶栏（拖动区 + 折叠 + 快捷控制）
│       ├── TaskRow / TaskList / TaskEditor
│       ├── MemoList.tsx       备忘录卡片与折叠
│       ├── QuickAdd.tsx       底部快速添加
│       ├── SettingsPanel.tsx  设置面板
│       ├── Toasts.tsx         轻提示与撤销
│       └── ui/                shadcn 风格基元（Button/Field/Controls/Overlay）
├── scripts/gen-icons.py       图标生成（可复现，改配色只需动常量）
└── src-tauri/                 后端（Rust）
    ├── Cargo.toml
    ├── tauri.conf.json        窗口刻意留空，由代码按需创建
    ├── capabilities/default.json  前端直接调用的能力白名单
    └── src/
        ├── lib.rs             启动顺序、插件注册、命令注册
        ├── main.rs            薄壳
        ├── state.rs           全局状态（数据库懒初始化）
        ├── paths.rs           数据目录解析（含便携模式）
        ├── error.rs           统一错误类型
        ├── models.rs          数据模型（时间一律 Unix 毫秒）
        ├── settings.rs        设置读写、免打扰判定、引导缓存
        ├── recurrence.rs      重复规则引擎（唯一实现）
        ├── scheduler.rs       唯一的定时源
        ├── glass.rs           系统级玻璃与降级链
        ├── tray.rs            托盘与运行时徽标
        ├── shortcuts.rs       全局快捷键
        ├── db/
        │   ├── mod.rs         连接、PRAGMA、备份、轮转
        │   └── schema.rs      建表与迁移（user_version）
        └── commands/          命令层
            ├── tasks.rs       任务 CRUD、完成/撤销、重复推进
            ├── memos.rs       备忘录
            ├── settings.rs    设置、自启、快捷键、隐私锁
            ├── window.rs      窗口创建、玻璃、折叠、吸附
            └── data.rs        备份、导入导出、维护
```

### 数据库结构

```sql
tasks(id, title, note, status, priority, tags, due_at, remind_at,
      repeat_type, repeat_interval, repeat_unit, repeat_weekdays,
      repeat_monthday, repeat_end_at, repeat_cron, parent_id,
      pinned, sort_order, created_at, updated_at, completed_at,
      deleted_at, remind_fired_at)

memos(id, title, content, color, collapsed, pinned, sort_order,
      created_at, updated_at, deleted_at)

settings(key, value)

completed_logs(id, task_id, task_title, completed_at, action, prev_due_at)

recurrence_exceptions(id, task_id, exception_date, action)
```

索引覆盖 `status`、`due_at`、`remind_at`、`deleted_at`，以及列表主查询
`status='todo' AND deleted_at IS NULL AND due_at <= ?` 对应的复合索引
`(deleted_at, status, due_at)`。

连接级 PRAGMA：WAL、`synchronous=NORMAL`、`foreign_keys=ON`、
`cache_size=-4000`（约 4MB，把内存钉死）、`temp_store=MEMORY`。

---

## 已知限制

- **空闲内存 70.2MB 还能再压，但需换架构**。WebView2 进程模型即便合并成单进程
  也仍有 ~70MB 的底线（browser + renderer + JS 运行时 + Chromium 库）。要再往下
  到 15–20MB 量级得换掉 Tauri，改用 Rust 原生 GUI（egui / iced / slint / gpui）。
  这意味着 React → Rust 重写、失去 Web 生态 —— 对便签应用不划算。

- **WebView2 配置目录可能因强杀而污染**。反复用 `taskkill /F` 强杀应用，会在
  `%LOCALAPPDATA%\com.glassnote.desktop\EBWebView\` 里堆积
  `Local State<hash>.tmp` 残留，攒多了后 WebView2 会起不来 —— 表现是窗口在、
  只有 Mica 背景。常规使用不会触发，自动化测试才会。修法：删掉整个
  `EBWebView` 目录，WebView2 会重建，只丢缓存。

- **tauri 任务面板里的 `.task-done` 动画需要 keyframes 都在 styles.css**。
  Tailwind 只在按对应的 `animate-*` 工具类被用到时才产出 keyframes，
  而 `.task-done` / `.check-pop` 是普通类（不是 `animate-task-done`），
  它们依赖的 `@keyframes` 必须和它们放在同一个文件里。这条注释在 styles.css
  也有，写给以后动 keyframes 的自己。

- **Vite 默认只绑 IPv6 回环**。`vite.config.ts` 里必须显式设
  `host: "127.0.0.1"`，否则它把 `localhost` 解析成 `::1` 并只监听 IPv6，
  而 WebView2 的 `localhost` 优先走 IPv4 —— `devUrl` 实际连不上、窗口停在
  about:blank 且无任何报错。`tauri.conf.json` 的 `devUrl` 也写死 IPv4，两边
  不留解析歧义。

- **仅 Windows 完整支持。** 玻璃效果与边缘吸附依赖 Win32 API；
  其他平台会回落到 CSS 半透明，功能可用但视觉不同。
- **PIN 锁不是加密。** 4-8 位纯数字 PIN 的熵很低，加盐 SHA-256 挡不住
  拿到数据库文件后的暴力破解。它的作用是**防止他人随手打开窗口看到内容**，
  不是保护数据机密性。
- **重复规则实例不单独存行。** 修改整个系列会同时影响历史与未来，
  目前不提供「仅修改本次」的独立实例记录（`recurrence_exceptions` 已预留了
  `skip` / `done` 两种动作，扩展「仅本次」需要再引入实例表）。
- **子任务**表结构（`parent_id` + 级联删除）已就绪，UI 暂未展开。
- **日历/时间轴视图**未实现，当前提供的是「今日 / 全部 / 即将开始」三种列表口径。
- **多语言**：设置项 `language` 已预留，界面文案目前为中文。
- **自动更新**未启用，需要自建签名密钥与 `latest.json` 托管。
