---
status: SPEC_DRAFTED
owner: product-strategist
related_adrs: []
related_specs: [00-product-overview, cross-platform-build]
created: 2026-05-06
updated: 2026-05-06
revised: 2026-05-06 — P2-3 UX 段由 ux-designer 填写
priority: P0
---

# floating-window — 320×420 透明置顶磨砂玻璃浮窗主界面

## 1. 问题（为什么做）

Sync Copy 的产品定位是"轻度伴随"工具——用户在前台干活，复制/粘贴在后台自然发生，但仍需要一个**始终可见**的入口来：看连接状态（小组几台？连上了？）、看历史、加入新设备、退出。窗口必须始终置顶（不能被 IDE / 浏览器盖住），形态要够"轻"（不能像普通应用窗口压过用户视野），同时承载"主菜单 + 状态栏 + 历史 + 底部信息"四象组合。这是 v2 进入 UI 阶段后第一个能看见的产物——后续所有前端组件都嵌在它里面。

## 2. 用户故事

- As a multi-machine user, I want a small always-on-top window that shows current group state at a glance and never blocks my main workflow, so that I can ignore it 99% of the time and only glance at it when needed.
- As a user, I want to drag the window anywhere on the screen by grabbing its top area, so that I can park it in the corner that suits my current task without keyboard shortcuts.
- As a Mac user, I want the window to feel native (rounded corners, frosted glass blur, subtle shadow), so that it does not look like a foreign embedded webview.

## 3. 范围

**in scope**：
- 单 Tauri window，label `main`，logical 320×420（默认尺寸）；`resizable` 暂定 `true`（沿用 v0），但用户主动拖拽尺寸不在 v2 v0.1 UX 测试范围内（见 out of scope）；待 ADR 决定是否改 false（见 第 7 节 [P1] [架构师]）
- 始终置顶（`alwaysOnTop: true`）
- 透明背景 + 内容层 `backdrop-filter: blur(20px)` 磨砂玻璃（macOS 需 `macOSPrivateApi: true`）
- 无原生窗口装饰（`decorations: false`）；10px 圆角 + 1px 微高亮边
- 顶部拖动条由前端 `data-tauri-drag-region` 标签声明
- 不可最大化 / 不可全屏（避免 Win 双击标题栏触发最大化 → 透明窗体闪白屏）
- 启动时窗口居中到主屏（`center: true`）
- 多屏支持：每次 show（含从托盘恢复）调用 `ensure_on_screen`，若超过半个窗口在屏幕外则居中到当前显示器
- 关闭按钮（×）行为：隐藏窗口（不退出应用，仍在托盘）

**out of scope**（v2 这个 feature 不做，留后续 feature）：
- 历史列表内容渲染（属于 `history-list`）
- 顶部状态栏的状态点 / 加入按钮 / 设置按钮交互（属于各自 feature；本 spec 仅约定**容器位置**，不约定**容器内容**）
- 缩为悬浮球的能力（属于 `floating-ball`）
- 用户自定义字段、设置面板内容（属于 `settings-panel`）
- 窗口尺寸记忆与恢复（v0 用 `lastExpandedSize`，仅在 collapse 流程中相关）
- 边缘吸附 / 半隐藏（v0 实现过又删除——v2 不做）

## 4. 验收标准（Definition of Done）

- [ ] 启动应用，浮窗在主屏中央显示，logical 尺寸 320×420，背景半透明可见后方桌面/应用
- [ ] 浮窗在最前置：打开任意全屏 IDE / 浏览器，浮窗仍可见（macOS 全屏空间例外，与 Mac OS 自身全屏空间隔离一致）
- [ ] 鼠标在窗口顶部拖动区按住拖动，窗口跟随移动；松手保留位置
- [ ] 窗口角是 10px 圆角，Mac 上 `backdrop-filter: blur(20px)` 生效（能看到桌面纹理透出）。Win 上的视觉表现（真模糊 / fallback / Mica 材质）行为见 第 7 节 [UX] 待答；本 spec 不强制 Win 上具体呈现形态，但要求与 Mac 视觉差距不至于让用户感觉是"两套产品"
- [ ] Win 上双击窗口顶部不触发系统 "最大化"，窗口尺寸不变
- [ ] 关闭按钮（×）点击后窗口 hide，应用仍在托盘运行；从托盘点击可重新唤出
- [ ] 把窗口拖到屏幕外的位置后，从托盘左键重新展示时窗口被自动拉回当前显示器中央

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的
`legacy-prototype` 分支 `src-tauri/tauri.conf.json` 的 `app.windows[0]` 写了 `width:320, height:420, resizable:true, maximizable:false, fullscreen:false, decorations:false, transparent:true, alwaysOnTop:true, center:true, shadow:true, macOSPrivateApi:true`。前端 `src/routes/+page.svelte` 的 `.window` CSS：`width:100vw; height:100vh; padding:8px 10px; background:rgba(28,28,32,0.88); border-radius:10px; border:1px solid rgba(255,255,255,0.08); backdrop-filter:blur(20px);`。`lib.rs` 的 `ensure_on_screen` 在托盘点击或 `window-shown` emit 时调用。

