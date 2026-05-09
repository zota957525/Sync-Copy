---
status: SPEC_REVIEWED
owner: product-strategist
related_adrs: [ADR-003, ADR-009, ADR-010]
related_specs: [00-product-overview, group-discovery, peer-heartbeat, settings-panel, tray-integration]
created: 2026-05-06
updated: 2026-05-08
revised: 2026-05-08 — ADR-003 第 3.5 节 锁定退出路径唯一化（tray / settings / Cmd+Q / OS close 4 处全部走 quit_app → Lifecycle::shutdown 7 步）
priority: P2
---

# group-leave-notify — 主动下线广播让组内立即感知

## 1. 问题（为什么做）

当用户主动退出 / 切换网络 / 关闭电脑时，组内其它机器需要立即看到"少了一台"。仅靠 `peer-heartbeat`（10s × 2 失败 = 至少 20s）会让其它人在长达 20 秒内**继续向已死设备发剪切板 / 文件**——浪费带宽 + 用户看到 `小组 · 3 台` 实际有效只有 2 台。`group-leave-notify` 是一条"我要走了"的礼貌广播：触发即广播 `/peers/leave` → 所有 peer 立即把自己从对端 peers 表中移除 + 状态计数减 1，**与心跳互补成双层防御**（主动 + 被动）。

它和 `peer-heartbeat` 的关系：leave 是**好情况下**的快速感知（用户正常退出），heartbeat 是**坏情况下**的兜底（断电 / 网络消失 / 进程崩溃）。两者都必须存在；只有 leave 没心跳→ 异常退出无法收敛；只有心跳没 leave → 正常退出延迟 20s。

## 2. 用户故事

