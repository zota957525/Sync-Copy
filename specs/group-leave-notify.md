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
