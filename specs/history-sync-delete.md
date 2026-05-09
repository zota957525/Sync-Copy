---
status: SPEC_REVIEWED
owner: product-strategist
related_adrs: [ADR-003]
related_specs: [00-product-overview, history-list, group-discovery, e2e-encryption]
created: 2026-05-06
updated: 2026-05-08
revised: 2026-05-08 — ADR-003 第 3.3 节 PeerRegistry.seen_seq_and_update(kind=delete_history / clear_history) 共享 dedupe 机制
priority: P2
---

# history-sync-delete — 跨机同步删除某条历史 / 清空所有历史

## 1. 问题（为什么做）

`history-list` 让用户在每台机器上独立删条目——但用户的心智是"敏感内容（密码、验证码）应该从**所有**地方消失"。如果删条目仅本机生效，用户必须挨个机器去清，违反"两台机器像一台机器"的产品承诺（00 总览 第 1 节）。`history-sync-delete` 让"在 A 上点 ✕"等价于"在 A、B、C 全部删除"——一个动作，全组干净。

技术挑战：跨机识别同一条目要靠**内容哈希**（SHA-256 of plaintext，已在 `clipboard-text-sync` / `clipboard-image-sync` / `file-transfer-drag` 中算出并存进 `HistoryItem.content_hash`）。由于哈希基于明文，跨机器一致——这是设计选择（00 总览 第 5.2 节 已点明 metadata 泄露风险）。删除消息走两个端点：单条 `DELETE` 用 content_hash、清空用 `GroupActionReq` 无 payload。

本 feature 的 broadcast 与 `clipboard-text-sync` / `group-leave-notify` 共享 fire-and-forget 模式 + seq 去重模式。

## 2. 用户故事

