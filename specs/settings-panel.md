---
status: SPEC_REVIEWED
owner: product-strategist
related_adrs: [ADR-003, ADR-010]
related_specs: [00-product-overview, floating-window, local-ip-display, group-leave-notify]
created: 2026-05-06
updated: 2026-05-08
revised: 2026-05-08 — ADR-003 第 3.5 节 quit_app 唯一退出路径 + 第 3.6 节 诊断模式 tracing reload handle (写 Config 持久化)
priority: P1
---

# settings-panel — 设备名编辑 / 清除历史 / 退出应用三件套设置面板

## 1. 问题（为什么做）

Sync Copy 的设计哲学是"零配置上手"——首次启动用 hostname 自动作 device_name，端口默认 5858——但用户**总有一天**会需要改：默认 hostname 太长不雅、想给笔记本起个家庭称呼、想知道"清除历史"在哪、想"完全退出"而不只是托盘里挂着。本 feature 提供一个**唯一**的设置面板，承载这三件事，避免让用户去找文件 / 改命令行 / 重启应用。

面板是浮窗内的 view 切换（不是独立窗口）—— 与 join 对话框、main 历史列表共用同一容器，view 字段在 `"main" | "settings" | "join"` 三态间切换。退出应用必须**同步走 leave 广播**（让组内其它机器立刻看到"少了一台"），不能像 v0 托盘退出那样草率（00 总览 第 5.4 节 教训）。

## 2. 用户故事

- As a user with a hostname like `Taos-MacBook-Pro-2.local`, I want to set a friendly device name like `工作 Mac`, so that other devices in the group see something readable instead of a system identifier.
- As a user with sensitive content in history, I want a "Clear all history" button that wipes my own + broadcasts to all peers, so that one click cleans every device in the group.
- As a user, I want a "Quit application" button that broadcasts a leave notice + exits cleanly, so that other devices show "1 台" instantly instead of waiting 20s for heartbeat to detect me missing.

## 3. 范围

**in scope**：
- 设置面板入口：浮窗顶部状态栏右侧 ⚙ 图标按钮（main view 时可见，settings view 时隐藏）
- 面板内容（垂直 stack）：
  - **本机设备名**：`<input type="text" bind:value={form.device_name} />` — 限长 ≤ 64 字符、过滤控制字符（与 `group-discovery` 第 7 节 安全风险呼应）
  - **divider 分割线**
  - **清除历史按钮**：ghost 样式，禁用状态当 history.length === 0；点击 → 调 `clear_history` 命令 → 本机历史清空 + emit history-updated + 异步 `broadcast_clear_history` 给所有 peer + 关回 main view
  - **退出应用按钮**：danger 样式（红色）；点击 → 调 `quit_app` 命令 → 后端走 leave 广播 + 1.5s 超时 + 清状态 + `app.exit(0)`
- 顶部 × 关闭按钮（取消未保存的 device_name 修改）：放弃 input 中的脏值，回 main view
- **保存策略**：input 失焦或 view 切回 main 时调 `set_config`（`port` 沿用旧值 + 新 device_name）→ 后端 `Config::save` 写盘到 `~/Library/Application Support/com.synccopy.app/config.json`（mac）或 `%APPDATA%\com.synccopy.app\config\config.json`（win）+ 自动启服务端（如未启动）
- 端口字段**P1 阶段不开放修改**（v0 在设置面板暴露过 port input，但实际改端口要重启监听 + 重新加入小组，体验差）；端口仅由底部 IP 栏展示（属 `local-ip-display`），改端口暂留 P2 视用户反馈
- device_id 不展示给用户（系统级识别符，UI 暴露反而引起混淆）

**out of scope**：
- 端口修改 UI（P1 不做；用户改端口需直接编辑 config.json 重启 —— 罕见场景）
- 信任名单管理（"我同意过哪些设备"列表 + 撤销）—— 信任非持久（00 总览 第 3 节 锁定），重启即清，故无需 UI
- 关于 / 版本 / 检查更新（属 release 流程，不是设置面板职责）
- 主题切换（macOS 跟随系统暗色，Win 暂只暗色）
- 启动开机自启（涉系统服务注册，留 v3）
- 落盘目录配置（属 `file-transfer-drag` 第 7 节 已列开放问题）
- 心跳间隔 / 审批超时等高级参数（00 总览 第 5.4 节 已留作架构师 ADR 论证）

