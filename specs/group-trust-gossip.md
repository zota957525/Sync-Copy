---
status: SPEC_REVIEWED
owner: product-strategist
related_adrs: [ADR-003]
related_specs: [00-product-overview, group-approval, group-discovery]
created: 2026-05-06
updated: 2026-05-08
revised: 2026-05-08 — ADR-003 第 3.3 节 PeerRegistry.approve / .ban 集中实现 trust 互斥覆盖语义；trust 传染性风险留 ADR-008
priority: P2
---

# group-trust-gossip — 一台同意 / 拒绝即全组生效的信任与封禁广播

## 1. 问题（为什么做）

`group-approval` 解决"任一在线设备的用户点同意 / 拒绝即作数"——但只解决**当前这次握手**。如果 C 后续主动联系 B（不再走 A），B 必须从零再走一次审批：A 上的人看不到，B 端没人也只能等 30s timeout。这违反"一次决定，全组生效"的产品承诺（00 总览 第 1 节）。trust-gossip 是**记忆与传播**那个决定的层：A 同意 C 后，A 把这个事实 broadcast 给所有已连接 peer → 它们各自把 C 的 device_id 加入 `approved_device_ids` 内存集合 → C 后续主动连接它们时直接跳过审批。

封禁同理：A 拒绝 C 后 broadcast `/peers/ban` → 所有 peer 加入 `banned_device_ids` 集合 → C 后续被任一节点直接 403。

这是分布式状态收敛问题。设计简单但坑多：trust 与 ban 是**互斥覆盖**关系（被 trust 的同时移除 ban，被 ban 的同时移除 trust）；origin 必须是已知 peer 才接受；本机是 subject 自身时不能"封自己"；trust 没持久化（重启全清，与"无密钥管理心智"一致）。

## 2. 用户故事

- As a user with A+B established and trying to add C, I want to approve C **once** on either A or B, so that C can subsequently connect directly to the unattended one without forcing me to walk over and approve a second popup.
- As a user who rejected D on any one device, I want D to be auto-rejected by all my devices in the group, so that one decision blocks D group-wide without me having to ban on every machine.
- As a member of a group, I want trust state to be wiped when I restart, so that an old "trust" inherited from a colleague's device cannot accumulate stale risk—security posture matches the no-key-management ethos.

## 3. 范围

**in scope**：
- HTTP 端点 `/peers/trust` 与 `/peers/ban`（POST，body = `TrustReq`）：
  - `TrustReq { origin_device_id, seq, subject_device_id, subject_device_name }`
  - origin = 决策广播来源（A / B / 任一已加入节点）
  - subject = 被信任 / 被封禁的设备 ID
- handler 共用规则（`handle_trust` + `handle_ban`）：
  - origin 必须在本机 `peers` 表 → 否则 403（防止陌生设备投毒 trust 列表）
  - `seen_seq_and_update(origin, seq)` 去重 → 重复 200 OK 静默丢
  - subject == 本机 device_id → 200 OK 静默丢（自己不信任 / 封禁自己）
- `handle_trust` 行为：
  - `approved_device_ids.insert(subject)`
  - `banned_device_ids.remove(subject)` — **trust 覆盖 ban**
- `handle_ban` 行为：
  - `banned_device_ids.insert(subject)`
  - `approved_device_ids.remove(subject)` — **ban 覆盖 trust**
  - **额外**：若 subject 当前在 peers 表（v0 称 `was_peer = true`），直接 `peers.remove(subject) + peer_keys.remove(subject)` + `update_status_connected` + emit `status-updated`（即时踢出连接）
- 客户端 broadcast：
  - `broadcast_trust(state, subject_id, subject_name)` 和 `broadcast_ban(...)` 共享底层 `broadcast_trust_decision(state, path: "/peers/trust" | "/peers/ban", subject_id, subject_name)`
  - 用 `state.next_seq()` 生成单调 seq
  - 跳过当事人自己（不发给 subject）
  - **用 `tokio::join_all` 等所有 peer 完成或失败**（区别于剪切板的 fire-and-forget），便于上游用 `tokio::time::timeout(2s)` 控总时长
  - 每 peer 失败仅 warn log，不影响其它 peer 的发送
- 触发点：
  - `group-approval` 中 A 的握手 handler 决定 = approve 时调 `broadcast_trust`，决定 = reject 时调 `broadcast_ban`（已在 `group-approval` 流程接口预留）
  - 各自接 `tokio::time::timeout(2s, broadcast_trust/ban)` 限制总时长（避免阻塞握手响应）
- 已知 peer 的握手短路（`handle_handshake` 中已实现，本 feature 仅依赖）：
  - `peers` 中已存在 → 直接重新协商密钥（已在握手流程）
  - `approved_device_ids` 命中（白名单）→ 直接 ECDH + 加 peers，不弹审批
  - `banned_device_ids` 命中（黑名单）→ 直接 403