### 5.2 v0 暴露的具体坑
- "Windows 双击标题栏触发最大化导致透明窗口闪白屏"——v0 经过几次 Win 用户实测后，加了 `maximizable:false, fullscreen:false` 才稳定。这是**隐式**的不变式（注释里有，但易被回归）
- `decorations:false` + `transparent:true` 在 Mac 上必须配 `macOSPrivateApi:true`，否则 backdrop-filter 不生效。这条 Mac/Win 差异要在 spec 里点名
- 窗口拖到屏幕外的恢复策略经过反复调整（`ensure_on_screen` 用"半可见"门槛而不是"完全可见"，避免误伤用户故意拖到边缘的窗口）
- v0 一度做过"边缘吸附"（窗口靠边自动半隐藏，鼠标接近恢复）—— `34ace33` 加，`a09ef6c` 删——增加复杂度但用户反馈鸡肋

### 5.3 v2 应继承
- 320×420 logical 尺寸 + 10px 圆角 + 1px 微高亮边 + 88% 不透明背景
- `decorations:false` + `transparent:true` + `alwaysOnTop:true`
- `maximizable:false, fullscreen:false` 防 Win 闪屏
- `ensure_on_screen` 半可见恢复策略

### 5.4 v2 应挑战
- 单 `+page.svelte` 1483 行的反模式：本 spec 仅定义"窗口容器"，里面承载的子组件（StatusBar / HistoryList / Footer / 各 Dialog）必须由架构师在 ADR 中规划独立组件，禁止延续 v0 的单文件堆砌
- 是否考虑 Win11 Mica 材质（`@windows/mica` 之类）以让 Win 上视觉与 Mac 接近？v0 没做导致两边视觉差距明显
- 关闭按钮（×）vs 最小化（−）vs 退出三者交互在 v0 略乱（× = hide，− = collapse to ball，⚙ → 退出）；v2 应让 spec 与 UX 段一起明确

## 6. UX 设计

> 本节由 ux-designer 在 2026-05-06 填写。本 spec 是**视觉语言字典的定义源**——其余 5 份 spec 的第 6 节引用此处，不重复定义。

### 6.1 信息架构

本视图是所有 UI 的外层容器，自身承载以下信息（按优先级）：

1. 连接状态（顶部状态栏：状态点 + 小组台数 + 操作入口）
2. 历史列表区域（中央，90% 面积，属 history-list 规格）
3. 本机标识（底部：IP:PORT 左 + 设备名右）
4. 品牌落款（最底部极小字）

窗口本身不展示业务数据，只定义**布局分区**和**视觉容器**。

### 6.2 关键流程图（文字版）

主路径（窗口生命周期）：

1. 应用启动 → 浮窗居中在主屏出现 → 显示 main view（历史列表为空态）
2. 用户拖动顶部区域 → 窗口跟随移动 → 松手落点保留
3. 用户点击 × → 窗口 hide → 托盘图标仍在 → 托盘左键唤回
4. 从托盘唤回 → `ensure_on_screen` 检查位置 → 若超出当前显示器半个窗口则居中

异常路径：

- 拔副屏时：窗口位置由操作系统保留在离开时的坐标，下次 show 时 `ensure_on_screen` 拉回主屏
- Win 上 backdrop-filter 不生效时：回退到半透明纯色背景（见 6.5 状态字典 + 6.6 边界与例外）
- 用户误把窗口拖到完全离屏：不自动干预，等用户通过托盘唤回时再拉回

### 6.3 ASCII wireframe（必填）

