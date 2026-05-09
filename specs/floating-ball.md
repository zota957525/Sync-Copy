---
status: SPEC_DRAFTED
owner: product-strategist
related_adrs: []
related_specs: [00-product-overview, floating-window]
created: 2026-05-06
updated: 2026-05-06
revised: 2026-05-06 — P2-3 UX 段由 ux-designer 填写
priority: P1
---

# floating-ball — 浮窗收缩为 48×48 圆形悬浮球（点击展开 / 拖动移位）

## 1. 问题（为什么做）

浮窗 320×420 已经够小，但用户在屏幕空间紧张时（如配多窗口写代码 + 看文档）仍嫌它"占半个角"。Sync Copy 的产品定位是"轻度伴随"——能更轻就更轻。悬浮球形态：48×48 的小圆，停在屏幕边角，仅作"我还活着 + 单击展开"的视觉锚点。这是 v0 实战验证用户喜欢的形态（`f4be188` 之前已稳定），v2 必须保留但要把"拖动 vs 点击"的鼠标手势分辨重做（v0 用 8px 移动阈值 + 1500ms 时长上限，体验已收敛）。

技术挑战不在功能而在**手势消歧**：鼠标按下后**移动 ≤ 8 px** 视为点击（→ 展开）；**移动 > 8 px** 视为拖动（→ Tauri startDragging 接管）。一旦手势误判，用户会陷入"我点了一下怎么没反应 / 我想拖怎么变成展开了"的挫折。

## 2. 用户故事

- As a user with crowded screen real estate, I want to collapse the 320×420 floating window into a 48×48 ball that I can park in any screen corner, so that the app stops competing for my visual attention.
- As a user with the ball parked, I want a single click to expand it back to the previously remembered size, so that resuming work takes one tap.
- As a user, I want to drag the ball with mouse-down + drag-out > 8 px to move it across the screen (multi-monitor too), without that drag accidentally triggering an expand on release.
- As a user expanding the ball, I want the window to recall its prior expanded size (e.g., I had resized it to 320×600), so that "collapse + expand" round-trip preserves my layout.

## 3. 范围

**in scope**：
- 浮窗顶部状态栏 `−` 按钮触发 `collapseToBall()`：
  - `getCurrentWindow()` 取窗口句柄
  - `outerSize()` 返回**物理像素** + `scaleFactor()` → 算出**逻辑像素**记入 `lastExpandedSize: { w, h }`（默认 `{ w: 320, h: 420 }`）
  - `collapsed = true`（前端 $state，控制 UI 切换为球形态：圆形 48px 容器 + app icon SVG 居中 + 半透明背景 + 1px 微高亮边）
  - `setSize(new LogicalSize(48, 48))` 应用到窗口
- 球形态 UI：
  - 圆形（border-radius: 50%）+ 48×48 logical px
  - app-icon SVG 居中（与浮窗顶栏 logo 同款，设计统一）
  - 同浮窗的磨砂玻璃 + 88% 不透明度 + 微高亮边
  - **无**关闭按钮 / 设置按钮 / 状态文字（极简）
- 展开 `expandFromBall()`：
  - `lastExpandedSize` 取出 (w, h)；缺失时 fallback 到 `DEFAULT_EXPANDED = { w: 320, h: 420 }`
  - `collapsed = false`
  - `setSize(new LogicalSize(w, h))`
  - 展开后视口检查：取 `outerPosition()` + `outerSize()` + `currentMonitor()` → 若窗口右边缘 / 下边缘超出 `monitor.position + monitor.size` 则 `setPosition` 回拉，确保完全可见
- **手势消歧**（`onBallMouseDown(ev)`）：
  - 仅左键（`ev.button === 0`）
  - 记录起始 `screenX/Y` 与 `Date.now()`
  - 监听 window mousemove：移动距离 max(|dx|, |dy|) > 8 → 视为拖动 → cleanup listener + 调 `getCurrentWindow().startDragging()`（Tauri 原生窗口拖动）
  - 监听 window mouseup：cleanup listener + 若 `!didDrag && elapsed < 1500ms` → `expandFromBall()`
