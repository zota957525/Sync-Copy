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