- As a user normally quitting via tray menu / settings panel / OS app close, I want my peers to instantly see "1 台" instead of "2 台" within ~1 second, so that the group state is honest right after my departure.
- As a network operator on the same LAN, I want the leaving device's broadcast to be best-effort (max 1.5 second wait) rather than blocking the quit indefinitely, so that a slow peer never delays my exit.
- As a peer of the leaving device, I want the leaver to be silently removed from my peers table without any popup (it's not a "rejection" notification, just a quiet update), so that user attention is not interrupted.

## 3. 范围

**in scope**：
- HTTP 端点 `/peers/leave`（POST，body = `GroupActionReq { origin_device_id, seq }`）
- handler `handle_leave`：
  - origin 在本机 peers 表 → 否则 403（防止陌生设备伪造 leave 让组员误剔除）
  - `seen_seq_and_update(origin, seq)` 去重
  - `peers.remove(origin) + peer_keys.remove(origin)` + `update_status_connected` + emit `status-updated`
  - tracing::info `peer left`
- 客户端 broadcast `broadcast_leave(state)`：
  - 取 `device_id` + `next_seq()` 构 `GroupActionReq`
  - 对 `peers.snapshot()` 每个 peer 起独立 `tokio::async_runtime::spawn` task POST `/peers/leave`
  - **等所有 task 完成或失败**（与 trust gossip 同模式：内层 spawn + 外层 join_all），便于上游用 `tokio::time::timeout(1500ms)` 控总时长
  - 单 peer 失败仅静默丢（leave 是最大努力；失败靠 heartbeat 兜底）
- 触发点（共四处入口，所有调 `quit_app` / `leave_group` 命令的源头）：
  - **`commands.rs::quit_app` 命令**：用户点设置面板 `退出应用` 按钮 → `tokio::time::timeout(1500ms, broadcast_leave(state))` → 清状态 → `app.exit(0)`
  - **`commands.rs::leave_group` 命令**：（v0 中存在但 UI 未暴露——预留为"切组"入口；本 P2 阶段保留命令但 UI 不暴露）走相同的 leave 广播 + 清 peer / approved / banned / forwarded_approvals + status = Idle
  - **托盘菜单 `退出`**：`tray-integration` 第 3 节 中 P0 阶段简化为 `app.exit(0)`，本 P2 阶段升级为调 `quit_app` 命令统一走 leave 广播
  - **OS 关闭信号 / Cmd+Q**：Tauri 可挂 `on_window_event(CloseRequested)` → 路由到 `quit_app` 命令（属架构师 ADR 决策；本 spec 仅约定语义需统一）
- 1.5 秒超时：单 peer 网络挂死时，整体不超过 1.5s 等待（`build_client()` reqwest 自身 5s timeout 在外层 1.5s 之内会被强制 cancel）；保证用户感知 `app.exit(0)` 在 ≤ 2 秒内发生
- 后续 200ms `tokio::time::sleep` 让网络任务收尾再 exit（v0 经验值；不严格保证，仅缓解）

**out of scope**：
- ack / 重发机制（leave 是 best-effort；网络挂时丢就丢，让 heartbeat 兜底）
- 携带 "为什么离开"（用户主动 vs 网络切换 vs 进程崩溃）—— 不区分，UX 不需要
- leave 后某 peer 漏收 → 该 peer 仍向已死设备发请求 → 等 20s heartbeat 失败踢出（与 `peer-heartbeat` 互补）
- 加密 leave body（DTO 仅 device_id + seq，无机密内容；v0 不加密；v2 同）
- 暂时离线（"network sleep" 模式）；用户走了就是走了
- 跨进程 leave（如 macOS 登出会话）—— OS 关闭信号兜底
- 区别"完全退出"与"仅下线但应用还在"的两种 leave 语义（v2 仅一种：进程要退出）

## 4. 验收标准（Definition of Done）

- [ ] A、B、C 三机已 `小组 · 3 台`。在 A 上点设置面板 `退出应用` → A 进程在 ≤ 2 秒内退出 + B、C 浮窗状态 1 秒内变为 `小组 · 2 台`（不等心跳）
- [ ] A 上点托盘菜单 `退出` → 同上行为（与设置面板退出统一走 leave 广播）
- [ ] A 上 Cmd+Q（macOS）或关闭最后一个窗口 → 同上（OS 信号路由到 `quit_app`，属架构师 ADR 决策）
- [ ] 在 A 拔网线后用户退出 → A 等 1.5 秒后仍 exit（不卡住）；B、C 端 20 秒后由心跳剔除 A
- [ ] 陌生设备 X 向 B 直接 POST `/peers/leave { origin: <某 device_id> }` → B 返 403，不剔除任何 peer（origin 必须在 peers 表）
- [ ] 同一 leave 广播由网络重传两次（DOS 模拟）→ 第二次 seq 去重静默丢
- [ ] A 离开后 5 秒内 B 上原本要发给 A 的剪切板内容 → broadcast_text 发现 peers 表已无 A，跳过该 peer 不报错
- [ ] tray quit / settings quit / Cmd+Q / window close 四个入口都走 `quit_app` 命令（路径唯一，由 ADR 强制）

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的
`legacy-prototype` 分支 `src-tauri/src/network/server.rs::handle_leave`（约 360-380 行）：origin 在 peers 表校验 → seen_seq_and_update → `peers.remove + peer_keys.remove + update_status_connected + emit status-updated + tracing::info`。`network/client.rs::broadcast_leave(state)`（约 240-275 行）：构 `GroupActionReq { device_id, seq }` → for each peer → spawn POST `/peers/leave` → `for h in handles { let _ = h.await; }` 等所有完成。`commands.rs::quit_app` async 命令：
1. `tokio::time::timeout(1500ms, broadcast_leave(state_c))` 给所有 peer 发 leave
2. clear peers / peer_keys / approved_device_ids / banned_device_ids / forwarded_approvals
3. server_shutdown 关 axum
4. `tokio::time::sleep(200ms)` 让 spawn 的网络 task 收尾
5. `app.exit(0)`

`commands.rs::leave_group` 同样模式，但不调 `app.exit`（保留进程，仅断组），且把 status 设回 Idle。`lib.rs` 中**托盘菜单 `quit` 直接调 `app.exit(0)` 不走 quit_app**（00 总览 第 5.4 节 + tray-integration 第 5.4 节 已点名这是 v2 必须修的一致性 bug）。`commands.rs::hide_window` 命令（× 关浮窗按钮）调 `ensure_on_screen + window.hide()` 不退应用——这是预期，× 是隐藏不是退出。

### 5.2 v0 暴露的具体坑
- **三处退出路径不一致**：托盘 `退出` 直接 exit 不发 leave；设置面板 `退出应用` 走 quit_app 发 leave；浮窗 × 是 hide。维护者改一处易遗漏其它（00 总览 第 5.4 节 + tray-integration 第 5.4 节 + settings-panel 第 5.4 节 共同提及）
- **OS 关闭信号未挂**：macOS Cmd+Q 直接退到 OS 层（不走 quit_app），v0 行为基本同托盘 quit—— 不发 leave
- **leave 是 best-effort**：网络挂时丢即丢；这是设计选择但用户期望"我点了退出我组员就该看到我走了"——靠心跳兜底要等 20s，体验不一致
- **1.5s 超时是经验值**：单 peer 网络挂死时整体 1.5s 即放弃；多 peer 时若某 peer 慢但其它快，整体仍等 1.5s（不会因 timeout 单 peer 缩短整体）
- **200ms sleep 让 spawn task 收尾**：是经验值；spawn 的 task 在 `app.exit(0)` 时会被 OS 杀掉，部分 leave 请求可能未发出。`tokio::join_all` 已在 1.5s 内 await 但 spawn 内部的 reqwest 可能仍在 handshake 阶段
- **leave 不带 reason / metadata**：用户层不知道"X 是主动走还是被踢"——v0 透明无差别
- **OS 强制 kill（活动监视器 → 强行退出）**：不发 leave，组员等心跳——属设计可接受，文档说明即可
- **leave_group 命令（保留进程仅断组）UI 未暴露**：v0 没 "切组" 按钮，命令存在但走不到；v2 同样保留作 future use

### 5.3 v2 应继承
- `/peers/leave` 端点 + `GroupActionReq` DTO（与 `clear_history` 共用 DTO 结构）
- handler 校 origin 在 peers + seq dedupe + remove peer + update status + emit
- broadcast_leave 等所有 peer + 上游 `timeout(1500ms)`
- quit_app 命令的 5 步序列（broadcast → clear → close server → sleep → exit）
- leave 是 best-effort + heartbeat 兜底的双层设计

### 5.4 v2 应挑战
- **退出路径合并到唯一 `quit_app`**：四个入口（设置面板 / 托盘菜单 / Cmd+Q / OS 关闭信号）必须**全部**走 `quit_app`（必须在 ADR 明文，CLAUDE.md 第 4.2 节 禁止维护者只改一处）
- **OS 关闭信号挂载**：Tauri `on_window_event(CloseRequested)` → 阻止默认行为 + 调 `quit_app`（属架构师 ADR 决策）
- **broadcast_leave 内层 spawn + 外层 join_all + 再外层 timeout 三层包装**：是否合并简化（如 `try_join_all_with_timeout`）—— 与 `group-trust-gossip` 第 5.4 节 同议题
- **leave 广播添加 ack？**：当前 fire-and-forget，丢包即丢；是否要求每个 peer 200 OK 后才计数（"已通知 N/M 台"反馈给用户）—— 增加复杂度是否值得
- **leave_group 命令（保留进程仅断组）的 UI 暴露**：是否在 settings 加 `离开当前小组` 按钮？v0 命令保留无入口；属 settings + UX

## 6. UX 段（占位）

> 本 feature 主要是后端协议层；用户感知点仅在"对方退出后我多久看到"。第 6 节 N/A（无显式 UI 元素，所有 UI 由 `settings-panel` 退出按钮 / `tray-integration` 退出菜单 / `floating-window` 关闭按钮等已有 UI 触发）。

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 2 条] [P1 4 条] [P2 2 条]

