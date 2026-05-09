---
status: SPEC_REVIEWED
owner: product-strategist
related_adrs: [ADR-003, ADR-010]
related_specs: [00-product-overview]
created: 2026-05-06
updated: 2026-05-08
revised: 2026-05-08 — ADR-003 第 3.6 节 锁定 tracing-appender rolling file + non_blocking guard + Reload Handle (诊断模式开关) + std::panic::set_hook v4-7 三件套；轮转 crate 选型留 implementer ADR
priority: P1
---

# diagnostic-logging — 持久化诊断日志 + 一键导出（事后取证）

## 1. 问题（为什么做）

用户连续使用 v0 几天后**反复遇到偶发性问题**——"有时不灵敏"（剪切板同步延迟数秒到数十秒）、"失联"（peers 列表里设备数变成 1 但实际三台还都在）、突然不响应等。这类问题的共同特征：**无法稳定复现 + 无法在出问题时实时调试 + 等想起来时已经过去了几小时**。

v0 的现状（`legacy-prototype` 分支 `src-tauri/src/lib.rs::run`）是 `tracing_subscriber::fmt().with_env_filter(...).try_init()`——日志只输出到进程 stderr。在 release build（`tauri build`）下根本没有可见的 stderr 终端，用户实际看不到任何日志；在 dev build（`tauri dev`）下 stderr 滚动出去也无人留存。结果是用户遇到 bug 后**0 证据可发**，开发者只能猜。

本 feature 的目标：把 `tracing` 的输出**持久化到磁盘**（OS 标准日志目录 + 自动轮转），在浮窗设置面板提供"导出日志"按钮，让用户在遇到问题后**把最近一段日志打成 zip 发给开发者**，事后取证 + 还原现场。这是把 v2 推上"用户敢长时间用"的关键支撑——00 总览 第 4 节 项目级验收 #2（三机集成场景全过）和"v0 中已能跑通的所有用户场景在 v2 上至少同样能跑通"之间的工程信心来源。

本 feature 是横切关注（cross-cutting）：所有其它 feature 的实现都会调 `tracing::info!/warn!/error!`，本 feature 仅负责"日志去哪、如何轮转、如何让用户拿到"。

## 2. 用户故事

- As a user with intermittent sync issues, I want the app to keep a rolling log of the last several days on disk, so that when I notice a problem I can hand over evidence even if it happened hours earlier.
- As a user, I want a single "Export logs" button in settings that produces a zip I can attach to an email or chat message, so that I do not need to know where logs live or how to find them.
- As a developer receiving a user's log zip, I want the log lines to include timestamps, severity, and component (clipboard / network / approval / crypto), so that I can locate the failing path without asking follow-ups.
- As a privacy-conscious user, I do not want my actual clipboard contents (passwords, code snippets) to appear in logs, so that "send the logs" never becomes "leak my secrets".
- As a user troubleshooting a specific bug with the developer, I want a temporary "diagnostic mode" toggle that elevates log verbosity to debug for the next session, so that I can capture extra detail without permanently bloating the log files.

## 3. 范围

**in scope**：

- **持久化日志文件**：`tracing` 的所有输出同时写到 stderr（dev 习惯不丢）+ 磁盘文件（release 唯一可见来源）
- **日志位置**（OS 标准目录，由 `directories` crate 解析）：
  - macOS：`~/Library/Logs/com.synccopy.app/sync-copy.log`（+ 轮转产物）
  - Windows：`%LOCALAPPDATA%\com.synccopy.app\logs\sync-copy.log`
  - Linux（cfg 隔离，仅本地 dev 用）：`~/.local/share/com.synccopy.app/logs/sync-copy.log`
- **轮转策略**：按"日 + 大小"双触发，单文件 ≤ 10 MB，超过即开新文件；保留最近 7 天或最近 10 个文件中较大者；日志总占用 ≤ 100 MB（具体实现选型属架构师 ADR，见 第 7 节 [P0] [架构师]）
- **日志级别**：默认 `info`（本应用代码）+ `warn`（依赖库），允许通过两条路径调整：
  - 进程启动前的 `RUST_LOG` 环境变量（继承 v0 行为，开发者用）
  - 运行时设置面板"诊断模式"开关：开启后立即把本应用 filter 升到 `debug`，关闭后回到 `info`；开关状态写入 `Config`（设置面板 spec 的 `settings-panel`）+ 日志记录"诊断模式开/关"事件