- 信任与封禁状态**不持久化**（00 总览 第 3 节 锁定）

**out of scope**：
- trust / ban 持久化到磁盘（00 总览 第 3 节）
- multi-hop trust gossip（A → B trust 后 B 再传给 C 是否触发重发？v0 不做，因 A 直接 broadcast 给所有 peer 包括 C 已经覆盖）
- 用户主动撤销某个 trust（settings 没有 trust 名单 UI，重启即清；属 `settings-panel` 第 3 节 已 out of scope）
- 时序保证（最终一致性即可；网络抖动让某个 peer 漏收 trust 时退化为再走一次审批）
- ban 后强制断开正在传输的文件 / 剪切板（仅删 peer + 密钥；正在飞行的请求继续完成或被对端 403）

## 4. 验收标准（Definition of Done）

- [ ] A、B 已连接。在 C 上加入填 A 的 IP → A 上点同意（B 上弹框也同时消失，由 `group-approval` 保证）→ 5 秒内**直接**在 C 上加入填 B 的 IP（不点 A）→ B 不弹审批，C 直接连上 B（白名单生效）
- [ ] A、B 已连接。在 D 上加入填 A 的 IP → A 上点拒绝 → 5 秒内 D 主动加入填 B 的 IP → B 直接 403（黑名单生效），不弹框
- [ ] D 被 ban 后正在小组里的 C 设备如果 device_id 与 D 相同（不会发生，但模拟）→ B 收到 ban 时 `was_peer = true` 立即把 C 踢出 + 状态计数减 1
- [ ] 陌生设备 X（不在 peers 表）向 B 直接 POST `/peers/trust { subject: D }` → B 返 403，不污染 approved_device_ids
- [ ] 同一 trust 决定 broadcast 两次（重发）→ 第二次因 seq 去重被静默丢
- [ ] 重启 A → A 的 approved_device_ids 与 banned_device_ids 都清空；C 下次主动连 A 重新走审批
- [ ] trust 与 ban 互斥覆盖：A 先 ban D 后 trust D → A 的 banned_device_ids 不含 D + approved_device_ids 含 D（反之亦然）
- [ ] broadcast_trust 在 5 个 peer 中 1 个网络挂掉 → 4 个 peer 在 2s 内更新成功，挂掉的 peer 仅 log warn 不影响整体（最终一致性）

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的
`legacy-prototype` 分支 `src-tauri/src/network/server.rs::handle_trust`（约 410-440 行）：origin 在 peers 表校验 → seen_seq_and_update → subject == my_id → return；`approved_device_ids.write().insert(subject)` + `banned_device_ids.write().remove(subject)` + tracing::info。`handle_ban`（约 580-620 行）镜像逻辑：banned 加、approved 移；额外检查 `was_peer` —— 若 subject 当前在 peers 表中则 peers.remove + peer_keys.remove + update_status_connected + emit status-updated。两个 handler 都用 `TrustReq` DTO。`network/client.rs::broadcast_trust_decision(state, path, subject_id, subject_name)` 是私有函数，`broadcast_trust` / `broadcast_ban` 是 thin wrapper 仅传不同 path。底层用 `tokio::async_runtime::spawn` 起每 peer 独立 task，最后 `for h in handles { let _ = h.await; }` 等所有完成。`build_client()` 共用 5s timeout / 3s connect_timeout。`state.rs::AppState` 含 `approved_device_ids: RwLock<HashSet<String>>` + `banned_device_ids: RwLock<HashSet<String>>`。`server.rs::handle_handshake` 在新设备路径前先查 banned_device_ids → 403，再查 approved_device_ids → 跳过审批直接通过。`handle_handshake` 决定 = approve 后用 `tokio::time::timeout(2s, broadcast_trust(...))` 控时长；同理 reject 后 `broadcast_ban`。

### 5.2 v0 暴露的具体坑
- **trust gossip 的传染性风险**：若某成员被攻陷 → 它能 `/peers/trust` 任意 device_id 让全组接受陌生设备（00 总览 第 7 节 安全已点名）。v0 假设组内成员可信
- **互斥覆盖只在 handler 内部一行**：`approved.insert + banned.remove` 是隐式不变式；后续 maintainer 若漏 `banned.remove` 会让设备同时在两个集合，处理顺序决定行为。v0 注释里有但 spec 没文档化
- **重启即清**与"用户期待"的微差：用户偶尔会问"我已经同意过这个设备啊"——其实重启了。这是产品设计选择，但 UX 没专门提示
- **chronic 网络抖动场景**：A trust C 后广播给 B，B 网络挂 → B 漏收 trust → C 主动连 B 仍走审批弹框（B 用户可能困惑"为什么不是直接连上"）。v0 接受最终一致性
- **broadcast_trust 同步等所有 peer**（`for h in handles { let _ = h.await; }`）+ 上游再裹 `timeout(2s)` —— 双重等：单 peer 的请求 timeout 是 reqwest 5s（build_client 默认），超过 2s 总时长会被外层 cancel；handles 里的 spawn 任务不会立即取消，留 zombie。v0 接受
- **subject 自己不会被发给**（`if peer.device_id == subject_device_id continue`）：但 subject 实际上**不在** peers 表里（刚握手成功时，broadcast_trust 紧接其后；subject = 新加入的设备已在 peers 表）—— 这条 if 是防御性，正确
- **TrustReq.subject_device_name 只在 log 里用**：用户层不展示；如果要让用户看到"X 已被 A 信任"通知，name 在协议里已就绪
- **multi-hop 不存在**：A trust C 直接 broadcast 给 B/D/E，B 不会再 broadcast 给其它（已是全连接 mesh，没意义；但若 mesh 不全则 trust 传播不到）—— v0 不防御 partial mesh 场景