- [P0] [架构师] 退出路径合并：CLAUDE.md 第 4.2 节 已禁止"散点维护"，必须 ADR 明文：tray quit / settings quit / Cmd+Q / OS close 四入口全部经过 `quit_app` 命令
- [P0] [架构师] OS 关闭信号挂载：`on_window_event(CloseRequested)` → block + 调 quit_app；与 × 关浮窗按钮（hide 而非 exit）的语义如何区分？是 macOS Cmd+W 关窗口 vs Cmd+Q 退应用的差别
- [P1] [安全] leave body 不加密：仅 origin_device_id + seq 是公开 metadata，无密钥泄露；攻击者**伪造**leave 时被 origin 校验拦下（origin 必须在 peers 表）—— 仍有疑问：被攻陷成员可发 leave 让自己提前消失吗？属安全审阅
- [P1] [架构师] 1.5s 超时是否过短？多 peer + 慢 WiFi 时部分 leave 可能未发出
- [P1] [架构师] broadcast_leave 三层包装（spawn + join_all + timeout）合并简化
- [P1] [架构师] 200ms 收尾 sleep 是否可去掉（替换为 graceful shutdown 等所有 spawn task）
- [P2] [架构师] leave 是否升级为带 ack 的 broadcast（"已通知 N/M 台"反馈），还是保持 best-effort
- [P2] [UX] 用户主动 leave 后 UI 上是否有"已通知 X 台"反馈？v0 无（直接退出）