- **必须记录的事件类别**（具体字段属架构师 ADR，spec 仅约束"哪些事件不能漏"）：
  - 启动 / 退出 / 监听端口绑定结果
  - 握手发起 / 收到 / 审批弹出 / 决定 / dismiss / timeout（来自 `group-discovery` + `group-approval`）
  - 剪切板事件触发（本地复制广播尝试 / 接收远端 / 解密失败 / suppress 写入）—— 仅记录元数据，**不记录明文内容**（见 安全约束）
  - 心跳 ping / 失败 / 剔除（来自 `peer-heartbeat`）
  - peer leave 主动广播 / 接收（来自 `group-leave-notify`）
  - trust / ban gossip 接收与本地表更新（来自 `group-trust-gossip`）
  - 文件传输请求 / 接受 / 拒绝 / 保存路径（来自 `file-transfer-drag`）
  - 网络请求失败（reqwest 错误、HTTP 非 2xx 响应、超时）
  - panic / 关键 unwrap 失败（捕获后写日志再退出，不留盲区）
- **绝不记录的内容**（敏感字段黑名单）：
  - 剪切板明文（text 内容、image 字节、file 字节）
  - AES 密钥、X25519 私钥、shared secret
  - HKDF 派生中间值
  - Config 里的 `device_id` 是否记？device_name 是否记？peer IP 是否记？—— 待 第 7 节 [P0] [安全] 决议
- **导出按钮**（UI 在 `settings-panel` 中追加一个"导出日志"按钮）：
  - 点击后后端把当前日志目录下所有日志文件打包成 `sync-copy-logs-<YYYYMMDD-HHMMSS>.zip`
  - 通过系统 file save dialog 让用户选保存位置（默认 Downloads）
  - 导出本身记一条日志（含触发时间、生成的 zip 路径、文件大小）
  - 导出成功后浮窗显示 "已导出到 <路径>" 反馈（≤ 3s 自消失）
- **诊断模式**：
  - 设置面板有"诊断模式（更详细日志）"开关
  - 开启后立即生效（不需要重启），设置 filter = `debug`
  - 状态写 Config，重启后保留（让用户在重现 bug 期间不用每次重开）
  - 在浮窗顶部状态栏右侧显示一个小的 `DBG` 角标（开启状态下，提醒用户日志量在膨胀），具体视觉留 UX
- **错误兜底**：日志目录不可写（权限问题）/ 磁盘满 → 应用不崩，降级到仅 stderr + 在浮窗状态栏显示"日志写入失败"小提示
- **横切引用**：本 spec 不重新定义其它 feature 应记录的事件细节，仅在 第 3 节 in scope 列出"事件类别"，每个 feature 在自己 spec / ADR 阶段补充"我应该 log 什么字段"

**out of scope**（v2 这个 feature 不做）：

- 远程日志上传（违反 00 总览 第 1 节 "内容不出局域网" 与 第 3 节 out of scope "不做云同步"）
- 应用内实时日志查看 UI（用户用 Console.app / Notepad / 解压 zip 即可看；开发应用内查看器需要 UI 工程量但价值有限）
- 日志加密（日志在用户本机，威胁模型是"用户主动导出发开发者"——不做加密；如有敏感字段就直接不记录）
- 日志结构化为 JSON 给机器消费（v2 用人类可读 text；JSON 留 v3 评估）
- 自动崩溃报告 / 自动上传 stack trace（同"远程上传"原因 out）
- 把 v0 已有的命令行 `RUST_LOG=...` 环境变量改名（继承）

## 4. 验收标准（Definition of Done）

