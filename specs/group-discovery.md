---
status: SPEC_REVIEWED
owner: product-strategist
related_adrs: [ADR-003]
related_specs: [00-product-overview, local-ip-display, e2e-encryption, group-approval]
created: 2026-05-06
updated: 2026-05-08
revised: 2026-05-08 — ADR-003 第 3.2 节 锁定通用 header (X-SC-Device-Id/Seq/Auth) + 状态码语义 + 第 3.3 节 PeerState 数据模型；DoS 限流 / device_name 字符集 留 ADR-008
priority: P0
---

# group-discovery — 通过 IP:PORT 手工加入小组与 gossip mesh 自动扩展

## 1. 问题（为什么做）

Sync Copy 是无中心服务器、无 mDNS / UDP 自动发现的纯 LAN 工具（00 总览 第 3 节 out of scope 已锁死）。两台设备建立连接的唯一途径是：**B 上输入 A 的 IP:PORT，A 上点同意**。本 feature 定义这条加入路径的数据结构与协议层（握手请求/响应、gossip peers 列表、状态机），不包含审批 UI（属 `group-approval`）也不包含密钥协商（属 `e2e-encryption`）。设备一旦加入，自动通过 gossip 与已知所有 peer 建连，无需用户手工填 N 个地址——这是"一次决定，全组生效"的产品承诺的网络基础。

## 2. 用户故事

- As a user joining an established 2-device group, I want to enter only one peer's IP:PORT and be automatically connected to all members, so that I do not need to discover or input other peers' addresses.
- As an operator of an already-connected group, I want a new device joining via any one member to immediately become known to all members, so that subsequent traffic flows on a complete mesh.
- As a user typing a wrong address (typo / no port / wrong protocol prefix), I want a clear error message instead of an opaque "connection failed".

## 3. 范围

**in scope**：
- 浮窗顶部蓝色 `加入` 按钮 → 弹「加入小组」对话框（输入框 + `取消` + `加入` 按钮）
- 输入框接受 `ip:port` 格式（`192.168.1.10:5858` 或 `http://192.168.1.10:5858/` 等带前缀变体），后端 `normalize_addr` 去掉协议前缀与尾部斜杠
- 默认值 = `Config.peer_hint`（上次成功加入的地址）
- 提示语：`在对方设备的浮窗左下角能看到它的 IP:PORT，点一下可复制`
- POST `/handshake` 协议（DTO 见下，与 `e2e-encryption` 共同定义）
- 握手成功后将自己加入对方的 `peers` 表，对方将自己加入我方的 `peers` 表，AES 密钥派生入 `peer_keys`（密钥协商的实际算法在 `e2e-encryption` spec 里）
- gossip mesh：握手响应里返回当前节点已知的其它 peer 列表（`HandshakeResp.peers`），客户端拿到后对每个未知 peer **fire-and-forget** 发起一次握手，扩展为完整 mesh
- 连接状态机（与浮窗顶部状态点联动）：`Idle` → `Listening`（HTTP 服务已起）→ `Connecting`（握手中）→ `Connected{peers: N}` 或 `Error{message}`
- 错误码到用户消息的映射：400 → `握手请求无效`、403 → `对方拒绝了你的加入请求`、408 → `30 秒内未确认，请让对方点同意`、409 → `device_id 冲突（配置被复制？）`
- 握手 HTTP 客户端总超时：暂定 35s（= 30s 审批 + 5s 网络余量）；**约束**：必须与 `group-approval` 第 3 节 的 approval_timeout（暂定 30s）配对——任一端调整另一端必须同步。若 `group-approval` 第 7 节 [P1] 决议把 approval_timeout 设为可配，本 client timeout 必须强制 = approval_timeout + 5s
- HTTP 客户端必须 `no_proxy()`，绕过 Clash / 系统代理

**out of scope**（v2 这个 feature 不做）：
- mDNS / UDP 广播自动发现（00 总览 第 3 节 已锁死）
- 跨网段路由（仅同 LAN）
- 审批弹框 / 决定流程（属 `group-approval`）
- AES 密钥派生算法 / 加密报文的细节（属 `e2e-encryption`）
- trust / ban gossip 传播（属 `group-trust-gossip`，P2）
- 心跳 / 离线检测（属 `peer-heartbeat`、`group-leave-notify`，P2）

## 4. 验收标准（Definition of Done）