- As a user copying a one-time password to share with myself, I want one click on its history row to delete it from **every** device in my group, so that I don't have to walk to the other machine and clean it manually.
- As a user wanting to start fresh after a long session, I want a single "Clear all history" button (in settings) to wipe history on all my devices simultaneously, so that "clean slate" is a single action.
- As a recipient of a sync-delete request from a peer, I want it applied silently without a popup or confirmation, so that my work is not interrupted (the original action was the consent—the peer's user is me too).

## 3. 范围

**in scope**：
- HTTP 端点 `/delete_history`（POST，body = `DeleteHistoryReq`）：
  - `DeleteHistoryReq { origin_device_id, seq, content_hash: String }`
  - handler `handle_delete_history`：
    - origin 在本机 peers 表 → 否则 403
    - `seen_seq_and_update(origin, seq)` 去重
    - `history.remove_by_hash(&content_hash)` → 返 `bool` 是否删到东西
    - 删到 → emit `history-updated` + tracing::info `remote delete applied`
    - 没删到（本机历史里没这条）→ 静默 200 OK（不算错）
- HTTP 端点 `/history/clear`（POST，body = `GroupActionReq { origin_device_id, seq }`）：
  - handler `handle_clear_history`：
    - origin 在本机 peers 表 → 否则 403
    - seq dedupe
    - `history.clear()` + emit `history-updated` + tracing::info
- 客户端 broadcast：
  - `broadcast_delete(state, content_hash)`：构 `DeleteHistoryReq` → for each peer spawn POST `/delete_history` → fire-and-forget（不 join，不计数；失败仅 warn log）
  - `broadcast_clear_history(state)`：构 `GroupActionReq` → for each peer spawn POST `/history/clear` → fire-and-forget
  - 两者均用 `build_client()` 的 5s/3s reqwest（与剪切板共用，区别于文件传输的 60s/5s）
- 触发点：
  - `commands.rs::delete_history_item(state, app, id)` 命令：`history.remove(&id)` → emit history-updated → 若 removed.content_hash 有值 → 异步 `broadcast_delete(state, hash)`（不 await，让前端 UI 即时返回）
  - `commands.rs::clear_history(state, app)` 命令：`history.clear()` + emit + 异步 `broadcast_clear_history(state)`（同样 fire-and-forget）
- 删除按 content_hash **批量**：`history.remove_by_hash(hash) -> bool` 用 `retain(|it| it.content_hash != Some(hash))` 移除所有匹配项（理论上同 hash 多条已被 push 时去重过，但残留场景仍兜底）
- 删除是**幂等**：重复广播（网络抖动重发）由 seq dedupe + `remove_by_hash` 自然幂等吸收
- 文件条目（saved_path 已落盘）的本地删除**不删磁盘文件**：仅删历史条目（用户文件主权）；广播也不要求对端删磁盘，仅删历史

**out of scope**：
- 跨机删除磁盘文件（用户落盘的文件不动；history 删除是历史条目层）
- 删除审计日志（"X 在 A 上删了 Y 条"）—— v2 不记审计
- 撤销删除（undo）—— 删了就删了
- 选择性广播（"仅在 A、B 删除，不在 C 删除"）—— 全 broadcast 或不 broadcast 二选一
- content_hash 缺失的条目跨机同步删除（v0：file_status = "failed" 且未算 hash 的条目删除时 `removed.content_hash` 为 None → 不广播；本机删除有效但其它机器同条不动）
- 用 device_id + timestamp 作为跨机标识（替代 content_hash）—— 增加协议复杂度且仍要求两端有共识
- 删除请求加密（DeleteHistoryReq 含 hash 但不含明文；仍是 metadata 暴露——同 `clipboard-text-sync` 第 5.2 节）
- 重放保护到密码学层（GCM AAD 绑 origin/seq/kind）—— 属 `e2e-encryption` 第 5.4 节 待 ADR 决策

## 4. 验收标准（Definition of Done）

- [ ] A、B 两机已 `小组 · 2 台`，A 复制了 3 条文本同步到 B（双方各有 3 条历史）。在 A 上点第 2 条的 ✕ → A 自身历史变 2 条 + 1 秒内 B 历史也只剩对应的 2 条
- [ ] 在 B 上点 settings → 清除历史 → A、B 同时变空（`小组 · 2 台`但历史 0 条）
- [ ] A 删一条不存在 content_hash 的条目（如发送失败的 file 条目，其 content_hash = None）→ A 本机删除生效 + 不广播给 B（B 历史不变）
- [ ] A 删条目时 B 网络挂了 → A 本机即时变 + B 上的对应条目仍在 → B 网络恢复后**不会**自动同步删除（fire-and-forget 设计选择，不重发）
- [ ] 陌生设备 X 向 B 发 `/delete_history { origin: ..., hash: <B 的某条 hash> }` → B 返 403，B 历史不变
- [ ] 同一删除请求 broadcast 重发两次（网络重传） → 第二次 seq 去重静默丢；本机已删的条目 `remove_by_hash` 返 false 也无副作用
- [ ] A 与 B 的 broadcast_clear_history 同时发生（罕见 race）→ 两机都清空 + emit history-updated 各自一次（无错乱）
- [ ] A 删除某文件条目 → A 与 B 历史中该条消失，但 B 上 Downloads 中已落盘的文件**不动**（用户文件主权）
- [ ] history.remove_by_hash 删除多个匹配条目（理论上 push 时已去重，残留场景兜底）—— 单元测试覆盖

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的
`legacy-prototype` 分支：

`network/protocol.rs` DTO：
- `DeleteHistoryReq { origin_device_id, seq, content_hash: String }`
- `GroupActionReq { origin_device_id, seq }`（与 leave / clear-history 共用）

`network/server.rs::handle_delete_history`（约 320-340 行）：origin 在 peers 表 → seen_seq_and_update → `history.remove_by_hash(&hash) → bool` → 删到 → emit history-updated + tracing::info；没删到也 200 OK 不报错。

`network/server.rs::handle_clear_history`（约 380-400 行）：相同流程 + `history.clear()` + emit + log。

`network/client.rs::broadcast_delete`（约 410-450 行）：构 DeleteHistoryReq → for each peer `tauri::async_runtime::spawn(async move { client.post(/delete_history).json(body).send().await })` —— fire-and-forget，不 join handles。失败仅 `tracing::warn`。

`network/client.rs::broadcast_clear_history`（约 285-320 行）：构 GroupActionReq → for each peer spawn POST /history/clear；同样 fire-and-forget。**有一处 `tracing::info!(peer = %peer_name, "clear-history broadcast ok")` 日志**（v0 commit `a09ef6c` 加进来调试用，已留在代码里）。

`commands.rs::delete_history_item(state, app, id)` 命令：
1. `history.remove(&id)` 取出（若不存在直接 return）
2. emit history-updated
3. 若 `removed.content_hash` 有值 → `tauri::async_runtime::spawn(async move { broadcast_delete(state_c, hash).await })` —— 不 await

`commands.rs::clear_history(state, app)` 命令：
1. `tracing::info!(peer_count, "clear_history invoked, will broadcast to peers")`
2. `history.clear()`
3. emit history-updated
4. spawn `broadcast_clear_history` —— 不 await

`history.rs::remove_by_hash(content_hash) -> bool`：retain 不匹配的 + 比对前后 len 判断是否删到。

### 5.2 v0 暴露的具体坑
- **content_hash 缺失的条目无法跨机删除**：file_status = "failed" 等场景没算 hash → 本机删 OK 但远端不动 → 用户感知不一致。v0 已知，没修
- **fire-and-forget 没 ack**：B 网络挂时 A 不知道；用户删除后看 A 干净，过去到 B 仍有 → 困惑。v0 选了"简单 + UI 即时反馈"
- **content_hash 是明文 SHA-256**：跨机器一致是优点，但**协议层暴露 metadata**（同 LAN 攻击者抓两台机器的 /delete_history → 知道这两条删除是同一内容）。安全 trade-off 已在 `clipboard-text-sync` 第 5.2 节 / `clipboard-image-sync` 第 5.2 节 列出
- **删除请求不加密**：DeleteHistoryReq body 是 `{origin_device_id, seq, content_hash}` 明文 JSON —— 仅暴露哈希（不可逆），但 LAN 内"哪些条目被删了"可被观察
- **broadcast_clear_history 多了一行 info log**：`peer_count` 在 commands.rs 也 log 一次；噪音
- **race condition**：A 与 B 同时删同一条 → 两边各自 broadcast → 各自收到对方广播 → 各自 remove_by_hash 返 false（已删）→ 静默丢。无问题但是隐式合并语义
- **删除文件条目的"文件主权"语义未文档化**：本机历史删 vs 磁盘文件不删，用户首次理解可能困惑
- **/history/clear 用 POST 但语义类似 DELETE**：HTTP 风格不一致（v0 全部 POST），代价是无法 curl `-X DELETE` 测；架构师 ADR 可统一

### 5.3 v2 应继承
- DTO `DeleteHistoryReq { origin_device_id, seq, content_hash }` 与 `GroupActionReq { origin_device_id, seq }`
- 两端点 `/delete_history` + `/history/clear`
- handler origin 校验 + seq dedupe + remove_by_hash / clear + emit
- broadcast fire-and-forget（不 join，UI 即时返回）
- delete_history_item / clear_history 两命令异步 spawn broadcast
- content_hash 缺失时不广播（本机删除 OK 即可）
- 删除幂等（重复广播 / 已删 hash 都无副作用）
- 文件条目本地删除不删磁盘（用户文件主权）

### 5.4 v2 应挑战
- **content_hash 缺失场景**：file_status = "failed" 等条目跨机同步删除是否升级？可用 `device_id + timestamp_ms` 作为补充标识（前提：跨机器时钟一致，未必）
- **fire-and-forget 是否升级 best-effort with retry**：A 删时 B 网络挂 → 是否在 A 端缓存 pending_deletes 表，等 B 心跳重连后补发？增加复杂度 vs 用户体验
- **content_hash = SHA-256(plaintext) 的 metadata 泄露对策**：HMAC(per-pair-key, plaintext) 让跨机 hash 不一致 → 但则**无法**跨机识别同条目 → 必须改用其它跨机标识（如 origin device_id + 原始 seq）—— 属安全 + 架构师在 ADR 共商
- **DeleteHistoryReq body 加密**：是否需要把 hash 也走 AES-GCM 包一层？v0 不加密；hash 不可逆但删除模式 metadata 仍泄露
- **HTTP 方法语义化**：`/delete_history` 用 DELETE 而非 POST；`/history/clear` 用 DELETE / POST 哪种—— 属架构师 ADR
- **文件条目本地删除 vs 磁盘文件**的语义必须明文写进 spec：仅删历史条目，不删 Downloads 中文件
- **delete after clear 的顺序保证**：用户先 clear-history（broadcast 飞行中）再 delete 单条 → 单条 broadcast 可能先于 clear-history 到达远端 → 远端先删单条（无副作用，已被 retain）再 clear（清空）→ 结果一致。无问题但是 spec 必须明文记录幂等 + 无序容忍

## 6. UX 段（占位）

> 本 feature 主要是后端协议层；UX 触发点已在 `history-list` 第 3 节 / `settings-panel` 第 3 节 中定义。本 spec 第 6 节 N/A（无独立 UI 元素）。

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 1 条] [P1 4 条] [P2 3 条]