- [ ] 在 release build 安装后，启动应用 → macOS 上 `~/Library/Logs/com.synccopy.app/sync-copy.log` 文件存在且当前会话事件在写入；Windows 上 `%LOCALAPPDATA%\com.synccopy.app\logs\sync-copy.log` 同
- [ ] 应用启动后跑 30 分钟（含 1 次握手 + 5 次复制 + 1 次 leave）然后退出 → 重新启动 → 日志文件**仍然存在**且包含上一会话的事件
- [ ] 单个日志文件超过大小阈值后自动开新文件，旧文件保留；目录下日志文件总大小 ≤ 100 MB（在持续 7 天后或 N 次轮转后采样验证）
- [ ] 设置面板"导出日志"按钮 → 选择保存位置后产出有效 zip 文件 → 解压后含至少 1 个 `.log` 文件，文件内是 UTF-8 文本可读
- [ ] 在 A 上复制一段文本 "secret-password-123" → 检查日志文件 → **不应**出现 "secret-password-123" 子串（明文绝不入日志的硬约束）
- [ ] 设置面板"诊断模式"开关 → 切到开启 → 后续 1 分钟操作期间日志多出至少一种 `DEBUG` 级别条目（说明 filter 实际切到 debug） → 关闭后 1 分钟操作不再产生 `DEBUG` 条目
- [ ] 诊断模式状态在应用重启后保留（开启 → 关 → 开启 → 重启 → 仍开启）
- [ ] 日志目录被设为只读 / 磁盘满模拟 → 应用启动后不崩 + 浮窗某处显示 "日志写入失败" 一次性提示 + 仍能正常完成握手与剪切板同步（业务功能不被日志拖垮）
- [ ] 日志条目格式包含：时间戳（含日期 + 时区）、级别、模块名（如 `network::server` / `clipboard` / `crypto`）、消息体；可在文本编辑器里用时间戳 grep 出某段时间窗口内全部活动
- [ ] 在 v2 实现的所有 P0 feature（cross-platform-build / floating-window / tray-integration / group-discovery / group-approval / e2e-encryption / clipboard-text-sync / local-ip-display）至少各有 1 条 info 级别日志在正常路径上可观测，且至少 1 条 warn 或 error 级别日志在失败路径上可观测

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的

`legacy-prototype` 分支 `src-tauri/src/lib.rs::run`（前 8 行）：

```
tracing_subscriber::fmt()
    .with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,sync_copy_lib=debug")),
    )
    .with_target(false)
    .try_init()
    .ok();
```

依赖（`Cargo.toml`）：`tracing = "0.1"` + `tracing-subscriber = { version = "0.3", features = ["env-filter"] }`，**没有 file appender、没有轮转、没有任何持久化**。

各模块已经普遍使用 `tracing::info!` / `tracing::warn!` / `tracing::error!` 记录关键路径——例如 `network/server.rs::handle_handshake` 里有 5 处 info、3 处 warn 覆盖审批流程；`network/server.rs::handle_clipboard` 在 decrypt 失败时 warn；`clipboard.rs` 在每秒 poll 不打印（避免噪声）但在 set 时打印；`network/health.rs` 心跳失败打印 warn。**事件类别覆盖率本身是充分的**——问题仅在"输出去哪"。

### 5.2 v0 暴露的具体坑

- **release build 下 stderr 丢失**：用户双击应用后没有终端，`tracing::info!` 全部进入虚空。这就是当前用户报"我也不知道哪里出问题了"的根本原因
- **dev build 下 stderr 不留存**：开发者本地能看见，但用户场景重现不了，且 stderr buffer 滚出后追溯不能
- **没有日志级别运行时切换**：唯一调整是 `RUST_LOG=debug npm run tauri dev` 启动时定，对 release 用户不可达
- **没有事件分类约定**：不同模块 log 风格不一致——有的写 `peer = %name`（kv 字段）有的直接 `format!`；缺少统一的"组件名 + 事件名"约定让事后 grep 困难
- **没有任何"何时开始写日志"的事件**：进程启动那一刻没有"version: ... commit: ... os: ...""banner"行，事后看日志时无法判断是哪个 build 在跑
- **panic / unwrap 路径无日志**：v0 多处 `.unwrap()` / `.expect("...")` 在失败时让进程死掉，没有日志写入；用户看到的就是"应用突然没了"
- **日志没有任何敏感字段过滤约定**：v0 的 `tracing::info!` 调用点全靠开发者自觉不打印明文；v2 必须把"不可打印的字段"作为 spec 级硬约束（见 第 4 节 验收 #5）
- **设置面板与日志无连接**：v0 设置面板 0 入口让用户 reach 日志（既不能切级别也不能导出）

### 5.3 v2 应继承

- `tracing` + `tracing-subscriber` 技术栈（成熟、零运行成本 disabled、已普遍 instrument 各模块）
- `EnvFilter` 默认 = `info,sync_copy_lib=debug`（开发体验保留）
- 各模块已有的 `info!/warn!/error!` 调用点（事件类别覆盖率已经够）
- `with_target(false)` 的简洁格式（target 字段对用户不重要）
- 通过 `RUST_LOG` 环境变量临时调级别（开发者用）

### 5.4 v2 应挑战