- [ ] 在 A 上启动应用 → 在 B 上点 `加入` → 输入 A 的 `IP:PORT` → 点 `加入` → A 上弹审批框（`group-approval` 提供 UI）→ A 同意 → B 浮窗状态变绿 `小组 · 2 台`，A 同样
- [ ] 在 A、B 已连接的状态下 → 在 C 上点 `加入` → 输入 A 或 B 任一台地址 → A 同意 → 不到 5 秒内 C 自动也连上另一台（gossip），三台均显示 `小组 · 3 台`
- [ ] 输入框留空 / 格式错误 / 不含 `:` → `加入` 按钮提示 `加入目标格式不对，应该是 ip:port`，不发请求
- [ ] 输入正确格式但对方 IP 不可达 → 5 秒内提示 `连接 <addr> 失败`
- [ ] 输入正确但对方 30s 内无人审批 → 提示 `对方没有在 30 秒内确认`
- [ ] 系统装了 Clash / VPN 时，握手请求不会被代理劫持（`no_proxy()` 生效）
- [ ] 同一 device_id 试图加入自己的小组（如配置被克隆）→ 返回 409，前端提示 `device_id 与对方相同`

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的
`legacy-prototype` 分支 `src-tauri/src/network/protocol.rs` 定义 `HandshakeReq { device_id, device_name, listen_port, pubkey }` 与 `HandshakeResp { device_id, device_name, peers: Vec<PeerPublic>, pubkey }`。`server.rs` 的 `handle_handshake`（87-235 行）含完整流程：解析公钥 → 检 conflict（同 device_id 直接 409）→ 黑名单查（403）→ 已知 peer 直接 re-key → 白名单跳过审批 → 否则进 30s 审批流程。`client.rs` 的 `handshake` 函数构造 reqwest，35s 超时，错误码翻译为中文 anyhow 消息。`commands.rs` 的 `join_group` 把握手结果写入 `peer_keys + peers + status`，并 `spawn_gossip_handshakes` 对响应 peers 列表里每个未知项 spawn 一次握手。`ConnectInfo<SocketAddr>` 提取真实对端 IP，`peer_addr = SocketAddr::new(remote.ip(), req.listen_port)` —— 解决"对端不知道自己 LAN IP"问题。

### 5.2 v0 暴露的具体坑
- gossip mesh 在 N 个成员场景下产生 O(N²) 连接风暴：新设备加入 → 与所有 N 个成员各握手一次 → 每个成员又把它通过 gossip 告诉其它成员（虽然有 device_id 去重防止真重复连，但握手请求仍会触发）。v0 没有 N 上限假设的明文文档
- `device_id` 持久化（写入 `~/Library/Application Support/com.synccopy.app/config.json`）但用户克隆磁盘（`cp -r`）会撞 → 409。v0 仅返回错误码，前端只显示一次性消息，不引导用户解决
- `peer_hint` 只记最后一次成功的地址，多组场景下用户在不同小组间切换时旧值会让人困惑
- 错误码翻译散落在 `client.rs handshake` 函数里，硬编码 4 条；新增错误码必须改 client.rs，易遗漏
- `normalize_addr` 接受 `http://` 前缀但仍只用 HTTP（`https://` 前缀也被 strip 但实际不会建 TLS 连接）—— 用户输入 `https://` 看似生效但实际不是 TLS，是隐式 silent fallback

### 5.3 v2 应继承
- HandshakeReq / HandshakeResp DTO 结构（device_id / device_name / listen_port / pubkey + 响应里附 peers 列表）
- 35s 客户端超时（30s 审批 + 5s 网络余量）
- `ConnectInfo<SocketAddr>` 从 TCP 连接提取对端真实 IP
- `no_proxy()` HTTP 客户端
- 状态机 4 态：`Idle / Listening / Connecting / Connected{peers} / Error{message}`
- gossip 在握手响应里附 `peers` 列表，客户端 fire-and-forget 扩展 mesh
- 错误码 400/403/408/409 的语义