## 8. Review 段（占位）

> code-reviewer / tech-architect / security-reviewer 后续填写。本 feature 涉及网络协议与状态收敛路径，必须经 security-reviewer ACK（CLAUDE.md 第 9 节）。

---

## 8. Code Review (by code-reviewer · 2026-05-09 · PR-3 Lifecycle + ClientPool + AppState)

> 范围：commit 25fe411（基础设施三件套最后一件，落 ADR-010 v1.2 + ADR-009 第 3.5 节 client_pool + ADR-008 MUST-5）。本 spec 第 8 节是 5 份共享 spec（peer-heartbeat / group-leave-notify / diagnostic-logging / tray-integration / settings-panel）中最直接关联的承载点（关闭 7 步含 leave 1500ms timeout 由 PR-3 占位实现）。

**结论**：CHANGES_REQUESTED（2 条 [低/nit] 文本/单测级补丁；非阻塞主路径，主窗口可静默派 backend-impl 落）

### 8.1 ADR / spec 一致性（5 聚焦点）

1. **MUST-5 panic hook 注册位置**：✅ APPROVED。`lib.rs::run` 第 34 行（Builder::default 第 49 行 + AppState::new 第 47 行触发的 Lifecycle::new 之前）；`prev(info)` 链保留 ✅；hook 不调 `app.emit` / `tauri::dialog`，用 osascript / powershell / eprintln 三 cfg 隔离 ✅；payload 仅取 `&'static str` / `String` 字面 ✅；dialog 文案不含 payload ✅；`prev(info)` 注释明文 "默认 backtrace 含函数符号 + 行号，release 模式不含栈变量值" 落 P1 补丁 ✅。
2. **client_pool 接口契约**：✅ APPROVED。`get(id)` miss 返 None 不 lazy add（单测 `get_does_not_lazy_add` 显式断言 + 池 size 仍 0 ✅）；`remove` 是 `pub(crate)` + `#[allow(dead_code)]` PR-3 临时标注 ✅；`replace` 用 `Client::builder().no_proxy().build()` 落 lessons-learned 第 4.1 节 ✅；replace 写锁内 HashMap 替换让旧 Client 随原值 drop ✅。
3. **Phase 状态机 + shutdown 幂等**：⚠ APPROVED-with-nit。Phase 4 态 + 转移注释明文 ✅；shutdown 入口检查 `Shutting | Dead` 返 Duration::ZERO（单测 `shutdown_idempotent_reentry` + `shutdown_idempotent_when_already_shutting` 双覆盖 ✅）；step 6 在 step 4 join 之后 ✅；log_guard 字段顺序最后 ✅。**遗漏**：ADR-010 第 6 节单测 #9 期望"非法转移（Dead → Running）panic"，当前 `phase_transitions_valid` 仅验证合法路径可写，没有 enforcement（任何代码可 `*phase.write() = X`）。属占位阶段可接受 — 见 8.5 todo P-low-1。
4. **deadline 命中 tracing::warn**：✅ APPROVED。step 3 leave / step 4 health / step 5 server 三步 timeout 均有 `tracing::warn!(target: "lifecycle", step, deadline_ms, actual_ms, ...)` 落盘 ✅；P0 tray TODO 注释在 lib.rs `quit_app` 命令上方明文（行 202）✅；P0 tray bypass tracing::warn 强制观测线 PR-3 未触达（tray 集成 PR-4 落地 — 符合 PR-3 范围）。
5. **依赖兼容性 cross-check**：✅ APPROVED。tokio-util 0.7 + tracing-appender 0.2 + thiserror 1 三依赖全在 ADR-010 第 5 节实施提示 #2 列出；tokio 1 / tracing 0.1 同生态版本无冲突；`tokio-util = { version = "0.7", features = ["rt"] }` 启用 CancellationToken ✅。