```
┌────────────────────────────────┐  ← 10px 圆角，1px 微高亮边
│  ● 小组 · 2 台  [加入]  −  ⚙  │  ← 顶部状态栏，36px 高，drag-region
├────────────────────────────────┤
│                                │
│   [历史列表区域]                │
│   (属 history-list spec)       │
│                                │
│                                │
│                                │
│                                │
│                                │
│                                │
│                                │
│                                │
│                                │
├────────────────────────────────┤
│  192.168.1.50:5858    工作 Mac │  ← 底部 footer，24px 高
├────────────────────────────────┤
│   Made with Claude · by Tao   │  ← brand line，16px 高
└────────────────────────────────┘
  ← 320px logical →
  ↕ 420px logical
```

顶部状态栏细节：

```
│ ●  小组 · 2 台    [加入]  −  ⚙ │
  ↑  ↑                 ↑     ↑  ↑
  状  文字            胶囊  折叠  设置
  态  12px            按钮  球   齿轮
  点
```

### 6.4 交互细节

点击区域划分：

- 整个顶部状态栏（36px 高）：`data-tauri-drag-region` 整行均可拖动，按钮除外
- `[加入]` 胶囊按钮：primary blue，点击切换到 join view
- `−` 按钮：12px 文字大小，点击收缩为悬浮球
- `⚙` 按钮：点击切换到 settings view；settings view 时隐藏，× 显现
- `×` 关闭按钮：点击 hide 窗口（不退出），仅在 settings view 才显示（main view 无 ×）
- 底部 IP:PORT 文字：可点击，点击后复制到剪切板 + 短暂显示"已复制"
- 底部设备名：只展示，不可点击

鼠标悬停反馈：

- 整个顶部拖动区（排除按钮）：`cursor: grab`，按下后 `cursor: grabbing`
- `[加入]` 胶囊：背景稍亮（brightness 提 10%）
- `−` / `⚙` / `×` 按钮：出现圆形半透明背景（white-12%）
- IP:PORT：`cursor: pointer`，下划线提示

点击反馈：

- `[加入]` 胶囊：100ms scale(0.95) 下压感
- 按钮类：同上
- IP:PORT 复制：文字变绿 + "已复制" 替换显示约 1.2 秒后恢复

状态颜色：见 6.5 状态与颜色字典。

键盘可达性：

- `Esc`：在 settings / join view 时退回 main view（等价于点 ×）
- `Tab`：在按钮间循环（顶部栏三个按钮 + 底部 IP）
- 不实现完整 ARIA 角色（工具性质，以鼠标为主要输入）

### 6.5 状态与颜色字典（全局视觉语言——其余 5 份 spec 引用此节，不重复定义）

#### 画布规格

| 属性 | 值 |
|---|---|
| 逻辑尺寸 | 320×420 px |
| 圆角 | 10px |
| 背景色 | `rgba(28, 28, 32, 0.88)` 即约 88% 不透明暗色 |
| backdrop-filter | `blur(20px)`（macOS）/ 半透明纯色 fallback（Win，见下） |
| 边框 | `1px solid rgba(255, 255, 255, 0.08)` |
| 阴影 | 由 Tauri `shadow: true` 控制，不在 CSS 层重复声明 |

#### 状态点颜色

| 状态 | 圆点颜色（hex） | 圆点颜色语义 | 状态文字 | 触发条件 |
|---|---|---|---|---|
| 未连接 | `#9ca3af` 灰 | 中性 | `未连接 · 0 台` | 无任何 peer 已加入 |
| 已连接 | `#22c55e` 绿 | 正常 | `小组 · N 台` | 有 ≥ 1 peer 在线 |
| 等待审批 | `#3b82f6` 蓝 | 进行中 | `等待对方同意…` | join 请求已发出但未决 |
| 错误 | `#ef4444` 红 | 异常 | `连接失败` | 握手 4xx/5xx / 超时 |

#### 按钮样式

| 类型 | 背景 | 文字 | 用途 |
|---|---|---|---|
| primary blue | `#3b82f6` | `#ffffff` | `[加入]` 胶囊 |
| ghost | `rgba(255,255,255,0.12)` | `#f3f4f6` | 次要操作（取消、一般按钮） |
| danger red | `#ef4444` | `#ffffff` | 退出应用、危险操作 |
| disabled | `rgba(255,255,255,0.06)` | `#6b7280` | 禁用状态（如历史为空时清除历史） |

#### 文字颜色阶梯