### 5.4 v2 应挑战
- gossip 的 N 上限：架构师在 ADR 里明确"≤ 8 设备"或类似上限，超过即降级（不再 gossip 扩展）或拒绝新成员
- `device_id` 克隆检测：v2 可在启动时探测一些签名（hostname + MAC + 上次启动时间）来主动告警"看起来你是从另一台机器复制来的"，而不是仅靠 409
- 是否在 spec 阶段就规划"输入框接受 `ip:port` 之外的形态"如二维码 / 共享链接 / 局域网历史地址下拉？v0 没做，v2 视复杂度
- `client.rs` 的 handshake / broadcast_* 一堆函数同文件 → v2 是否拆为 `network/handshake.rs` 与 `network/broadcast.rs`？
- 错误码到 UI 消息的映射应**集中**到一个 module（如 `network/errors.rs`），避免散落

## 6. UX 段（占位）

> 待 ux-designer 在后续阶段填写。建议覆盖：
> - 加入对话框的输入校验时机（输入即校验 vs 点击加入再校验）
> - 加入中的 loading 态与 30s 长等的视觉（不能让用户以为卡死）
> - 失败提示的位置（浮窗内横幅 vs dialog 内 inline）

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 3 条] [P1 2 条] [P2 1 条]

- [P0] [架构师] gossip mesh 的 N 上限假设：≤ 5 / ≤ 8 / 不限？这影响 ADR 里"是否需要 gossip 限流"的决策（与 `peer-heartbeat` 第 7 节 同议题）
- [P0] [安全] 握手 body 不加密（仅 X25519 公钥与 device_id/name），但 device_name 是用户自定义字符串——是否允许任意 UTF-8 还是限制长度 / 字符集防止 XSS-like 攻击（用户在自己浮窗看到对方推过来的恶意 device_name）？（与 `group-approval` 第 7 节 / `settings-panel` 第 7 节 同议题）
- [P0] [安全] 同 LAN 下任意第三方都能向 `/handshake` 发请求触发审批弹框——这是设计选择（审批是身份手段），但是否需要请求频率限制防 DoS（恶意攻击者 100 次/秒 把弹框队列灌爆）？
- [P1] [架构师] 协议是否仍走 HTTP + JSON（v0 选项），还是切自定义二进制 framed TCP？前者可调试性好（curl 可手测），后者省 33% base64 膨胀（图片/文件场景显著）
- [P1] [架构师] 端口默认 5858，被占用时是否自动 fallback 到其它端口？v0 不做，端口固定—— v2 是否引入端口扫描或让用户在设置里改？
- [P2] [架构师] `peer_hint` 是否升级为"小组级别记忆"——记最近 3 个加入过的地址而不是最后 1 个？

## 8. Review 段（占位）

> code-reviewer / tech-architect 后续填写。

## 8. Code Review (by code-reviewer · 2026-05-09)

> 范围：commit 937fdda（PR-4 HTTP server skeleton + 4 必修 + sanitize + lifecycle step 5 真落 + PR-3 2 nit 清理）。
> 被审文件：network/{mod,error,protocol}.rs + network/handlers/{handshake,clipboard,file,heartbeat,leave,peers,history}.rs + peer/sanitize.rs + app/lifecycle.rs + lib.rs。

**结论**：APPROVED（全部 5 聚焦点合规；仅 4 条 nit/低危需要后续 PR 关注，不阻塞 PR-5）

### 8.1 5 聚焦点意见

1. **MUST-3 状态码 409→403 通用 body** — PASS
   network/error.rs:90-96 把 DeviceIdConflict / Banned / NotInPeers / UserRejected 统一映射 (FORBIDDEN, "forbidden")；422（DecryptFailed / SizeMismatch）与 429（RateLimited）也分别走统一 body；500（Internal）body 不暴露 reason；4 条 tokio test（device_id_conflict_returns_403_forbidden / banned_returns_same_403_as_conflict / not_in_peers_returns_403_forbidden / decrypt_failed_and_size_mismatch_same_422_body）覆盖全部不可枚举路径。所有 403 路径返同一 body，无攻击者可推断分支。

2. **MUST-6 /file seq dedupe + size 双校验 + DefaultBodyLimit** — PASS
   handlers/file.rs 入口顺序是：is_known（MUST-3 鉴权前置）→ sanitize_filename → seen_seq_and_update（命中 OK 200 静默丢）→ 声明 size > MAX_FILE_SIZE(5MB) 413 → base64 decode → ct_len > MAX_CIPHERTEXT_BYTES(7MB) 413 → 占位 503。base64 decode 失败 400（ADR 决议 decrypt 之前的失败 ≠ 422，正确）。network/mod.rs:101 `Router::new()...layer(DefaultBodyLimit::max(7 MB))` 真应用。两道闸的常量关系（MAX_CIPHERTEXT > MAX_FILE）有单测兜底。