- **加 file appender + 轮转**：用 `tracing-appender` 还是自写 file rotation？跨平台路径如何稳定（macOS Logs 目录 vs Windows LocalAppData）？—— 见 第 7 节 [P0] [架构师]
- **加运行时级别切换**：诊断模式开关是用 `tracing-subscriber::reload::Handle` 热重载还是其它机制？—— 见 第 7 节 [P0] [架构师]
- **加敏感字段过滤的 spec 级约束**：明文剪切板 / 密钥永不入日志这条规则必须在 ADR 里以"任何 `tracing::*!` 调用点 review 时必须经过敏感字段 checklist"的形式落地（00 总览 第 5.2.1 节 "零文档化的隐式不变式" 反例）
- **加启动 banner**：每次进程启动的第一条日志记录 version / commit hash / OS / Tauri version，事后看日志知道是哪个 build
- **加 panic hook 写日志**：`std::panic::set_hook` 在 panic 时把 message + backtrace 写日志再 abort，避免"应用突然没了"无证据
- **加导出 UI 入口**：`settings-panel` spec 的 第 3 节 必须把"导出日志"按钮纳入；本 spec 仅约定 button 存在 + Tauri 命令签名（实际 UI 视觉留 `settings-panel` UX 段）
- **横切日志事件 schema 约定**：v0 各模块 log 风格不一，v2 应在 ADR 里约定一个最小 schema（如必须含 `component`、推荐含 `event`、`peer_id`、`request_id` 等结构化字段）让 grep 可机械化

## 6. UX 设计

> 本节由 ux-designer 在 2026-05-06 填写（P2-3.b）。颜色/字号/字体/间距等视觉语言定义见 floating-window.md 第 6.5 节，本节不重复定义。本 feature 无独立视图，所有 UI 元素寄生在 settings-panel（settings view）与浮窗顶部状态栏内。

### 6.1 信息架构

本 feature 没有独立视图，UI 分布在两处：

**寄生在 settings-panel 的 settings view（顺序即纵向排布优先级）：**

1. 诊断模式开关（toggle，紧跟在设备名 input 下方的分割线之后）
2. 导出日志按钮（ghost 样式，位于诊断模式开关下方）

**浮窗顶部状态栏（仅诊断模式开启时）：**

1. `DBG` 角标（4×14px 蓝底白字小方块，位于顶部状态栏右侧，⚙/× 按钮左侧）

**浮窗状态栏 hint 区（一次性出现，静默恢复）：**

1. "日志写入失败"小提示（仅在磁盘写入异常时一次性显示）

### 6.2 关键流程图（文字版）

主路径（导出日志）：

1. 用户在 main view 点 ⚙ 按钮 → 进入 settings view
2. 用户点"导出日志"按钮 → 按钮变为 loading 态（spinner + "打包中…"文字）
3. 后端完成 zip 打包 → 系统文件保存对话框弹出（默认路径 Downloads）
4. 用户选择路径确认 → 对话框关闭 → 按钮恢复正常 → settings view 底部出现内联成功提示"已导出到 ~/Downloads/synccopy-logs-YYYYMMDD.zip"（约 3s 自消失）

主路径（开启诊断模式）：

1. 用户在 settings view 点击诊断模式 toggle → toggle 切换到 ON 状态
2. 后端立即调整 filter 到 debug 级别 → toggle 下方出现 11px 说明文字"已开启，将记录更多调试信息；日志文件增长较快"
3. 返回 main view → 顶部状态栏右侧出现 `DBG` 角标（蓝底白字，4×14px）
4. 用户关闭诊断模式 toggle → `DBG` 角标消失 → filter 回到 info

主路径（关闭诊断模式）：

1. 用户在 settings view 点 toggle → OFF 状态
2. 后端立即回到 info filter → toggle 下方说明文字消失 → main view 的 `DBG` 角标消失

异常路径：

- 导出过程中系统对话框被用户取消：按钮从 loading 态直接恢复正常，不显示任何提示（取消是合理操作）
- 导出 zip 打包失败（磁盘满 / 权限）：按钮恢复，settings view 底部出现内联错误提示"导出失败，请检查磁盘空间"（红色，约 3s 自消失）
- 日志目录不可写（磁盘满 / 权限）：在浮窗顶部状态栏下方一行显示一次性 hint "日志写入失败"（12px，`#9ca3af`，约 5s 自消失，不打断用户操作）