附加查项：cargo clippy `-D warnings` 0 warning ✅；cargo test --lib 43/43 pass ✅（PR-1 18 + PR-2 14 + PR-3 11）；cargo fmt --check pass ✅；空 worker 仅 `select! { cancel | sleep(5s) }` 不引业务（无 reqwest / arboard）✅；4 退出路径仅 quit_app 命令注册（CloseRequested / tray menu PR-4）✅；无 `app.exit(0)` / `process::exit` 绕过 Lifecycle 的代码（仅启动失败 + panic hook 内 abort，符合 ADR-010 第 3.4 节）✅。

### 8.2 发现的问题

#### [低/nit] `show_native_fatal_dialog` 内 `_message` 变量未使用 + osascript / powershell 命令字符串拼接潜在注入面

- 文件：`src-tauri/src/lib.rs:144-147`（dead code）+ `:155-157`（osascript format!）+ `:172-176`（powershell format!）
- 现象：`_message` 计算后从未读；mac/Win 分支各自独立 `format!` 把 `location` 直接插入 shell 命令字符串
- 风险：`location` 来源 `info.location()`（编译期 `file!()` + `line!()`，攻击者无法构造），但若未来源码引入特殊字符路径（如含 `"` / `'` / `\`）会让 shell 命令体破裂；`_message` 是 dead code 引轻微误导
- 建议修法：删 `_message` 行；mac/Win 分支用 `Command::arg()` 多次传参（osascript 用 `-e` 单参数较难；可改 escape `"` `\` 后再插入）；或加 `// SECURITY: location is compile-time file!:line!, attacker-uncontrollable` 注释明示已审

#### [低/nit] ADR-010 第 6 节单测 #9 "非法转移（Dead → Running）panic" 未实现 + lifecycle.rs 521 行小超 ADR 第 5 节"≤ 350 行硬约束"