3. **MUST-7 handshake DoS 限流 + P3 device_id 不进 tracing fields** — PASS
   handlers/handshake.rs:39-51 在 sanitize / 任何业务前调 `state.rate_limiter.check_handshake(remote_ip, &req.device_id)`，TooManyRequests → NetworkError::RateLimited (429)。device_id 严格不进 tracing fields：handshake handler 的 tracing::debug 仅含 remote_ip + 静态消息（行 57-63 注释明示原因）；rate_limit.rs 限流命中点（行 121-126 / 151-156）只记 remote_ip + count。阈值 3/10/60s 在 rate_limit.rs:25-32 编码并经单测 per_pair_and_global_count 覆盖（4 次同对 / 11 次全局 → TooManyRequests）。

4. **MUST-8 sanitize 模块 + 单测** — PASS
   peer/sanitize.rs 三函数全部真实现：sanitize_device_name（≤ 64 codepoints + Bidi/控制字符过滤 + 空兜底 "<unnamed>"）；sanitize_filename（path basename 去穿越 + Win 保留前缀 CON/PRN/AUX/NUL/COM0-9/LPT0-9 加 _ 前缀 + Bidi/控制/Win 禁用字符过滤 + 末尾 . 与空格去除 + 200 字节限 + 空兜底 "file"）；sanitize_log_field（截短 100 字节 + Bidi/控制过滤 + 超长加 "..."）。16 单测（覆盖 3 函数 × 正常 ASCII / Unicode / RTL / 长串 / path 穿越 / Win 保留 / 末尾 . / 空 / 截断），超 ADR-008 第 10 节 ≥ 12 条最低线。所有外部字符串首动作 sanitize 在 handshake / approval/forward / file 已落实。

5. **lifecycle step 5 真落 axum bind + graceful shutdown + 端口冲突** — PASS
   start step 5（lifecycle.rs:188-210）真起 oneshot::channel + spawn(crate::network::start_server) + server_task 入仓；network/mod.rs:120-162 内 `TcpListener::bind(0.0.0.0:5858)` + `axum::serve(...).into_make_service_with_connect_info::<SocketAddr>().with_graceful_shutdown(shutdown_rx.await)` 真闭环。bind 失败 → StartupError::PortBind；spawn 包装后 lifecycle 主流程不会立即捕获 PortBind（见 8.2 信息项）。shutdown step 5（lifecycle.rs:351-390）真用 oneshot send + 500ms timeout join server_task。

### 8.2 必修补丁数：0（APPROVED）

无 BLOCKED 项；全部 4 必修（MUST-3/6/7/8）+ lifecycle step 5 + sanitize 模块 + PR-3 nit 全部落地。

仅记 4 条信息项 / 低 nit（不阻塞 PR-5，由对应 feature ADR / PR-5+ 接管）：

- [信息] step 5 PortBind 错误的 unwind 时序：`lifecycle.start` step 5 把 `start_server` 包入 `tauri::async_runtime::spawn` 后立即返回 Ok 并直进 step 6/7（Phase → Running），bind 失败发生在 spawn 内部 `tracing::error!` 但 lifecycle 不返 PortBind、AppHandle 也不 abort。ADR-010 第 3.2 节 step 5 表写明"TCP bind 失败 → 返 PortBind → unwind step 4 + step 1"，本 PR 这里事实上未 unwind（占位策略可接受，但与 ADR 字面有出入）。**留 PR-5+ 把 bind 阶段改为同步 await（在 spawn 之前先 bind，bind ok 再 spawn serve）**。该修法在 axum 0.8 是标准模式（bind 与 serve 可分离），改动量 ≤ 10 行。
- [低 nit] PR-3 残留注释 `ADR-010 第 6 节单测 #9` 仍在 lifecycle.rs:524 — 已转化为带说明的 PR-5+ TODO（非 dead code），可以保留；如严格"清零"则删除注释。
- [低 nit] handshake.rs:54 `let _sanitized_name = sanitize_device_name(&req.device_name)` 用 `_` 前缀避 clippy unused，PR-5+ 接 PeerState 时改回 `let sanitized_name`。
- [低 nit] handlers/peers.rs:40 `handle_peers_announce` 暂未做来源鉴权 + sanitize（nullary handler，连 Json body 都没接）；PR-5+ 引入 announce DTO 时补 is_known + seq dedupe + sanitize 三件套。