| 用途 | 颜色（hex） | 透明度 |
|---|---|---|
| 主文字 | `#f3f4f6` | 100% |
| 次要文字（meta、hint） | `#9ca3af` | 100% |
| brand line | `#ffffff` | 22% |
| 危险提示 | `#ef4444` | 100% |
| 成功状态（"已复制"） | `#22c55e` | 100% |

#### 字号阶梯

| 层级 | 字号 | 用途 |
|---|---|---|
| 默认 | 13px | 历史条目主文字、按钮文字 |
| 次要 | 12px | 状态栏文字、meta 行（来源 + 时间） |
| hint | 11px | 输入框 placeholder、badge 文字 |
| footer brand | 9px | "Made with Claude · by Tao" |

#### 间距单位（4px 基准网格）

常用值：4 / 6 / 8 / 10 / 14 px。

- 顶部状态栏内边距：水平 10px，垂直居中（栏高 36px）
- 历史列表内边距：水平 8px，条目间距 4px
- 底部 footer 内边距：水平 10px，垂直居中（栏高 24px）
- brand line 内边距：垂直 4px

#### 字体栈

`-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif`

#### Win 平台 backdrop-filter 回退策略

- 首选：Edge WebView2 若支持 `backdrop-filter: blur()`，沿用 Mac 方案
- 回退：`background: rgba(28, 28, 32, 0.94)`（提高不透明度补偿无模糊的单薄感）
- 不做 Win11 Mica 材质（需原生 API，属架构师 ADR 议题）
- 两平台视觉差异可接受：颜色一致、圆角一致、布局一致；模糊有无不构成"两套产品"

### 6.6 边界与例外

- 历史 = 0 条时：中央区域显示空态（属 history-list 第 6 节定义），窗口仍全高显示
- 窗口宽度 < 280px 时：本 spec 暂定 `resizable` 保留但 min-width = 280px；文字不换行截断而非折叠
- 多显示器拔插：下次 show 时 `ensure_on_screen` 处理，不在事件发生瞬间介入（避免打断用户当前操作）
- 全屏应用（macOS Space）：alwaysOnTop 在全屏空间失效属系统限制，不做特殊处理
- 实测可能暴露的问题：Win 上 backdrop-filter 实际效果可能在不同 GPU/WebView2 版本间差异，需实机测试后回头调整 6.5 中的 fallback 策略

### 6.7 给前端工程师的实现提示（可选）

- 状态栏按钮的 hover 圆形背景建议用 CSS transition（opacity 过渡）而非 JS 控制，确保流畅
- IP:PORT 复制的"已复制"反馈建议用 CSS class toggle 控制文字内容切换，时间由 `setTimeout` 管理，不要持有额外 $state 变量
- 顶部拖动区与按钮的 pointer-events 分区要仔细：按钮必须 `pointer-events: auto` 脱离 drag-region 的拖拽语义
- 整个窗口的 `border-radius` 建议用 `overflow: hidden` 配合，避免子元素的背景色溢出圆角

### 6.8 给主窗口和 PM 的反馈（UX 开放问题回应）

**问题 1**：Win 上 `backdrop-filter: blur` 是否退化为半透明纯色？需 fallback 策略。

结论：是，应预备 fallback。策略已在 6.5 定义：回退为 `rgba(28, 28, 32, 0.94)`（提高不透明度）。不追求 Win11 Mica，两平台颜色和布局保持一致即达标。从 UX 角度，视觉差异在可接受范围内；是否启用 Mica 属架构师 ADR 决议。

**问题 2**：圆角 + 阴影在 Win 上的额外配置。

结论：圆角靠 CSS `border-radius` + `overflow: hidden` 实现，不依赖系统；阴影靠 Tauri `shadow: true`。Win 上阴影范围可能溢出透明区域，但属轻微视觉问题，不阻塞功能。建议架构师在 ADR 中记录"Win 上 shadow 溢出为已知可接受缺陷"。

**问题 3**：拓扑变化（拔副屏）时的恢复策略。

结论：不主动监听显示器拓扑事件，依赖 show 时的 `ensure_on_screen` 兜底。用户拔副屏后不会立即看到异常——只有下次从托盘唤出或切换窗口显示时才触发检查。这个延迟恢复在"轻度伴随工具"定位下可接受。主动监听是 P2 优化项，列入 第 7 节 [P2] [UX] 已追踪。

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 2 条] [P1 3 条] [P2 2 条]