## 4. 验收标准（Definition of Done）

- [ ] 浮窗 main view 状态栏右侧 ⚙ 按钮可见；点击切到 settings view + ⚙ 隐藏 + × 显现
- [ ] 在设备名 input 输入新值 `工作 Mac` → 切回 main view → 浮窗底部 device 区域立刻显示 `工作 Mac` + B 端历史中后续来自本机的条目 source 显示 `来自 工作 Mac`
- [ ] 设备名留空时 `set_config` 应拒绝（保留旧值，UI 显示 banner `设备名不能为空`）
- [ ] 设备名 > 64 字符时 input 截断或 banner 提示
- [ ] 清除历史按钮在 history.length === 0 时灰化禁用；非空时点击 → 本机历史清空 + B 端历史 1 秒内同步清空
- [ ] 退出应用按钮点击 → A 端 1.5 秒内进程结束 + B 端浮窗状态从 `小组 · 2 台` 变为 `小组 · 1 台`（无需等心跳）
- [ ] × 取消按钮不保存 input 当前值；下次进入 settings 显示原 device_name
- [ ] 配置文件保存后立即可见：杀进程重启应用，device_name 仍为新值

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的
`legacy-prototype` 分支 `src-tauri/src/commands.rs::get_config -> ConfigView { port, device_name, peer_hint, device_id }`、`set_config(state, app, update: ConfigUpdate { port, device_name }) -> Result<ConfigView, String>` 写入 `state.config` + `Config::save()` 到 `directories::ProjectDirs::from("com", "synccopy", "app").config_dir()` + 自动起服务端（如未起）。`Config::default()` 用 `hostname()` 函数（先看 `HOSTNAME` / `COMPUTERNAME` env，再 fallback `hostname` 命令）作 device_name + 随机 UUID 作 device_id。`commands.rs::quit_app` async 命令：
1. `tokio::time::timeout(1500ms, broadcast_leave(state))` 给所有 peer 发 `/peers/leave`
2. clear peers / peer_keys / approved / banned / forwarded_approvals
3. 关 server（oneshot send 给 server_shutdown）
4. sleep 200ms 让网络任务收尾
5. `app.exit(0)`

`commands.rs::clear_history`：清本机 + emit history-updated + 异步 `broadcast_clear_history`。前端 `+page.svelte` settings view（约 725-740 行）模板：input bind device_name + divider + 清除历史按钮（disabled when empty）+ danger 退出按钮。`closeSettings` 函数把 view 改回 `"main"`。`form` 是 `$state` 拷贝自 get_config 结果；保存逻辑在 `closeSettings` 时调 `set_config`。

### 5.2 v0 暴露的具体坑
- **设备名无校验**：可输空、可输 1000 字符、可输控制字符 / Unicode 反向覆盖字符 → 对端弹审批框时显示乱七八糟（与 `group-discovery` 第 7 节 安全开放问题呼应）
- **port input 在 v0 设置里暴露但不功能**：改 port 需要重启 server，v0 没有 reload 逻辑；用户改了点保存看 UI 没反应。v2 移除该字段
- **三处退出路径不一致**：托盘菜单 `退出` 调 `app.exit(0)` 不发 leave；设置面板 `退出应用` 调 `quit_app` 发 leave；浮窗 × 是 hide 不退。维护者改一处易遗漏其它（00 总览 第 5.4 节 + tray-integration 第 5.4 节 已点名 v2 必须**唯一**退出路径）
- **`peer_hint` 字段持久化但 UI 不暴露管理**：用户切小组时旧 peer_hint 仍是默认值 → join 对话框 placeholder 用旧地址易误导
- **device_id 也持久化但用户看不到**：`get_config` 返回 device_id 给前端但前端从不渲染——克隆磁盘冲突时用户**无法**自查"我和那台机器是不是同个 device_id"
- **保存策略隐式 = 切回 main 时**：用户在 settings 里改了文字直接关浮窗（红 ×）→ 修改丢失；v0 没用户投诉但是隐式行为
- **`Config::save()` 同步阻塞调用**：写盘 200ms 量级，主线程阻塞；用户感知不强但不洁