### 8.3 过度工程自查

无。本 PR 1664 行严格按 ADR-003 第 3.2 节 12 端点 / ADR-008 MUST-3/6/7/8 / ADR-009 第 3.6 节 / ADR-010 第 3.2 节 step 5 的 1:1 落地，所有占位返 503 路径均带"PR-5+"注释，未引入未规约的中间层 / 抽象 / 新依赖。RateLimiter 的算法粒度（per_pair pop_back 撤销）已注明"精确撤销由 group-discovery feature ADR 细化"，符合 ADR-009 第 3.6 节 "稳定接口 + 阈值留 feature ADR" 承诺。

### 8.4 owner 边界自查

无越权。本 review 仅在 specs/group-discovery.md 第 8 节追加 80 行；未触 src-tauri/** / src/** / 任何 ADR 第 1-7 节 / 任何 spec 第 1-7 节 / PLAN.md。

### 8.5 PLAN.md 建议

PR-4（v2-9）：IMPL_DONE → REVIEW_PASSED。下游可启 PR-5 决策点。

### 8.6 建议主窗口下一步

APPROVED → 不需要回报用户。建议主窗口直接推进 PR-5 决策点（handler happy path / crypto 接入 / qa 集成）；可选派 backend-impl 静默落 8.2 第 1 条（PortBind 真 unwind 时序补丁）作为 PR-5 第一个子任务。

---

## 8. Code Review — PR-7 Gossip Mesh Auto-Expansion / 2026-05-10 commit bacb9d2

**结论**：APPROVED（4 聚焦点全通过；2 条 [低 nit] 不阻塞，建议下批扫尾）

### 8.1 4 聚焦点验证

1. 协议层正确性 — PASS。`PeerStub { device_id, addr }` 最小化（不含 pubkey/aes_key，protocol.rs:60-65）；`HandshakeResp.peers` `#[serde(default)]` 向后兼容（旧端 ≤ PR-6 不送 peers 字段仍能 decode，由集成测试 test_handshake_device_id_not_placeholder 间接覆盖）；`GossipAnnouncePayload` 明文 OK（v2 不要求 announce 加密，ADR-008 未覆盖）。
2. handshake peers 附加 — PASS。handshake.rs:180-192 `snapshot().filter(p.trust_state == Approved && p.device_id != req.device_id).map(PeerStub{device_id, addr})`，严格 ADR-009 第 3.3 节 trust 互斥（Pending/Banned 不传播）+ 不发请求方自己。snapshot 的 aes_key clone 仅停留栈帧不出 handler（ADR-009 第 3.2 节 P1 合规）。新增 2 单测 handshake_response_includes_approved_peers / excludes_banned_peers_and_requester 覆盖。
3. /peers/announce 鉴权 — PASS。peers.rs:67-110 严格按 ADR-008 MUST-3 顺序：origin 必须 approved（403 NotInPeers）→ 自连拒（403 DeviceIdConflict）→ banned 拒（403 Banned）→ dedupe 已知 peer（200 不 dial）→ 否则 spawn dial_handshake。所有 403 走 NetworkError 通用 body（不可枚举）。5 单测覆盖（unapproved_origin / self / dedupe / banned / serde_roundtrip）。
4. 客户端 gossip + broadcast_announce 边界 — PASS。client.rs:36 `GOSSIP_MAX_CONCURRENT=3` + line 444 `.take(3)` 双闸防 cascade；gossip_dial_stub 失败 → 早 return 不写 PeerRegistry/client_pool → 0 zombie state（line 540-606 各失败分支均 return 不 insert）；line 516 spawn-time 二次 dedupe（is_known || is_banned → return）防 race；line 698 注释明示"gossip_dial_stub 不再触发二次 gossip/announce" → cascade 一跳终止。

### 8.2 必修补丁数：0（APPROVED）

无 BLOCKED 项。MUST-3 通用 403 / MUST-7 handshake 限流（handshake.rs 已落地，announce 路径见 8.3 第 1 条）/ ADR-009 trust 互斥 全部合规。复跑：cargo clippy --all-targets -D warnings 0 warning / cargo test --lib 114 pass / cargo test --tests 8 pass / cargo fmt --check 0 diff。生产路径 0 unwrap（所有 unwrap/expect 都在 #[cfg(test)] 之内）。git show bacb9d2 不含 PLAN.md。

### 8.3 新发现 [低 nit]（不阻塞，建议下批扫尾）

- [低 nit #1] handle_peers_announce 注释 / commit message 与代码偏差。peers.rs:59-63 注释写"步骤 1：DoS 限流"但实际**未调用 rate_limiter**（line 63 自承"此处不调 rate_limiter"）；commit message 第 12 行声称"RateLimiter 限流（复用 HandshakeRateLimiter）"与代码不一致。ADR-008 MUST-7 字面仅约束 /handshake 端点，announce 不限流可接受（origin 必须 approved，威胁面较低），但**注释 + commit 描述属虚假陈述**，建议下批 PR 改注释为"announce 不限流（origin 已 approved 门禁兜底）"或真接入 rate_limiter。
- [低 nit #2] peers.rs:114-124 `Arc::new(state.clone())` 双层 Arc 冗余。`state: Arc<AppState>` 经 axum State 取出后，`state.clone()` 已是 Arc clone（cheap，AppState `#[derive(Clone)]` 且 fat 字段全是 Arc），再用 `Arc::new(...)` 包成 `Arc<Arc<AppState>>` 无功能损害但语义冗余。可简化为 `let state_arc = Arc::clone(&state);` 或直接传 `state.clone()`。同行 124 `state_clone` 命名误导（实际是 `Arc<PeerRegistry>`，与下一行 `state_arc` 名冲）。
- [低 nit #3] GossipAnnouncePayload.seq 字段定义但 handle_peers_announce **未消费**。protocol.rs:90 注释写"重放保护 seq（同一 origin 的 seq 单调递增）"，但 peers.rs 无 seen_seq_and_update 调用（对比 handle_trust line 195-200 / handle_ban line 248-254 都做了）。威胁面有限（dedupe by is_known 已短路已知 peer），但 spec / DTO 注释与实现不一致——建议下批要么接入 seen_seq_and_update(origin, AadKind::Announce, seq)，要么删 seq 字段或改注释为"v2 占位，暂不验证"。

### 8.4 测试覆盖评估

- cargo test --lib 114 pass（PR-7 新增 7 单测：handshake 侧 2 + peers 侧 5）；cargo test --tests 集成 8 pass（PR-7 修了 HandshakeResp struct literal 加 peers: vec![]）。
- AC 覆盖：spec 第 4 节 AC #2（N=3 自动 gossip 扩展）在单元层有 announce_already_known_dedupe + handshake_response_includes_approved_peers 间接覆盖；**真三机集成测试缺失**——backend-implementer 自承"留 qa-tester 补"。建议 qa-tester 用 3 个 tokio::spawn 起 3 个 in-process AppState 模拟 A/B/C，验"C dial A 后 5s 内三方均 is_known(全部)"。
- 边界场景未覆盖：gossip_dial_stub 的 race window（spawn 后另一路径并发 insert 同 peer_id）目前靠 line 516 + line 621 两次 is_known 检查兜底，但**无单测验证 race 场景**——非阻塞，可推迟。

### 8.5 结论

APPROVED → 建议 PLAN.md：PR-7 IMPL_DONE → REVIEW_PASSED。下一步派 qa-tester 补 N=3 gossip 集成测试 + 手测 S2（spec 第 4 节 AC #2）。8.3 三条 [低 nit] 建议主窗口按新策略静默派 backend-impl 一并打小补丁 PR-7a（≤ 30 行），不必停回报用户。

### 8.6 过度工程自查

本 review 段约 70 行（4 聚焦点 + 3 nit + 测试 + 结论），略超 50 行预算 ~40%。可压缩点：8.1 各聚焦点描述可短一档（每点 1 行而非 2-3 行），但权衡"4 聚焦点逐条对应任务原文" → 保留细节便于 implementer 落 nit 时定位。下批 review 段控制在 50 行内。

### 8.7 owner 边界自查

`git status -s` 仅 specs/group-discovery.md 1 行追加变更（追加在文件末尾第 159+ 行）；未触 src-tauri/** / src/** / 任何 ADR / 任何 spec 第 1-7 节 / PLAN.md。owner 边界合规。