### 5.3 v2 应继承
- `/peers/trust` + `/peers/ban` 双端点 + 共享 TrustReq DTO
- approved_device_ids / banned_device_ids 内存集合
- 互斥覆盖语义（trust 加 + ban 移；ban 加 + trust 移）
- ban 时 `was_peer = true` 立即踢出连接
- origin 必须已知 peer + seq dedupe + subject 不能是自己
- 不持久化（重启即清）
- broadcast_trust_decision 底层共享，broadcast_trust / broadcast_ban 薄封装
- broadcast 等所有 peer 完成 + 上游 `tokio::time::timeout(2s)` 限总时长
- 跳过 subject 自己（防御性）

### 5.4 v2 应挑战
- **trust 传染性安全审阅**：是否需要"trust 必须本机也确认" 的二次验证（接收 trust 时本机用户也要同意一次）？这与"任一同意全组生效"产品承诺直接冲突；属安全 + 产品 在 ADR 共商
- **互斥覆盖不变式必须明文 ADR**：approved/banned 永远互斥，进入哪个集合**必须**从另一个集合移除
- **subject_device_name 在 UI 暴露**（toast：`X 同意了 Y 加入，已自动信任`）—— 让用户感知到 gossip 在工作而非"莫名其妙不弹框了"。属 UX
- **broadcast 等所有 peer 与外层 timeout 双重等**：是否改为单层 `try_join_all` + 内层 timeout
- **partial mesh 场景**：A 与 B 连，A 与 C 连，但 B 与 C 未直连 → A trust D 时只能广播给 B 和 C；若 D 与 B/C 都没连过，B/C 收到 trust 时是否要主动 handshake D？v0 不做，trust 仅等 D 主动连过来。v2 可能升级为更主动
- **统一 module**：v0 trust 散在 server.rs（handler）+ client.rs（broadcast）+ state.rs（集合）—— 抽出 `network/trust.rs` 集中

## 6. UX 段（占位）

> 本 feature 主要是后端协议层；用户感知点仅在"为什么有时不弹审批框 / 为什么有时直接被拒"。第 6 节 N/A（无显式 UI 元素）；UX 风险已并入 第 7 节 给 ux-designer 评估是否需要 toast 提示 trust 状态变化。

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 3 条] [P1 3 条] [P2 2 条]

- [P0] [安全] trust 传染性：被攻陷成员可任意 trust 陌生设备入侵全组——是否需要"接收 trust 时本机用户二次确认"？与"任一同意全组生效"产品承诺直接冲突
- [P0] [安全] subject_device_name 是任意字符串 → 接收端若展示 toast 必须做与 device_name 同样的字符过滤（≤ 64 + 控制字符 + Unicode 反向覆盖字符）（与 `group-discovery` 第 7 节 / `group-approval` 第 7 节 同议题）
- [P0] [架构师] 互斥覆盖不变式必须 ADR 明文：trust 与 ban 集合永远互斥
- [P1] [安全] origin 校验仅查 peers 表是否足够？是否要求 origin 经过加密（如 TrustReq 也走 AES-GCM 加密 body 而不是明文 JSON）？v0 trust body 不加密
- [P1] [架构师] broadcast_trust 等所有 peer + 外层 timeout 双重等是否合并为单层 `try_join_all_with_timeout`
- [P1] [架构师] trust / ban 模块抽出 `network/trust.rs`：handler + broadcast + 集合管理集中（v0 散在 3 文件）
- [P2] [架构师] partial mesh 场景下 trust 传播不全：v2 是否在 trust 接收端主动 handshake subject（B 收到 A trust C 后主动 ping C）？v0 不做
- [P2] [UX] 用户是否需要可见反馈 "X 已被 A 信任"，或保持透明（v0 透明）

## 8. Review 段（占位）

> code-reviewer / tech-architect / security-reviewer 后续填写。本 feature 涉及信任状态机与组内传播路径，**必须**经 security-reviewer ACK（CLAUDE.md 第 9 节）。