### 5.3 v2 应继承
- 设置面板是**view 切换**而非独立窗口（与 floating-window 单 main 窗口约束一致）
- 设备名 / 清除历史 / 退出三件套
- `get_config` / `set_config` Tauri 命令 + `Config::save` 写 ProjectDirs 的 config.json
- `quit_app` 命令含 leave 广播 + 1.5s 超时 + 清状态 + exit
- `clear_history` 命令含 broadcast_clear_history
- 清除历史按钮在 history 空时禁用
- ⚙ 图标入口 + × 关闭返回 main view
- device_name 默认值 = hostname

### 5.4 v2 应挑战
- **退出路径合并到唯一 `quit_app`**：托盘菜单 / 设置按钮 / OS 关闭信号都走 `quit_app`（00 总览 第 5.4 节 / tray-integration 第 5.4 节 / 本 spec 共同要求；属架构师 ADR）
- **device_name 校验**：长度 ≤ 64 + 过滤控制字符 + 过滤 Unicode 反向覆盖字符 + 空串拒绝（与 `group-discovery` 共建安全防线）
- **保存策略明确化**：input 失焦保存 vs 显式保存按钮 vs 切回 main 自动保存——属 UX
- **port 字段从 settings 移除**：UI 仅展示在底部 IP 栏（属 `local-ip-display`）；P1 不允许改 port
- **device_id 是否暴露给用户作"诊断"用**：克隆磁盘冲突场景的 self-help（"复制此 ID 给支持"或 "重新生成 device_id" 按钮）。属 UX + 安全 共商
- **Config::save async 化**：`tokio::fs::write` 替代 `fs::write` 避免主线程阻塞
- **关于 / 版本号显示**：可在面板底部加只读一行（来自 package.json version）—— 属 docs-writer 后续 release 流程关心

## 6. UX 设计

> 本节由 ux-designer 在 2026-05-06 填写。颜色/字号/字体/间距等视觉语言定义见 floating-window.md 第 6.5 节，本节不重复定义。

### 6.1 信息架构

设置面板是浮窗的次级 view，与 main view 共用同一容器（view 切换，不是新窗口）。本面板展示的信息按重要性：

1. 设备名输入框（核心，用户最常需要改的项）
2. 清除历史按钮（危险操作但有可逆性——数据会在下次同步重新出现，故风险中等）
3. 退出应用按钮（危险操作且不可逆——进程退出，其它设备立刻感知）
4. 版本号（只读，辅助诊断）

### 6.2 关键流程图（文字版）

主路径（进入设置 → 改名 → 退出）：

1. 用户在 main view 点 ⚙ 按钮 → 切换到 settings view（⚙ 隐藏，× 显现）
2. 用户修改设备名 input → 失焦时调 `set_config` 保存（自动保存，无需点"保存"按钮）
3. 用户点 × 取消 → 若有**未保存的脏值**（input 聚焦中从未失焦）则放弃，回 main view
4. 或：用户直接点 × → 切回 main view（已失焦的改动已自动保存）

主路径（清除历史）：

1. 用户点"清除历史"（ghost 样式，history 非空时可用）→ 出现内联二次确认条（见 6.3）
2. 用户确认 → 调 `clear_history` → 本机历史清空 + 广播 + 切回 main view + banner "历史已清除"
3. 用户点"取消" → 确认条消失，无操作

主路径（退出应用）：

1. 用户点"退出应用"（danger red）→ 直接调 `quit_app`（无二次确认，理由见 6.4）
2. 后端广播 leave + 1.5s 超时 + exit(0)

异常路径：

- 设备名留空失焦：`set_config` 拒绝，input 下方显示"设备名不能为空"红字提示，保留旧值
- 设备名 > 64 字符：input 层截断（`maxlength="64"`），不允许输入超长

### 6.3 ASCII wireframe（必填）

设置面板（复用 320×420 容器，替换中央区域）：

