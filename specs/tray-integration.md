---
status: SPEC_REVIEWED
owner: product-strategist
related_adrs: [ADR-003, ADR-010]
related_specs: [00-product-overview, floating-window]
created: 2026-05-06
updated: 2026-05-08
revised: 2026-05-08 — ADR-003 第 3.5 节 退出路径 P0 阶段允许 app.exit(0) + P2 升级到 quit_app（与 group-leave-notify / settings-panel 同步）
priority: P0
---

# tray-integration — macOS 菜单栏 / Windows 通知区图标与菜单

## 1. 问题（为什么做）

浮窗设计为"始终置顶但可隐藏"的轻量伴随工具。当用户点 × 隐藏浮窗后，必须有一个恒定可见的入口让用户重新唤出窗口、或在不打开窗口的情况下退出应用。系统托盘（Mac 顶部菜单栏 / Win 右下通知区）是桌面应用的标准做法，且 Tauri 2 内置 `tray-icon` feature 直接支持。本 feature 是 `floating-window` 的逻辑伴生：窗口 + 托盘构成 v2 的"最小可见 UI"。

## 2. 用户故事

- As a user who hid the floating window, I want a single click on the tray icon to bring it back, so that I am never stuck without a way to interact with the app.
- As a user, I want a right-click menu on the tray with "Show / Hide / Quit" entries, so that I can quit the app even when the window is hidden.

## 3. 范围

**in scope**：
- 应用启动时注册托盘图标（id `main-tray`），用 Tauri default window icon 作为图标，tooltip 显示 `Sync Copy`
- 左键单击托盘图标：切换浮窗显示 / 隐藏（已显示则 hide，已隐藏则 show + focus + `ensure_on_screen`）
- 右键托盘菜单三项：`显示浮窗` / `隐藏浮窗` / `退出`
- `显示浮窗` / `隐藏浮窗` 行为同左键 show/hide 的两态
- `退出` 走"主动 leave 广播 + 1.5s 等待 + exit(0)"路径（leave 广播由 `group-leave-notify` 在 P2 实现；P0 阶段 `退出` 简化为 `app.exit(0)` 直接退出，TODO 标记）
- 显示浮窗时 emit `window-shown` Tauri 事件，供前端组件做相应 refresh
- `show_menu_on_left_click(false)`（Mac/Win 一致，左键不弹菜单只切窗口）

**out of scope**：
- 托盘图标动态徽章（计数 / 状态色）——v2 不做，状态信息全部在浮窗内表达；徽章在 macOS 系统托盘是图片，复杂度不值得
- 未读计数提示音 / 系统通知（v2 沿 v0 不做）
- 不同状态下托盘图标变色（连接 = 绿、错误 = 红等）——v0 没做，v2 不做
- 自定义托盘菜单图标（菜单项仅文字）

## 4. 验收标准（Definition of Done）

- [ ] 应用启动后，macOS 顶部菜单栏 / Windows 右下通知区出现 `Sync Copy` 图标，hover 显示 tooltip `Sync Copy`
- [ ] 左键单击托盘图标，浮窗显示 / 隐藏可来回切换
- [ ] 右键托盘图标，弹出菜单包含 `显示浮窗` / `隐藏浮窗` / `退出` 三项
- [ ] 点击 `显示浮窗`：浮窗显示 + 获取焦点；若之前被拖到屏幕外，自动拉回当前显示器
- [ ] 点击 `隐藏浮窗`：浮窗 hide，但应用仍在后台运行
- [ ] 点击 `退出`：应用进程结束（exit code 0），托盘图标消失
- [ ] 浮窗 hide 时左键托盘图标，窗口能正确再次显示并被 focus（不会出现"窗口可见但无焦点"）

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的
`legacy-prototype` 分支 `src-tauri/src/lib.rs` 的 `build_tray` 函数：用 `MenuItem::with_id` 创建 show/hide/quit 三项菜单 → `TrayIconBuilder::with_id("main-tray").icon(default_window_icon).tooltip("Sync Copy").menu(&menu).show_menu_on_left_click(false)` → `on_menu_event` 处理三个 menu id → `on_tray_icon_event` 监听 `Click { button: MouseButton::Left, button_state: MouseButtonState::Up }` 切换窗口可见。`Cargo.toml` 的 `tauri = { version="2", features=["macos-private-api","tray-icon"] }`。

### 5.2 v0 暴露的具体坑
- 左键单击 v0 是按 `MouseButtonState::Up` 触发的（按下不算），跨平台行为一致；但在 Mac 部分版本上 `Up` 事件偶尔丢——v0 没做兜底
- `quit` 直接 `app.exit(0)`，**没有走 leave 广播**——这与从 ⚙ 设置面板"退出应用"按钮（v0 走 `quit_app` 命令含 leave）行为不一致，在多机场景下其它机器要等 10-20 秒心跳才发现你下线
- v0 的 quit 路径双重存在（设置面板按钮 vs 托盘菜单 vs 关窗口）容易让维护者误以为只要修一处
- 显示浮窗时 emit 的 `window-shown` 事件只在前端某些组件用了，部分组件（如悬浮球膨胀）依赖事件却没监听——容易遗漏
- 托盘图标用 `default_window_icon`，与浮窗内的 `app-icon.svg` 不是同一份设计（v0 有但视觉不一致；user 反馈不大但是细节）

### 5.3 v2 应继承
- Tauri 2 内置 `tray-icon` feature
- 左键 = show/hide 切换；右键 = 三项菜单
- `show_menu_on_left_click(false)`
- `tooltip("Sync Copy")` + `id("main-tray")`
- 显示时 emit `window-shown` 事件用于前端联动

### 5.4 v2 应挑战
- "退出" 路径必须**唯一**：托盘菜单 / ⚙ 设置面板 / 关窗口三处的退出逻辑由架构师在 ADR 中合并到一个 `quit_app` 命令，包含 leave 广播
- 是否引入"启动时直接 hide 到托盘"的开机自启场景？v0 总是显示浮窗，下一步若做开机自启需此选项

## 6. UX 段（占位）

> 待 ux-designer 在后续阶段填写。建议覆盖：
> - 托盘图标在 macOS 暗色 / 亮色菜单栏的可读性（v0 用彩色 default icon，部分主题下不够清晰）
> - Win 通知区图标可能被系统折叠到溢出区，是否提供"始终显示"指引

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 1 条] [P1 2 条] [P2 2 条]

- [P0] [架构师] 退出路径合并到唯一 `quit_app` 命令：本 spec P0 阶段允许暂时 `app.exit(0)`，但需在架构师 ADR 与 `group-leave-notify` 的 spec 之间约定接口，避免后期改动 break（与 `group-leave-notify` 第 7 节 / `settings-panel` 第 7 节 同议题）
- [P1] [架构师] 托盘图标资源是否使用专门为托盘设计的图标（macOS 推荐黑白 template image）？v0 复用 default_window_icon 不规范但能跑——是否在 P0 阶段就矫正？
- [P1] [架构师] `window-shown` 事件的消费方未来可能很多（悬浮球展开 / 历史 refresh / 状态拉取），是否升级为统一的"窗口生命周期 store"（Svelte rune store）以避免散点监听？
- [P2] [UX] macOS 上托盘左键 vs 右键的预期：Mac 用户更习惯左键直接弹菜单；v0 选了"左键切窗口"——是否仍保留？
- [P2] [UX] Win 系统托盘默认折叠到溢出区，需文档指引用户"将 Sync Copy 拖到通知区一直显示"

## 8. Review 段（占位）

> code-reviewer / tech-architect 后续填写。