### 6.3 ASCII wireframe（必填）

settings view 增量部分（插入在 settings-panel 第 6 节 已定义的分割线与清除历史按钮之间）：

```
┌────────────────────────────────┐
│  [⚙ 设置]                   × │← 顶部，drag-region
├────────────────────────────────┤
│                                │
│  本机设备名                     │← 12px #9ca3af label
│  ┌──────────────────────────┐  │
│  │ 工作 Mac                  │  │← input，同 settings-panel 第 6.3 节
│  └──────────────────────────┘  │
│                                │
│  ──────────────────────────── │← 1px 分割线
│                                │
│  诊断模式（更详细日志）   [●  ] │← label 12px #9ca3af + toggle ON(蓝)/OFF(灰)
│  已开启，将记录更多调试信息；   │← 11px #9ca3af，仅 ON 时显示
│  日志文件增长较快              │
│                                │
│  [导出日志]                    │← ghost 按钮，全宽
│                                │
│  ──────────────────────────── │
│                                │
│  [清除历史]                    │← 同 settings-panel 第 6.3 节
│                                │
│  ──────────────────────────── │
│                                │
│  [退出应用]                    │← danger red
│                                │
├────────────────────────────────┤
│  v2.0.0                        │← 11px #9ca3af，只读
└────────────────────────────────┘
```

导出日志按钮的 loading 态：

```
│  [⟳ 打包中…]                  │← spinner 符号 + 文字，ghost 样式 disabled
```

导出成功后 settings view 底部内联提示（在版本号行上方，约 3s 后自消失）：

```
│  已导出到 ~/Downloads/          │← 11px #22c55e（成功绿），truncate 路径
│  synccopy-logs-20260506.zip    │
│  v2.0.0                        │
```

导出失败内联提示（同位置，红色）：

```
│  导出失败，请检查磁盘空间        │← 11px #ef4444（danger red）
│  v2.0.0                        │
```

浮窗顶部状态栏（诊断模式 ON 时）：

```
┌────────────────────────────────┐
│  ● 小组 · 2 台  [加入] [DBG] − ⚙│← [DBG] 4×14px 蓝底#3b82f6 白字，位于 − 左侧
├────────────────────────────────┤
```

日志写入失败 hint（状态栏下方，仅异常时一次性出现）：

```
┌────────────────────────────────┐
│  ● 小组 · 2 台  [加入]   − ⚙  │
├────────────────────────────────┤
│  日志写入失败                   │← 12px #9ca3af，约 5s 自消失，背景微红透明
```

### 6.4 交互细节

点击区域划分：

- 诊断模式 toggle：整个 toggle 区域（label + toggle）均可点击，触发开关切换
- 导出日志按钮：ghost 样式，点击触发导出流程；打包期间变为 disabled loading 态，不响应二次点击
- `DBG` 角标：**不可点击**，纯展示用（点击无反应；提示诊断模式来自 settings view 的 toggle）
- "日志写入失败" hint：不可点击，自消失

鼠标悬停反馈：

- 诊断模式 toggle：整行轻微背景亮度提升（`rgba(255,255,255,0.04)`）
- 导出日志按钮（可用时）：ghost 背景稍亮（同 settings-panel 第 6.4 节 ghost 按钮规则）
- 导出日志按钮（loading 态）：不响应 hover，`cursor: not-allowed`
- `DBG` 角标：不响应 hover

点击反馈：

- 诊断模式 toggle 切换到 ON：toggle 动画（颜色从 `#9ca3af` 灰 → `#3b82f6` 蓝），说明文字以 opacity 淡入（约 150ms）；同时 main view 顶部 `DBG` 角标出现
- 诊断模式 toggle 切换到 OFF：反向，说明文字淡出，`DBG` 角标消失
- 导出日志点击：按钮立即进入 loading 态（spinner + "打包中…"），禁止重复点击

`DBG` 角标设计原则（关键决策）：

- 选择放在顶部状态栏的"−"（折叠球）按钮左侧，而非紧贴状态点旁边
- 理由：状态点旁边已有"小组 · N 台"文字，`DBG` 放在右侧按钮区域更不干扰主状态信息；用户在 main view 时一眼可见，但不遮挡核心连接状态
- 视觉：4×14px，`background: #3b82f6`（primary blue），白色 11px 文字，2px 圆角；颜色选蓝而非红/橙，避免与错误状态（`#ef4444`）混淆，蓝色表示"进行中的特殊模式"
- 不做动画闪烁（DBG 角标的目的是提醒用户"已开启"，不是催促用户关闭）