- `tauri.conf.json` 的 `minWidth: 40, minHeight: 40` 留空间给球（48 在 minWidth 之上）
- 与托盘联动：球形态下托盘左键单击仍然 toggle show/hide（hide 时整个窗口隐藏；show 时恢复球形态——保持上次形态）；不强制把球形态切回展开（用户可能就是想保持球形态）

**out of scope**：
- 边缘吸附（v0 加了又删，体验鸡肋；00 总览 第 5.2.4 节 + floating-window 第 3 节 已锁定不做）
- 球上显示状态徽章（连接状态点 / 未读计数）—— v0 short-lived 试过；v2 不做（球的产品定位是"完全极简"）
- 双击 / 右键 / 中键交互（仅左键拖 + 点）
- 球的尺寸用户可配（48 是设计选定值，不暴露）
- 球的**位置**持久化（v0 不持久化 + 启动总居中；位置层面 v2 锁死不持久化）。注：球的**形态**（启动时是球还是展开窗）是否持久化属 UX 开放问题，见 第 7 节 [P2] [UX]
- 球与浮窗的过渡动画（瞬时切换；动画属 UX 加分项后续评估）
- 球形态下的右键菜单（只有托盘有右键菜单）

## 4. 验收标准（Definition of Done）

- [ ] 浮窗 main view 顶部 `−` 按钮点击 → 窗口立即变为 48×48 圆形球 + 显示 app icon
- [ ] 球形态下单击（鼠标按下后移动 ≤ 8 px + 抬起 ≤ 1500ms）→ 窗口展开回**上次记住的尺寸**（如曾被用户拖到 320×600 则恢复到 320×600，否则 320×420）
- [ ] 球形态下按住拖动（移动 > 8 px）→ 窗口跟随鼠标移动；松开后**不展开**（手势识别为拖而非点）
- [ ] 球被拖到屏幕外 30%（半可见门槛）→ 浮窗顶部 → 任意操作（托盘左键 / `−` 按钮逻辑反向）展开时 `expandFromBall` 应保证窗口在监视器内完全可见
- [ ] 球形态下杀进程重启 → 默认回到展开态（球状态不持久化），位置回到屏幕中央
- [ ] 多显示器拖动：从主屏拖到副屏 → 球跟随；展开时窗口在球当时所在监视器内
- [ ] 球与展开浮窗共用同一窗口实例（label `main`），不创建新窗口
- [ ] 在球形态下托盘左键 toggle hide/show，重新 show 时仍是球形态（不自动展开）

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的
`legacy-prototype` 分支 `src/routes/+page.svelte`（约 295-380 行）：
- `BALL_SIZE = 48`，`DEFAULT_EXPANDED = { w: 320, h: 420 }`
- `collapsed` 是 `$state(false)`
- `lastExpandedSize: { w, h }` 普通变量（非 $state，跨函数共享）
- `collapseToBall()`：`outerSize()` 物理 → 除 `scaleFactor()` → 取整存 `lastExpandedSize`（注释强调"用 LogicalSize，setSize 也用 LogicalSize，避免 Retina 物理/逻辑混淆"）→ `collapsed = true` → `setSize(LogicalSize(48, 48))`
- `expandFromBall()`：取 `lastExpandedSize` 缺省回 DEFAULT → `collapsed = false` → `setSize` → 视口校正：`outerPosition` + `outerSize` + `currentMonitor` 算 `maxX/maxY` 边界 → setPosition 回拉
- `onBallMouseDown(ev)`：button !== 0 return；记录 startX/Y/at；onMove：dx/dy > 8 → didDrag=true cleanup + `startDragging()`；onUp：cleanup + `!didDrag && elapsed<1500` → expandFromBall。listener 注册在 window 上，cleanup 移除两个事件
- `hideWindow()`（顶部 `−` 按钮 onclick）：`banner = "收缩中…"` → `collapseToBall()` → 清 banner（注释：旧的 hide 是托盘 OS 隐藏，`−` 现在是 collapse）
- `tauri.conf.json` 的 `minWidth: 40, minHeight: 40` 让 setSize(48,48) 不被拒
- 球形态下 svelte 模板：`<div class="ball" onmousedown={onBallMouseDown}>` 嵌 app icon SVG
- CSS：`.ball { width: 100vw; height: 100vh; border-radius: 50%; ... }`（窗口本身已是 48×48，所以 vw/vh 充满）