- [P0] [UX] Win 上 `backdrop-filter: blur` 在 Edge WebView2 是否真生效？v0 实测有时退化为半透明纯色——是否需要平台条件 CSS 给 Win 一套 fallback（半透明纯色 / Win11 Mica 材质 / 其它）？决议直接影响 第 4 节 验收 #4 的 Win 期望
- [P0] [架构师] `ensure_on_screen` 的实现位置：Rust 后端还是前端？v0 在后端，前端通过事件触发；是否反过来更适合（前端有更细的视口信息）？与 `floating-ball` 第 7 节 [P0] 同议题
- [P1] [架构师] `resizable` 字段：v2 是 `true`（沿用 v0）还是 `false`（与 第 3 节 out of scope "用户拖拽尺寸不在 v2 v0.1 计划"对齐）？默认 `true` 也意味着用户偶然双击边缘可改尺寸 → 是否会引起浮窗越界
- [P1] [UX] 圆角 + 阴影在 Win 上是否需要额外配置（v0 仅靠 `shadow:true` 让 Tauri 渲染）？阴影范围对透明窗口可能溢出
- [P1] [架构师] 窗口标签固定 `main` 还是允许多窗口（如未来设置面板用独立窗口）？v0 全部塞在 main 的覆盖层里，组件耦合严重
- [P2] [架构师] 窗口位置 / 尺寸是否持久化？v0 不持久化，每次启动居中——简单，但用户若把窗口固定在某处会反感重置
- [P2] [UX] 用户主动拖到屏幕**完全外**（如 Win 多显示器拔掉副屏）后 `ensure_on_screen` 才介入；是否在显示器拓扑变化时主动检测一次？

## 8. Review 段（占位）

> code-reviewer / tech-architect 后续填写。

## 9. Code Review — PR-FE-1 / 2026-05-13 commit 5597afe

**结论**：CHANGES_REQUESTED（1 个低-中等必修 + 1 个 backend 配套必须在 PR-FE-2 前补；其余 nit）

### 9.1 4 聚焦点验证

- invoke / listen 命令名严格 mapping：✅ 部分。12/12 invoke 命令名完全对齐 `src-tauri/src/lib.rs` 第 61-73 行注册；DTO 字段（StatusInfo / PeerInfo / ConfigInfo / HistoryItem / PeerPendingPayload）严格镜像 `commands.rs` Serialize struct（含 `last_successful_sync_at: Option<String>` / `peer_hint: Option<String>` 的 null 映射）。3 个 listen 事件中 `status-updated` / `history-updated` 在 `commands.rs` 第 304/402/426/463/486 行有真 emit；**`peer-pending` 事件全 backend 0 emit**（grep 仅命中 `handshake.rs:406` 作为占位 device_id 字面值的字符串，与事件 emit 无关）— `+page.svelte:24` 订阅永远静默 → 见 第 9.2 节 [中等] 1。
- Svelte 5 runes 用法：✅。`$state` / `$derived` / `$props` 用法正确；`.svelte.ts` 模块 export 单 `$state({...})` 对象规避 `state_invalid_export` 是社区惯例；无闭包外 reactive 失效；`copyTimer` 用普通 `let` 不滥用 rune（符合 spec 第 6.7 节"不要持有额外 $state"）。
- 视觉语言字典 + spec 遵循：✅。逐项 diff 第 6.5 节字典对照 `tokens.ts` — 颜色 6 项（#22c55e / #9ca3af / #3b82f6 / #ef4444 / #f3f4f6 / 22% white）全对齐；尺寸 320×420 / 圆角 10px / 边框 0.08 / 背景 0.88 全对齐；字号 13/12/11/9 全对齐；间距 4/6/8/10/14 全对齐；字体栈逐字符相等；Win fallback 0.94 已落。`FloatingWindow.svelte` 三段布局（statusbar 36 + history-area flex + footer 24 + brand 16）对应第 6.3 节 wireframe；`data-tauri-drag-region` 用法正确，capability 已含。
- IPC 封装 + 错误处理：✅ 基本到位。`IpcError` class + `toIpcErrorCode` switch 映射 5 个通用 body（forbidden / not_found / invalid_input / internal_error / rate_limited）符合 ADR-008 MUST-3；每个 wrapper 含 try/catch + `wrapError`；事件 helper 全返 `UnlistenFn` 并在 `+page.svelte` `onDestroy` 用 `?.()` 防御 null。fatal user-visible 兜底（ErrorBoundary）按 spec 留 PR-FE-2，本批 OK。

### 9.2 必修补丁