- 文件：`src-tauri/src/app/lifecycle.rs`（整体 521 行 / `phase_transitions_valid` 测试 504-520）
- 现象：(a) ADR-010 第 6 节单测 #9 期望非法转移 panic，但 phase 字段是 `RwLock<Phase>` 直写无 enforcement；(b) lifecycle.rs 521 行比 ADR-010 第 5 节"≤ 350 行硬约束"超 49%（但 ADR 第 9 节自查写"≤ 500 行硬约束已达"两处文本自相矛盾）
- 风险：(a) 未来 implementer 误写 `*phase.write() = Phase::Running` 在 Dead 之后，无 enforcement；属低危（仅内部代码，无外部接口）；(b) 行数硬约束 ADR 文本自相矛盾，本 PR 选了较宽松的 500 解读
- 建议修法：(a) 加 `Lifecycle::set_phase(new)` 内方法做转移合法性 assert（仅 cfg(debug_assertions) 即可，release 不 panic 仅 warn）；或单测加"在 Dead 上写 Running 后 panic"用 `#[should_panic]` 但需先实现 enforcement；(b) 不必修代码，可在后续 ADR-010 supersede 时统一 350/500 文本；当前作 nit 记录

### 8.3 风险点（隐藏 bug 候选）

- **PR-4 落地 axum server 时**：`server_shutdown_tx` 的 oneshot::Sender 必须在 step 5 spawn axum 之前由 lifecycle 持有 — 当前 `start()` 占位未涉及；PR-4 实施时检查
- **PR-5 clipboard std::thread 落地时**：mpsc `ClipboardCmd::Shutdown` 的 send 必须在 step 2，join 在 step 4，且 join 100ms 超时后 detach（非 abort，因 std::thread 没 abort API）—当前 lifecycle struct 字段未预留 `clipboard_cmd_tx` / `clipboard_thread`，PR-5 实施时补
- **panic 在 start step 1 之前发生**：tracing 未 init → tracing::error! 是 no-op，仅 eprintln + dialog；ADR-010 第 4.2 节已声明可接受，PR-3 验证此边界（dialog 不依赖 tracing 可正常弹）
- **panic hook 内 eprintln msg 字面**：ADR-008 第 6.1 节明确允许 stderr 写 payload 字面（攻击者拿不到 stderr，仅 dev tail 可见）；release build stderr 用户看不到（ADR 已声明）

### 8.4 测试覆盖评估

PR-3 范围下 11 条单测合理（lifecycle 6 + client_pool 5）。**未覆盖但属 PR-4/5 范畴**（不算欠债）：
- `start_step5_bind_fail_unwinds_step4_and_step1`（mock TCP bind 失败 — PR-4 落 axum 后写）
- `shutdown_step3_leave_timeout` mock 真实 reqwest 永远 timeout（PR-4 落 leave broadcast 后写）
- `shutdown_step4_clipboard_join_timeout` mock std::thread（PR-5 落 clipboard 后写）
- `panic_hook_*` 4 条单测（panic hook 注册测试需 fork process 或 sentinel mock，独立测试模块；ADR-010 第 6 节亦为推荐非强制）

### 8.5 给 implementer 的明确 todo 清单

> 主窗口策略：以下 2 条均 [低/nit]，不阻塞 PR-4 启动。建议合到 PR-4 第一个 commit 顺手清理；不必单独 patch commit。

- [ ] **P-low-1**：删 `lib.rs:144-147` 的 `_message` dead code；mac/Win 分支 `format!` 上方加注释 `// SECURITY: location is compile-time file!:line!, attacker-uncontrollable`（明示 shell 拼接已审）
- [ ] **P-low-2**：（可选 — 不修代码亦可）lifecycle.rs `phase_transitions_valid` 单测顶端加注释说明 "ADR-010 第 6 节单测 #9 期望非法转移 panic 留 PR-4 enforcement；当前 PR-3 仅验证合法转移可写"；或在 `Lifecycle::set_phase` 加 `debug_assert!` 转移合法性

### 8.6 owner 边界自查

未写代码 ✅；未改 ADR / spec 第 1-7 节 ✅；未改 PLAN.md（v2-9 主窗口职责）✅；未调任何 agent ✅；review 段写到合适 spec（group-leave-notify 是关闭 7 步含 leave 1500ms timeout 最直接关联）✅；本段 ≤ 80 行预算控制 ✅。