- [P0] [安全] content_hash = SHA-256(plaintext) 的 metadata 泄露：与 `clipboard-text-sync` 第 7 节 / `clipboard-image-sync` 第 7 节 同一议题—— HMAC 替代会破跨机识别。决议直接影响本 feature 跨机标识方案
- [P1] [架构师] content_hash 缺失场景（file_status=failed 等）的跨机删除策略：device_id+timestamp 补充标识 vs 不支持
- [P1] [架构师] fire-and-forget 是否升级为 best-effort with retry：A 删时 B 网络挂的场景如何收敛
- [P1] [安全] DeleteHistoryReq body 是否加密（hash 不可逆但删除模式仍是 metadata）
- [P1] [UX] 删除前的二次确认（与 `history-list` 第 7 节 / `settings-panel` 第 7 节 重复，但本 feature 涉及跨机生效，确认对话需提示"将删除所有设备"）
- [P2] [架构师] HTTP 方法语义：用 DELETE 还是 POST？v0 全 POST 不一致
- [P2] [UX] file 条目本地删除不删磁盘文件的语义如何让用户感知（文案 / icon / hover 提示）—— 属 UX
- [P2] [架构师] `/history/clear` 与 `/peers/leave` 共用 GroupActionReq 是好的简洁还是会让 endpoint 语义模糊

## 8. Review 段（占位）

> code-reviewer / tech-architect / security-reviewer 后续填写。本 feature 涉及网络协议层 + content_hash metadata 泄露问题，必须经 security-reviewer ACK（CLAUDE.md 第 9 节）。