诊断模式说明文案（关键决策）：

- toggle ON 时，在 toggle 下方显示 11px `#9ca3af` 文字："已开启，将记录更多调试信息；日志文件增长较快"
- 这一文案是用户预期管理的核心——让用户知道"开了什么、会有什么影响"，避免"开了就忘"导致长时间积累大量 debug 日志
- 文案不超过两行（11px 次要色，不喧宾夺主）

导出 loading 态必要性（关键决策）：

- 选择显示 loading 态（spinner + "打包中…"）
- 理由：zip 打包若日志文件较多（如 100MB 上限满载），可能耗时 1-3 秒；无 loading 态用户会以为按钮没响应，连续点击导致重复打包。loading 态成本低（按钮 disabled + 文字变化），收益明确
- 不做全屏遮罩（打包在后台进行，settings view 仍可滚动查看其它设置项）

键盘可达性：

- `Tab`：可达 toggle 与导出按钮
- `Space` / `Enter`：在 toggle 聚焦时切换开关，在按钮聚焦时触发点击

### 6.5 状态与颜色字典

详见 floating-window.md 第 6.5 节。本 feature 特有颜色：

| 元素 | 颜色 | 说明 |
|---|---|---|
| 诊断模式 toggle（ON）| `#3b82f6` | primary blue，与全局字典 primary blue 一致 |
| 诊断模式 toggle（OFF）| `#9ca3af` | 次要灰 |
| `DBG` 角标背景 | `#3b82f6` | primary blue，与 toggle ON 色联动 |
| `DBG` 角标文字 | `#ffffff` | 白色，11px |
| toggle 说明文字（ON）| `#9ca3af` | 11px 次要色 |
| 导出成功提示 | `#22c55e` | 成功绿，与全局字典一致 |
| 导出失败提示 | `#ef4444` | danger red，与全局字典一致 |
| 日志写入失败 hint | `#9ca3af` | 次要色，不使用 danger red（避免令用户以为功能崩了）|
| 日志写入失败 hint 背景 | `rgba(239,68,68,0.08)` | 极浅红背景，微弱区分 |

### 6.6 边界与例外

- 诊断模式已开启 + 用户关掉应用后重启：开关状态持久化（写 Config），重启后仍为 ON，`DBG` 角标仍出现。这是有意设计——让用户在重现 bug 期间不用每次重开都打开开关
- 诊断模式开启期间导出日志：正常流程，导出文件包含 debug 级别日志（体积更大），成功提示照常显示
- 导出时日志目录为空（应用刚启动、尚无日志文件）：后端产出一个空 zip 或含 0 字节的空 log 文件，不报错；用户收到成功提示（zip 存在但内容为空，说明书中告知即可）
- 导出对话框：依赖系统文件保存 API（Tauri 的 `dialog` 插件），不可自定义对话框 UI；Windows 与 macOS 对话框外观不同，属平台差异，UX 不做统一要求
- 设置面板内容变长（增加了诊断模式 + 导出按钮）：settings view 内容区域可能超出 320×420 容器可见高度。需要前端工程师在 settings view 内部加 `overflow-y: auto` 或确保内容在预估高度内（参见 6.7）
- 导出路径太长：成功提示显示的路径做 CSS `text-overflow: ellipsis` 或只显示文件名，不做折行（底部空间有限）
- 实测可能暴露的问题：settings view 增加两个元素后，低分辨率 / 缩放因子高的屏幕可能需要滚动才能看到"退出应用"按钮；需实测验证是否需要缩减各元素间距

### 6.7 给前端工程师的实现提示（可选）

- settings view 在增加诊断模式与导出按钮后，建议对 settings view 内部的内容区域加 `overflow-y: auto`，确保在 320×420 容器里各按钮仍可到达
- 诊断模式 toggle 的展开说明文字，建议用 CSS `max-height` transition（`max-height: 0 → 48px`，200ms ease）控制折叠/展开，使过渡自然
- 导出按钮的 loading spinner，建议使用 CSS `@keyframes` 旋转一个 Unicode 圆圈字符（如 `⟳`）或简单 SVG，避免引入图标库
- `DBG` 角标的出现/消失建议用 `opacity` + `width` transition（`width: 0 → 28px`，150ms），避免角标突然出现对状态栏布局产生跳动感；或用 `visibility` + `opacity` 组合