### 5.2 v0 暴露的具体坑
- **8 px / 1500ms 阈值是经验值**：用户高 DPI 屏快速移动鼠标可能在 8px 内"颤抖"；触摸板用户拖动时 1500ms 太短易误判为点击。v0 没有设备级自适应
- **球形态下 hide（托盘 × 等价）→ show 切回球形态**：v0 行为 OK，但 ensure_on_screen 在 show 时若发现球在屏外会拉回——拉回的是球（48×48），用户看到一个孤零零的球可能不知道里面是 Sync Copy。这是 floating-window 与 ball 共用 ensure_on_screen 的副作用
- **`hideWindow()` 函数命名误导**：实际是 collapseToBall 不是真 hide；维护者改时易混
- **`lastExpandedSize` 是普通变量不是 $state**：rehydrate 后 stale；纯前端 reload 时丢失
- **expand 后的视口校正逻辑写在 `expandFromBall` 里 80 行**：与浮窗本身的 `ensure_on_screen`（在 Rust 后端）功能重叠 —— v2 应统一到一处
- **球形态下顶部状态栏 / 底部 footer / 历史列表 等都不渲染**：CSS 只 hide 容器；DOM 仍存在 + 数据 effect 仍跑——浪费但不致命
- **球形态用 `100vw 100vh` 充满窗口**：意味着窗口必须严格 48×48，否则 CSS 对不齐——隐式耦合 setSize 与 CSS
- **`startDragging()` 在某些 Win 屏幕缩放下会有 1-2px 跳动**：Tauri 已知问题，v0 没规避

### 5.3 v2 应继承
- 48×48 球尺寸 + 50% 圆角 + app icon 居中
- `collapsed: $state(false)` + `lastExpandedSize` 跨函数共享
- DEFAULT_EXPANDED = { w: 320, h: 420 }
- LogicalSize（区别于 outerSize 的物理像素）+ scaleFactor 换算
- 8 px 移动阈值 + 1500ms 时长上限的手势消歧
- expand 后视口校正（拉回监视器内）
- tauri.conf.json minWidth/minHeight = 40
- 球形态共用 main 窗口（不创建新窗口）

### 5.4 v2 应挑战
- **球形态视口校正与浮窗 ensure_on_screen 统一**：v2 在 ADR 决定是 Rust 后端兜底 + 前端不做，还是反过来；不允许双实现
- **球的状态持久化与否**：v0 不持久化（启动回展开态）；v2 是否记住"用户上次以球形态退出 → 启动时恢复为球形态"？属 UX
- **8 px / 1500ms 阈值**：是否暴露给设置或用 OS 提供的"双击容忍"参数？v0 硬编码
- **collapseToBall / expandFromBall 必须独立 Svelte 组件**（FloatingBall.svelte）—— 与 floating-window 第 5.4 节 同一原则
- **球形态下 DOM 节流**：列表 / footer / 状态栏的 effect 在 collapsed 时短路；省 effect tick 的 CPU
- **app icon 设计与浮窗 logo 一致**：v0 已对齐，v2 维持
- **拖动时窗口跟手延迟**：startDragging 是 Tauri 原生，无客制化空间；如有抖动只能上报上游

## 6. UX 设计

> 本节由 ux-designer 在 2026-05-06 填写。颜色/字号/字体/间距等视觉语言定义见 floating-window.md 第 6.5 节，本节不重复定义，仅描述球形态特有的交互和视觉规则。

### 6.1 信息架构

球形态是浮窗的极简收缩状态，自身展示的信息量：

1. App 图标 SVG（中心居中，标识"这是 Sync Copy"）
2. 仅此而已——无文字、无状态点、无 badge（见 6.6 边界与例外的决策理由）

信息优先级：能被识别为"Sync Copy 在运行"即达到目标。

### 6.2 关键流程图（文字版）