```
┌────────────────────────────────┐
│  [⚙ 设置]                   × │← 顶部，× 替换 ⚙，drag-region
├────────────────────────────────┤
│                                │
│  本机设备名                     │← 12px #9ca3af label
│  ┌──────────────────────────┐  │
│  │ 工作 Mac                  │  │← input，13px #f3f4f6
│  └──────────────────────────┘  │← 1px border rgba(255,255,255,0.2)
│  [设备名不能为空]               │← 错误时显示，11px #ef4444，默认隐藏
│                                │
│  ──────────────────────────── │← 1px 分割线 rgba(255,255,255,0.08)
│                                │
│  [清除历史]                     │← ghost 按钮；history 空时 disabled
│                                │  清除历史点击后展开内联确认：
│    ┌──────────────────────┐    │
│    │ 将清空所有设备的历史   │    │← 11px #9ca3af 提示文字
│    │ [取消]    [确认清除]   │    │← ghost + danger red
│    └──────────────────────┘    │
│                                │
│  ──────────────────────────── │
│                                │
│  [退出应用]                    │← danger red 按钮，全宽
│                                │
│                                │
│                                │
├────────────────────────────────┤
│  v2.0.0                        │← 11px #9ca3af，右对齐或居中，只读
└────────────────────────────────┘
```

input 聚焦状态：

```
│  ┌──────────────────────────┐  │
│  │ 工作 Mac              |  │  │← 光标 | + border 变为 rgba(59,130,246,0.6) 蓝色
│  └──────────────────────────┘  │
```

清除历史的内联确认展开后（替换按钮区域，不是弹框）：

```
│  ┌────────────────────────────┐ │
│  │ 将同步清空所有设备的历史    │ │← 11px 提示，#9ca3af
│  │ 无法恢复                   │ │
│  │  [取消]        [确认清除]  │ │← ghost + danger red
│  └────────────────────────────┘ │
```

### 6.4 交互细节

点击区域：

- × 关闭按钮：点击放弃未失焦的脏值，切回 main view
- 设备名 input：文字输入框，聚焦可编辑；失焦自动保存
- 清除历史按钮：ghost 样式；history 空时 disabled（opacity 降低，`cursor: not-allowed`）
- 清除历史确认条的"取消"：ghost 样式，点击折叠确认条
- 清除历史确认条的"确认清除"：danger red，点击执行操作
- 退出应用按钮：danger red，全宽，直接执行（无二次确认，理由见下）
- 版本号：只展示，不可交互

鼠标悬停反馈：

- 清除历史按钮（可用时）：ghost 背景稍亮
- 退出应用按钮：brightness 稍提亮（red 不变）
- disabled 状态按钮：不响应 hover

保存策略（关键决策）：

- 选择**失焦自动保存**（input blur 事件触发 `set_config`），而非"显式保存按钮"
- 理由：设置项少（仅设备名），失焦即保存符合 macOS 系统应用的习惯；避免用户改完忘点保存
- × 按钮的语义：退出 settings view，若 input 当前聚焦中（还未触发 blur）则放弃该脏值；若已失焦（已自动保存）则 × 只是切回 main view，不撤销

退出按钮的确认机制（关键决策）：

- 不做二次确认弹框
- 理由：退出是用户主动寻找的按钮（在设置面板深处），误点概率极低；danger red 样式已经是视觉警示；用户若只想隐藏浮窗会用 ×（hide）而不是进设置再退出
- 实测可能调整：如果多个用户报告误退，可在 v2.1 评估添加确认

清除历史的内联确认：

- 选择内联展开方式（展开一个说明 + 双按钮区域），而非弹框
- 理由：在小浮窗内嵌套弹框视觉层级混乱；内联确认信息明确（"将同步清空所有设备的历史，无法恢复"），足以让用户理解后果

设备 ID 不展示：

- 不向用户展示 device_id（UUID 字符串）
- 理由：对普通用户无意义；克隆磁盘冲突场景属支持/诊断场景，应提供专门路径（如 ADR 决策的诊断 CLI 或未来版本的"关于"页面），不污染主设置面板

### 6.5 状态与颜色字典

详见 floating-window.md 第 6.5 节。settings view 特有说明：

| 元素 | 颜色 | 说明 |
|---|---|---|
| input border（常态） | `rgba(255,255,255,0.20)` | 比窗口边框略亮 |
| input border（聚焦） | `rgba(59,130,246,0.60)` | primary blue 半透明 |
| 设备名 label | `#9ca3af` | 12px 次要色 |
| 错误提示文字 | `#ef4444` | 11px danger red |
| 分割线 | `rgba(255,255,255,0.08)` | 与窗口边框同色 |
| 版本号文字 | `#9ca3af` | 11px hint 级别 |
| disabled 按钮 overlay | opacity 0.4 降低 | 在现有 ghost 样式上叠加 |