### 6.8 给主窗口和 PM 的反馈（UX 开放问题回应）

**问题（来自 spec 第 7 节 [P1] [UX]）：诊断模式 `DBG` 角标的视觉（与现有状态点是否冲突？颜色选什么避免与"小组 · N 台"绿色状态点混淆）**

结论：`DBG` 角标放在顶部状态栏右侧（折叠球"−"按钮左侧），而非紧贴绿色状态点旁边。颜色选 `#3b82f6`（primary blue），与已连接状态的绿色（`#22c55e`）明确区分；蓝色在本视觉系统语义为"进行中的状态"（见 group-approval 第 6.3 节 申请方浮窗等待状态点也是蓝色），与"诊断模式进行中"语义一致。角标不做闪烁，仅静态展示。

**关于 settings-panel 第 6 节 既有布局的兼容性说明：**

本 feature 在 settings-panel 的 settings view 里新增了两个元素（诊断模式 toggle + 导出日志按钮），插入位置在分割线之后、清除历史按钮之前。这一排序体现操作风险梯度：诊断模式（低风险，可逆）→ 导出日志（中性操作）→ 清除历史（中风险，全设备清除）→ 退出应用（高风险，不可逆）。settings-panel 第 6.3 节 的 ASCII wireframe 需要 PM 在后续协调更新，以反映新增的两个元素（UX 不直接修改 settings-panel spec）。

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 4 条] [P1 3 条] [P2 2 条]

- [P0] [架构师] 用 `tracing-appender` 提供的 `RollingFileAppender`（按日 / 按大小自带轮转）还是自写 rotation？前者上手快、维护少，但轮转策略可能不够灵活（如不能同时按日 + 按大小双触发）。需 ADR 论证选型 + 否决另一路径
- [P0] [架构师] 运行时切换日志级别（"诊断模式"）的实现：`tracing-subscriber::reload::Handle` 提供热重载能力但要求初始化时就建好 reload handle。是否引入这一复杂度，还是退化为"切换后下次启动生效 + 提示用户重启"？
- [P0] [安全] 哪些字段算敏感、不许入日志？明文剪切板内容（text、image bytes、file bytes）+ AES 密钥 + X25519 私钥 100% 是；但 `device_id`（UUID 形式，跨设备稳定标识）/ peer 的 LAN IP / device_name（用户自定义可能含个人信息）是否算？需安全审阅明确黑名单
- [P0] [安全] 导出 zip 是否需要在导出前给用户一次"将包含以下信息：...，确认导出？"的预览或免责声明？还是导出文件本身在头部加一行 "本日志由 Sync Copy v<version> 在 <time> 导出，可能包含 device_id、peer IP、设备名等元数据" 的免责头？
- [P1] [架构师] 日志总占用上限 100 MB / 7 天保留：在轮转策略里如何精确控制？（按大小最简单但日期边界不齐；按日最直观但单日大量事件可能超 10 MB 单文件）。建议组合策略：单文件 ≤ 10 MB + 文件数 ≤ 10，需架构师在 ADR 里把数字锁死
- [P1] [架构师] panic hook 的实现：捕获 panic message + backtrace 写日志后是 `std::process::abort()` 还是让 Tauri 的默认 hook 接管？影响应用退出方式与用户感知
- [P1] [UX] 诊断模式 `DBG` 角标的视觉（与现有状态点是否冲突？颜色选什么避免与"小组 · N 台"绿色状态点混淆）
- [P2] [架构师] 是否需要在 `Config` 里持久化"用户上次自定义的日志级别"（不仅 on/off 诊断模式，而是允许 trace/debug/info/warn/error 任选）？v0 仅 RUST_LOG 临时；v2 在 settings-panel 暴露"专家档"是否值得增加 UI 复杂度
- [P2] [架构师] 多进程 / 多 tauri 实例情形（用户开两个 Sync Copy 实例）下日志文件会被两进程同时写入——是否需要 file lock 或按 PID 分文件？v0 的"单实例守卫"由系统托盘的 `--single-instance` 行为隐式承担，但日志层是否独立加固

## 8. Review 段（占位）

> code-reviewer / tech-architect / security-reviewer 后续填写。本 feature 的"敏感字段过滤"硬约束需 security-reviewer ACK；轮转策略与运行时级别切换的实现选型需 tech-architect ACK。