#### [中等] 1. `peer-pending` 事件无 backend emit 配套
- 文件：`src/lib/ipc.ts:193` + `src-tauri/src/network/handlers/handshake.rs`（缺侧）
- 现象：前端 `onPeerPending` 订阅 `"peer-pending"` 事件，但后端无任何 `app_handle.emit("peer-pending", ...)` 调用。handshake handler 第 405 行注释"待审批"但只 insert PeerRegistry，不 emit。
- 风险：PR-FE-2 group-approval 弹框无法被触发。本批 `+page.svelte:25` `console.log` 永远不会执行 → 静默错觉"订阅生效"。
- 建议：开一个独立 backend PR（PR-7 或 PR-FE-1.5）在 handshake handler 收到 Pending peer 后调 `app_handle.emit("peer-pending", PeerPendingPayload{...})`。本前端 PR 已为 emit 配齐 DTO + 订阅 helper，等 backend 接入即贯通。
- 不阻塞本 PR-FE-1 合入（前端职责已尽），但**必须在 PR-FE-2 开工前补齐**，否则 PR-FE-2 验收会失败。

#### [低] 2. `FloatingWindow.svelte:121` 用未定义 CSS 变量 `var(--color-success)`
- 文件：`src/lib/components/FloatingWindow.svelte:23, 121`
- 现象：第 23 行 import `COLOR_TEXT_SUCCESS`，第 121 行却用 `var(--color-success)` —— 该 CSS 变量在 `app.html` / `tokens.ts` / 当前组件 `<style>` 中**均未定义**。
- 风险：用户点 IP:PORT 后"已复制"反馈不会变绿（fallback 到继承色 `#f3f4f6` 主文字）。导入的 `COLOR_TEXT_SUCCESS` 成死代码 → `svelte-check` 仍 0 warning 因为模板属性是字符串拼接。
- 建议：把 `var(--color-success)` 改成 `{COLOR_TEXT_SUCCESS}` 与上下文风格一致；一行修补。

### 9.3 [低] nit 列表

- (a) `app.html` 全局 `user-select: none` 应注意：未来若历史条目要允许文本选取需局部 re-enable（PR-FE-3 注意；spec 第 6.4 节"工具型，鼠标主"已为此奠基，本批不阻塞）。
- (b) `recopyHistoryItem` 对 image / file 类型会收到 `invalid_input` 错误（backend `commands.rs:541/545` 决定）—前端无对应类型守卫，PR-FE-3 list 渲染时记得不要对 image / file 条目挂"单击复制"。
- (c) `+page.svelte` async `onMount` 在 await 期间组件被销毁理论上会让 unlisten 永远不赋值；浮窗主入口组件不卸载，可忽略，但 PR-FE-2 引入子 view 切换时同 pattern 复制需谨慎。
- (d) `statusbar` 父 div 也加了 `data-tauri-drag-region`，且 `statusbar-left` 重复加 — 后者多余（父已包含），不影响功能。

### 9.4 测试覆盖评估

- backend `commands.rs` 单测 12 条已覆盖 DTO 序列化、boundary 错误映射、sanitize 等核心路径；前端本批无单测（合理 — 全是 thin wrapper + style），E2E / 手测留 qa-tester 跑 Tauri dev 验证 第 4 节 6 条 AC。
- 空白覆盖：本 PR 不含 AC #1-#6 的自动化验证（视觉性、需真窗口）—— 由 qa-tester 在 PR-FE-2 后跑手测 checklist。

### 9.5 owner 边界 + 过度工程自查

- owner 边界：`git show 5597afe --name-only` 仅 `src/` 域，0 文件溢出到 `src-tauri/` / `PLAN.md` / `specs/` / `decisions/`。✅
- 过度工程：本 review 段 ~62 行（略超预算 60，但 第 9.2 节两个问题需要详细 risk 描述；可接受）。无重复引用，无对子 PR 越权设计建议。

### 9.6 结论

本 PR 是高质量的前端脚手架第一砖：12 invoke wrapper、视觉字典严格对齐、Svelte 5 runes 规范、错误层规范。**1 个 [低] 必修（var(--color-success)）建议小补丁直接落，1 个 [中等] backend emit 配套需要在 PR-FE-2 前由 backend-implementer 补齐。** APPROVED 在 [低] 修完后即可推进 PR-FE-2；[中等] 由主窗口在 PLAN.md 跟进单独 backend PR。