主路径（收缩 → 展开）：

1. 用户在 main view 点顶部 `−` 按钮 → `collapseToBall()` 记录当前尺寸 → 窗口瞬时缩为 48×48
2. 球停在原位（窗口中心不变）
3. 用户单击球（移动 ≤ 8px + 抬起 ≤ 1500ms）→ `expandFromBall()` → 窗口恢复上次记住的尺寸 → 视口校正

主路径（拖动球）：

1. 用户按下球 → 记录起始坐标
2. 移动 > 8px → 识别为拖动 → `startDragging()` 接管 → 窗口跟随鼠标
3. 松手 → 不展开，球停在新位置

异常路径：

- 球被拖到屏幕外：不主动干预；下次点击展开时 `expandFromBall` 做视口校正（详见 6.6）
- 托盘 hide 后 show：恢复球形态（不自动展开）
- 重启后：默认回展开态（球形态不持久化）

### 6.3 ASCII wireframe（必填）

球形态（48×48 logical px，圆形）：

```
      ╭──────╮
     /        \        ← border-radius: 50%
    |  [icon]  |       ← App icon SVG，约 28×28px，居中
     \        /        ← 同浮窗磨砂玻璃背景（rgba(28,28,32,0.88)）
      ╰──────╯         ← 1px 微高亮边（rgba(255,255,255,0.08)）
```

球在屏幕上的实际样子（示意位置在右下角）：

```
┌──────────────────────────────────────┐
│  桌面 / 其它应用                       │
│                                      │
│                                      │
│                                      │
│                              ╭────╮  │
│                             /icon  \ │
│                              ╰────╯  │
└──────────────────────────────────────┘
```

### 6.4 交互细节

点击区域：

- 整个 48×48 圆形区域：仅响应左键（`ev.button === 0`）
- 右键、中键：不响应（无右键菜单）
- 双击：等同于两次单击（第一次展开，第二次在展开后触发其它交互），不做特殊处理

手势消歧规则（直接继承 v0，已验证）：

- 按下后移动 ≤ 8px 且抬起 ≤ 1500ms → 视为单击 → 展开
- 按下后移动 > 8px → 视为拖动 → `startDragging()` 接管，松手不展开

鼠标悬停反馈：

- `cursor: grab`（球整体）
- 不做 hover 高亮（球本身无按钮语义，仅是可拖拽的图标）

拖动中反馈：

- `cursor: grabbing`（由 startDragging 控制，在原生窗口移动期间自动应用）
- 无其它视觉变化（拖动就是移动窗口）

收缩 → 展开切换动画：

- **瞬时切换**，不做 CSS transition 或缩放动画
- 理由：动画需要中间帧的窗口尺寸过渡，Tauri `setSize` 是原生调用，中间状态难以控制；瞬时切换比"卡顿动画"体验更好
- 这是一个刻意选择，不是实现缺陷

状态颜色：见 floating-window.md 第 6.5 节。

键盘可达性：

- 球形态下无焦点元素，不响应键盘（球是鼠标优先交互形态）

### 6.5 状态与颜色字典

详见 floating-window.md 第 6.5 节。球形态特有的视觉说明：

- 背景：与浮窗相同，`rgba(28, 28, 32, 0.88)` + `backdrop-filter: blur(20px)`（Mac）/ fallback（Win）
- 边框：`1px solid rgba(255, 255, 255, 0.08)`（与浮窗一致）
- 图标：App icon SVG，尺寸约 28×28px，居中，不带任何状态染色

球上**不显示**：

- 状态点（绿/蓝/灰/红）
- peer 数量 badge
- 未读审批 badge

决策理由：球的定位是"极简视觉锚点"，badge 会把一个 48px 小球塞满信息。用户想看状态只需单击展开浮窗——成本极低。v0 有过 peer 数 badge 的简短尝试，用户未明确要求保留。

### 6.6 边界与例外