### 6.6 边界与例外

- 设备名 = 空字符串：input blur 时拒绝保存，显示"设备名不能为空"；显示旧值（input value 重置为旧值）
- 设备名 = 纯空白（全是空格）：同空字符串处理，trim 后为空则拒绝
- 设备名 > 64 字符：input maxlength 截断，不允许输入超过 64 字符
- 历史 = 0 条时："清除历史"按钮 disabled，不展开确认条
- 设置面板内点击退出应用后，程序退出快于 UI 更新：属正常，进程退出即结束 UI
- 版本号来源：build 时从 `package.json` 注入，不做运行时读取；若无则不显示该行
- 实测可能暴露的问题：失焦自动保存在用户快速切换 view（未等 blur 事件触发）时可能丢失修改；如果实测发现此问题需在 × 按钮的 click handler 里主动触发 input blur

### 6.7 给前端工程师的实现提示（可选）

- 清除历史的内联确认建议用 `$state` 变量控制展开/折叠，展开时替换按钮区域，不要用 CSS max-height 动画（复杂且在浮窗内不值）
- input 的 blur 事件保存逻辑要注意：点"确认清除"时焦点可能从 input 转移到按钮，会意外触发一次 set_config，这是正常行为，确保幂等

### 6.8 给主窗口和 PM 的反馈（UX 开放问题回应）

**问题 1**：设备名输入即保存 vs 显式保存按钮。

结论：选失焦自动保存。设置项少，保存后无副作用（不需要重启），失焦保存最自然、最少操作。显式保存按钮会让用户产生"我改了要记得点"的心智负担，与浮窗"轻度伴随"的定位不符。

**问题 2**：退出应用按钮的危险提示样式。

结论：danger red 全宽按钮 + 深层位置（设置面板内）构成足够的摩擦。不做二次确认弹框。如实测发现误退率高，v2.1 可添加内联确认（与清除历史同样的模式）。

**问题 3**：设备 ID 是否暴露给用户。

结论：不暴露。设备 ID 是系统级标识符，对普通用户无意义。克隆磁盘冲突场景（v0 返回 409）的自助诊断路径留给 v2.1 的"关于"页面或 ADR 明确的 CLI 工具，不污染设置面板。架构师在 ADR 中应记录"v2 P1 不暴露 device_id 给 UI，克隆冲突的用户路径留待后续版本"。

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 2 条] [P1 4 条] [P2 2 条]

- [P0] [架构师] 退出路径合并：托盘菜单 / 设置按钮 / OS 关闭信号 / × 关浮窗按钮的语义 must 在 ADR 明文：哪些走 `quit_app`（含 leave）哪些走 hide（与 `group-leave-notify` 第 7 节 / `tray-integration` 第 7 节 同议题）
- [P0] [安全] device_name 校验规则（≤ 64、控制字符、Unicode 反向覆盖字符）必须前后端双层校验；属安全审阅（与 `group-discovery` 第 7 节 / `group-approval` 第 7 节 / `group-trust-gossip` 第 7 节 同议题）
- [P1] [UX] 清除历史的二次确认：v0 无确认 + 跨机生效，是否风险过高
- [P1] [UX] 保存策略：失焦保存 vs 显式按钮 vs 切回 main 自动保存
- [P1] [架构师] `Config::save` 同步阻塞 vs `tokio::fs::write` 异步——是否在 P1 就上
- [P1] [UX] 退出按钮 vs 隐藏按钮的视觉权重（避免误退）
- [P2] [架构师] device_id 是否在 settings 暴露用于诊断？克隆磁盘冲突时用户无可见路径
- [P2] [架构师] port 字段 v2 P1 完全不允许改？某些用户在端口冲突时只能改 config.json 手动；UX 是否需要错误提示引导

## 8. Review 段（占位）

> code-reviewer / tech-architect / ux-designer / security-reviewer 后续填写。device_name 校验涉及对端弹框显示，需 security-reviewer ACK。