- 球形态 = 0 个 peer 时：球的视觉无变化（不显示"0 台"badge），与有 peer 时完全相同
- 球被拖到屏幕外（> 50% 面积不可见）：不主动强制拉回；用户单击展开时 `expandFromBall` 做视口校正（拉回监视器内完全可见），这是合理的延迟恢复
- 多显示器场景：球可自由在任意显示器停留；展开时在球当前所在显示器内校正位置
- 重启后：回到展开态（默认 320×420 居中），不尝试恢复球形态（持久化复杂度不值）
- 高 DPI 屏（Retina / Win 150% 缩放）：48px 是逻辑像素，物理像素由系统 scaleFactor 处理，icon SVG 自动适配；8px 移动阈值是逻辑像素
- 球在 macOS 全屏 Space：同浮窗，alwaysOnTop 在全屏空间失效，属系统限制
- 实测可能暴露的问题：触摸板用户在精确点击与轻微滑动之间的边界感受可能与鼠标用户不同，8px 阈值可能需要在实测后微调

### 6.7 给前端工程师的实现提示（可选）

- CSS 中球的 `width: 100vw; height: 100vh; border-radius: 50%` 方案（v0）在窗口严格 48×48 时有效；如果窗口尺寸因平台原因有 1px 误差，需要用固定 `width: 48px; height: 48px` 在内部布局中兜底
- 手势消歧的 mousemove / mouseup 监听应注册在 `window`（而非球元素自身），确保鼠标快速划出球范围时仍然能接收事件

### 6.8 给主窗口和 PM 的反馈（UX 开放问题回应）

**问题 1**：球被拖到屏幕外或被其它窗口遮挡时的恢复。

结论：遮挡无法主动处理（球是系统级置顶窗口，alwaysOnTop 应保证它在普通窗口之上）。拖出屏幕外的恢复策略：延迟到用户下次展开时通过 `expandFromBall` 视口校正处理，不需要单独的"找回球"机制。从 UX 角度，用户拖出屏幕是主动行为，给一个托盘左键唤回的路径足够。

**问题 2**：球展开时窗口动画（淡入 / 滑动 / 直接出现）。

结论：直接出现（瞬时切换）。理由：Tauri `setSize` 是原生 API 调用，动画中间帧难以精确控制，强行做 CSS 动画会出现"大小跟不上窗口"的闪烁。等价地，从 48px 到 320px 的缩放感知只需约 80ms，用户几乎感知不到"消失 → 出现"的跳变。未来若 Tauri 官方提供 animated resize API，可以作为增量优化。

**问题 3**：球的数字 badge（peer 数）的颜色规则。

结论：v2 不显示 badge。球的定位是极简锚点，badge 在 48px 圆上视觉拥挤。peer 数字信息在展开后的浮窗顶部状态栏即可看到，且展开成本（单击）极低。如果实测阶段用户明确反馈"我需要在球上看到连接状态"，可以作为 v2.1 增量特性评估，届时颜色规则参照 floating-window.md 第 6.5 节的状态点颜色字典。

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 2 条] [P1 3 条] [P2 2 条]

- [P0] [架构师] 球的视口校正逻辑放后端（Rust ensure_on_screen）还是前端（expandFromBall 80 行）？v0 双实现耦合，v2 应集中
- [P0] [架构师] FloatingBall.svelte 独立组件的 props 边界（接收 collapsed: bool / 派发 expand 事件）；与 FloatingWindow 父组件的协议
- [P1] [UX] 8 px / 1500ms 手势阈值是否需要根据设备类型（触摸板 vs 鼠标）自适应？高 DPI 屏的 8 px 物理距离是否过短
- [P1] [UX] 球形态下托盘左键 hide → show 应恢复球形态还是自动展开？v0 保持球形态，可能让用户困惑
- [P1] [架构师] DOM 节流：collapsed 时是否短路 history-updated effect 等高频更新
- [P2] [UX] 球的形态持久化（启动恢复球形态 vs 总是默认展开态）
- [P2] [UX] 球上是否加状态徽章（v0 没加，用户感知缺失"连接状态"——但属浮窗职责，球极简还是给点信号）

## 8. Review 段（占位）

> code-reviewer / tech-architect / ux-designer 后续填写。本 feature 是浮窗的 UX 形态切换，UX 段必须由 ux-designer 完整填写后才能进入实现。
