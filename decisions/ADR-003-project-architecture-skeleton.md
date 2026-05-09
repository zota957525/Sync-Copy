---
id: ADR-003
feature_id: project-skeleton
title: 项目层架构骨架 — 模块切分 / HTTP 协议骨架 / PeerState / 加密层抽象 / lifecycle owner / 错误日志总策略 / 隐形掉线机制
status: ACCEPTED
owner: tech-architect
date: 2026-05-08
accepted_at: 2026-05-08
deciders: [tech-architect, main, user]
security_signoff: ADR-008 (ACCEPTED 2026-05-08, CHANGES_REQUESTED with 8 must-fix items)
user_decision_summary: 7/7 项目层决策卡片用户 2026-05-08 全选 B；ADR-008 3 张必修确认卡片 2026-05-08 全选 A（接受 8 必修 + 3 不必修议题边界）
related_specs:
  - 00-product-overview
  - clipboard-text-sync
  - clipboard-image-sync
  - file-transfer-drag
  - group-discovery
  - group-approval
  - e2e-encryption
  - peer-heartbeat
  - group-leave-notify
  - group-trust-gossip
  - history-list
  - history-sync-delete
  - settings-panel
  - floating-window
  - floating-ball
  - tray-integration
  - local-ip-display
  - cross-platform-build
  - diagnostic-logging
  - clipboard-snapshot-sync
related_adrs:
  - ADR-001
  - ADR-002
supersedes: []
superseded_by: []
revision_history:
  - version: v1
    date: 2026-05-08
    notes: 初版项目层骨架 ADR — 用户指定 A 单 ADR 模式 + 决策卡片清单（PLAN.md P2-1.a）
  - version: v1.1
    date: 2026-05-08
    notes: 用户对 7 张决策卡片全选 B；status PROPOSED → ACCEPTED_PENDING_SECURITY_SIGNOFF；deciders 加 [main, user]
  - version: v1.2
    date: 2026-05-08
    notes: ADR-008 ACCEPTED + 用户对 ADR-008 3 张必修卡片全选 A；status ACCEPTED_PENDING_SECURITY_SIGNOFF → ACCEPTED；P2-1.a 完成；P2-1.b 解锁
depends_on_artifacts:
  - path: specs/00-product-overview.md
    version: 2026-05-06
  - path: specs/_assumptions.md
    version: 2026-05-08（APPROVED_WITH_REVISIONS）
  - path: docs/handoff-lessons-learned.md
    version: 2026-05-08
  - path: specs/clipboard-text-sync.md
    version: 2026-05-06
  - path: specs/clipboard-image-sync.md
    version: 2026-05-08（v2）
  - path: specs/file-transfer-drag.md
    version: 2026-05-08（v2）
  - path: specs/peer-heartbeat.md
    version: 2026-05-08（v2）
  - path: specs/e2e-encryption.md
    version: 2026-05-06
  - path: specs/diagnostic-logging.md
    version: 2026-05-06
  - path: specs/clipboard-snapshot-sync.md
    version: 2026-05-06
  - path: src-tauri/Cargo.toml
    version: 现状（v0 留底）
  - path: src-tauri/tauri.conf.json
    version: 现状
  - path: package.json
    version: 现状
---

# ADR-003 — 项目层架构骨架

> 范围：本 ADR 是 v2 重写的"骨架决策"——所有 feature 层 ADR（ADR-004+，对应 P2-1.b）共享的项目层不变式。本 ADR 不替代任何 feature ADR，但 feature ADR 不得违反本 ADR 第 3 节里的任何项目层决议；如确需违反，feature ADR 必须显式 supersede 本 ADR 对应子节。

---

## 1. 上下文（Context）

### 1.1 触发本次决策的输入

- **20 份 feature spec** 全部 SPEC_DRAFTED（含 P1-7 重审后的 3 份 v2 升级：`file-transfer-drag` / `clipboard-image-sync` / `peer-heartbeat`）
- **`specs/_assumptions.md` APPROVED_WITH_REVISIONS**（2026-05-08 用户校对完成）：3 处事实层修正（A2 切换频率 10-100 次/天、A14 非 PNG 路由、A16 文件上限 5 MB）+ 1 条 v0 实战 bug（A_BUG_HIDDEN_DEAD 隐形掉线）
- **`docs/handoff-lessons-learned.md`**：第 1 段（30 秒引导）+ 第 4 段（v0 踩坑分类，含网络 / 剪切板 / Tauri / Rust 4 大类）+ 第 8 段（反风控约束）
- **`specs/00-product-overview.md` 第 5.4 节** 列出 v0 必须挑战的 10 项设计反模式（轮询 / 单文件膨胀 / 上帝 struct / 锁粒度 / 协议 base64 膨胀 / 内存暴涨等）
- **3 条 [P1] [架构师] 议题**（PM 在 P1-7 修订时入档）：① 非 PNG 路由（_assumptions A14 联动）② 隐形掉线参数 N/M/keepalive 三组 ③ OS 光栅化非 PNG 为 PNG 的边界判定
- **现状依赖**：`src-tauri/Cargo.toml`（tokio full / axum 0.8 / reqwest 0.12 默认 TLS / arboard 3 / image 0.25 仅 PNG / aes-gcm 0.10 / hkdf 0.12 / x25519-dalek 2 / parking_lot / tracing / if-addrs 0.13）；`package.json`（SvelteKit 2 + Svelte 5 runes + adapter-static SPA）；`tauri.conf.json`（main 窗口 320×420 + transparent + alwaysOnTop + macOSPrivateApi）

### 1.2 现在不决会有什么后果

- 没有项目层骨架 ADR，每个 feature ADR 都要重新论证模块切分 / 协议形式 / 加密 trait —— 决策重复 + 不一致 + 用户拍板成本暴增
- v0 教训第 5.2 节明确：M3→M4→M4 X25519+ECDH 三次演化没有 ADR，导致代码层"为什么这样"无源——这正是 v2 必须在第一刀避免的反模式
- _assumptions A_BUG_HIDDEN_DEAD（隐形掉线）是项目级阻塞 bug：不在 ADR 层决定参数（N / M / keepalive），feature 层 `peer-heartbeat` ADR 无法独立给出答案（涉及与 client.rs 的 retry 计数、reqwest 连接池、`PeerState` 字段三处联动）
- 模块切分、HTTP 端点列表、加密 trait 边界、lifecycle owner 等是横切关注（cross-cutting），feature 层 ADR 无法独立决定

### 1.3 为什么用单 ADR 而非 6-8 个分散 ADR

用户在 PLAN.md P2-1.a 拍板选 A：单 ADR 模式 + 末尾决策卡片清单。理由（用户原话+本节 PM 重述）：

- 项目层骨架 7 个子决策互相耦合（如"模块切分"决定"HTTP 协议骨架"放哪、"PeerState 数据模型"决定"加密层抽象边界"）；切散后跨 ADR 引用爆炸
- 单 ADR + 末尾决策卡片可让用户 5 分钟内一次拍完 7 个子决策，避免 7 次会话 7 次回到上下文
- 后续 feature ADR（P2-1.b）每次只需引用 ADR-003 一处，简化 cross-reference

---

## 2. 选项考虑（Options Considered）

> 项目层 7 个子决策点各自有 ≥ 2 选项；本节按"3.1 → 3.7"顺序逐个列。每个选项含怎么做 / 优点 / 缺点 / 跨平台风险 / 实现复杂度。

### 2.1 模块切分 — 后端 / 前端拆分粒度

#### 选项 A：极薄拆分（v0 + 1 层）

后端：在 v0 现状基础上，仅把 `network/server.rs`（v0 784 行）拆成 `handshake.rs / clipboard.rs / file.rs / approval.rs / gossip.rs / health.rs`，其它（`crypto.rs` / `clipboard.rs` / `state.rs` / `commands.rs` / `config.rs` / `history.rs`）保持单文件；前端把 `+page.svelte`（v0 1483 行）拆成 6 个组件（StatusBar / HistoryList / FloatingBall / ApprovalDialog / SettingsPanel / JoinDialog）+ 1 个 ipc 工具文件。

- 优点：风险最低 + 与 v0 概念映射 1:1，代码搬迁成本最低；改动范围可控
- 缺点：依然有 `state.rs` 的 AppState 上帝结构（v0 教训 5.4 已点名）；`commands.rs` 仍是大杂烩
- 跨平台风险：无（仅文件拆分）
- 实现复杂度：低
- 与 spec 关系：满足 `floating-window.md` 5.4 / `00-product-overview.md` 5.4 的"前端组件 ≥ 6 个"硬约束最低门槛

#### 选项 B：分层 + 域驱动拆分（推荐）

**后端 `src-tauri/src/`**：

```
src-tauri/src/
├── main.rs                      # Tauri 启动器，仅 init + run
├── lib.rs                       # 库入口；register commands + start runtimes
├── app/
│   ├── mod.rs
│   ├── state.rs                 # AppState 顶层聚合（仅持有子域 Arc）
│   └── lifecycle.rs             # 启动 / 关闭 / quit_app 编排
├── config/
│   ├── mod.rs
│   └── persistence.rs           # Config 读写 ProjectDirs json
├── crypto/
│   ├── mod.rs                   # trait 定义（KeyExchange / Sealer / Verifier）
│   ├── x25519.rs                # KeyExchange 实现
│   └── aes_gcm.rs               # Sealer 实现
├── clipboard/
│   ├── mod.rs                   # spawn(app, state) 入口
│   ├── poll.rs                  # 轮询循环（image 先 / text 后）
│   ├── encode.rs                # PNG 编解码 + last_text/last_image_hash 状态机
│   └── cmd.rs                   # ClipboardCmd enum + mpsc 通道
├── history/
│   ├── mod.rs                   # History 类型 + push/remove/clear
│   └── dedup.rs                 # content_hash 去重 + retain 逻辑
├── peer/
│   ├── mod.rs                   # PeerRegistry（统一持有 peers / peer_keys / fail_counts / last_successful_sync_at / approved / banned）
│   └── state.rs                 # PeerState struct（详见 3.3）
├── network/
│   ├── mod.rs
│   ├── protocol.rs              # 所有 DTO（HandshakeReq/Resp / ClipboardReq / FileReq / TrustReq / GroupActionReq / DeleteHistoryReq）
│   ├── server.rs                # axum router + 共享 middleware；handler 委派
│   ├── handlers/
│   │   ├── handshake.rs
│   │   ├── clipboard.rs         # /clipboard text + image_png 两 kind
│   │   ├── file.rs
│   │   ├── approval.rs          # /peers/approval/{forward,decide,dismiss}
│   │   ├── gossip.rs            # /peers/{trust,ban,leave}
│   │   ├── history.rs           # /delete_history + /history/clear
│   │   └── ping.rs
│   ├── client.rs                # broadcast_text / broadcast_image / broadcast_file / broadcast_trust / etc.
│   ├── client_pool.rs           # reqwest::Client 工厂（短超时 / 长超时 / 心跳三套）
│   ├── health.rs                # PING_INTERVAL / FAIL_LIMIT / 强制重连 (见 3.7)
│   └── lan_ip.rs                # if-addrs 枚举 + 过滤 + 优先级
├── commands/
│   ├── mod.rs
│   ├── group.rs                 # join_group / leave_group / quit_app
│   ├── clipboard.rs             # recopy_history_item
│   ├── files.rs                 # send_files / respond_file_save / reveal_file
│   ├── history.rs               # get_history / delete_history_item / clear_history
│   ├── config.rs                # get_config / set_config
│   └── system.rs                # get_local_ip
└── log/
    ├── mod.rs                   # tracing init + file appender + rotation
    └── export.rs                # diagnostic-logging 导出 zip
```

**前端 `src/`**：

```
src/
├── app.html
├── routes/
│   ├── +layout.ts               # ssr=false
│   └── +page.svelte             # 仅 view 切换 (main / settings / join / ball)
├── lib/
│   ├── components/
│   │   ├── FloatingWindow.svelte
│   │   ├── FloatingBall.svelte
│   │   ├── StatusBar.svelte
│   │   ├── HistoryList.svelte
│   │   ├── HistoryItem.svelte
│   │   ├── ApprovalDialog.svelte         # group-approval + file-receive 复用 BaseApprovalCard
│   │   ├── BaseApprovalCard.svelte
│   │   ├── JoinDialog.svelte
│   │   ├── SettingsPanel.svelte
│   │   ├── Footer.svelte                  # IP + device 区
│   │   └── Toast.svelte
│   ├── stores/
│   │   ├── group.ts             # peers / status / handshakePending Svelte 5 rune store
│   │   ├── history.ts
│   │   ├── config.ts
│   │   └── pendingFiles.ts
│   └── ipc/
│       ├── commands.ts          # invoke wrapper + 错误归一化
│       ├── events.ts            # listen wrapper for window-shown / history-updated / handshake-pending / handshake-dismissed / file-pending / file-saved / status-updated
│       └── types.ts             # 与 protocol.rs / commands.rs 对应的 TS 类型
└── static/                      # 图标
```

- 优点：拆解 v0 教训第 5.4 节列出的所有 5 个反模式（单文件膨胀 / 上帝 struct / 锁粒度粗 / 模块边界不清 / 测试不可达）；`peer/state.rs` 一个文件就能塞所有 PeerState 字段（含隐形掉线新增的 `last_successful_sync_at`），避免散在 4 个 HashMap；`crypto/mod.rs` trait 边界让单元测试可达
- 缺点：拆分量大；初次实现成本比选项 A 高 30-40%；目录嵌套增加导航成本（但 IDE / `rg` 抵消）
- 跨平台风险：无（仅源码组织）
- 实现复杂度：中
- 与 spec 关系：明确满足 `00-product-overview.md` 项目级验收 #5（每份 spec 含验收）+ #7（新人 30 分钟上手）；满足 `peer-heartbeat.md` 第 7 节 [P1] PeerRegistry 抽象议题；满足 `floating-window.md` / `clipboard-text-sync.md` / `history-list.md` 多份 spec 5.4 反复点名的"禁止单文件堆砌"

#### 选项 C：Actor 模型 + 全异步 channel

把 `clipboard / network / peer / approval` 各自封成独立 actor，AppState 退化为只持 actor handle（`mpsc::Sender`）；任何跨域调用都走消息。

- 优点：彻底消除锁；并发模型干净
- 缺点：v0 团队从未跑通 actor 模式；学习曲线陡；故障排查成本飙升（消息丢失 / actor panic / 死锁迁移到信道）
- 跨平台风险：无
- 实现复杂度：高
- 否决：单人项目 + v0 已用 `parking_lot::RwLock` 的简单锁模型且没有锁竞争问题；过度工程

---

### 2.2 HTTP 协议总骨架

#### 选项 A：v0 端点表沿用 + 加 3 个新端点

沿用 v0 11 个端点 + 新增 `diagnostic-logging` 不需要新端点 + `clipboard-snapshot-sync` 新增 1 个 `/clipboard/snapshot` 端点 + `peer-heartbeat` 隐形掉线兜底所需的 `/peers/rehandshake` 触发端点（视决策而定）。**通用 header**：v0 没有显式 header，所有元信息塞 body。

- 端点列表（v0）：`/handshake` POST、`/clipboard` POST、`/file` POST、`/peers/approval/forward` POST、`/peers/approval/decide` POST、`/peers/approval/dismiss` POST、`/peers/trust` POST、`/peers/ban` POST、`/peers/leave` POST、`/delete_history` POST、`/history/clear` POST、`/ping` GET（共 12 个）
- 优点：与 v0 1:1，代码搬迁简单
- 缺点：所有元信息（origin_device_id / seq）塞 body 让"未解析 body 前的鉴权"不可能；非 PNG 图片走 file 通路时复用 `/file` 没问题，但 snapshot 额外开端点增加协议面积
- 跨平台风险：无

#### 选项 B：v0 端点表沿用 + 通用 header（推荐）

沿用 v0 端点（仍 12 个，`/clipboard/snapshot` 不新增，复用 `/clipboard` 加 `is_snapshot` flag —— 见 3.2 决策依据）。**新增三个通用 HTTP header**：

- `X-SC-Device-Id`：origin device id（与 body 一致；先用于 fast-fail 校验，body 仍是权威）
- `X-SC-Seq`：单调递增 seq（与 body 一致；先用于 dedupe lookup）
- `X-SC-Auth`：暂不实现（占位字段，留 ADR-008 安全审阅决定 PSK / HMAC tag），但协议层定义存在让未来加更安全的认证不需要协议 break

**状态码语义**（统一规约）：

| 码 | 语义 | 谁返回 |
|---|---|---|
| 200 | OK | 全部成功路径 |
| 400 | 请求格式错（JSON 解析失败 / 字段缺失 / size 校验不通过） | 所有 handler |
| 403 | 鉴权失败（origin 不在 peers 表 / 用户拒绝审批 / device 在 banned_device_ids） | 所有 handler |
| 408 | 审批超时（30s 内无人决定） | `/handshake` `/file` |
| 409 | device_id 冲突 | `/handshake` |
| 413 | 请求体过大（size > MAX_FILE_SIZE = 5 MB） | `/file` |
| 422 | 解密失败 / plaintext.len != size | `/clipboard` `/file` |
| 500 | 写盘失败 / 不可恢复内部错 | `/file` |

**body 仍是权威**：header 是给 middleware / 监控 fast path 用，handler 内部依然以 body 字段为准（防止 header / body 不一致的攻击）。

- 优点：未来加认证 / 限流 / 监控不需要 break body 协议；不引入新端点保护协议面积；status code 统一让前端 ipc 层能集中映射用户提示
- 缺点：header / body 重复字段；冗余信息 ~30 字节/请求，对 LAN 不可见
- 跨平台风险：无（reqwest + axum 都原生支持 header）
- 实现复杂度：低

#### 选项 C：切换到自定义二进制 framed TCP

用 length-prefixed binary framing 替代 HTTP+JSON+base64，省 33% 膨胀。

- 优点：流量小（图片/文件场景显著）；无 base64 编解码开销
- 缺点：丧失 curl 可调试性（v0 教训 5.4 节点名 HTTP 调试性是 v0 选 HTTP 的核心理由）；轮转协议版本协商工作量大；与 axum 0.8 / reqwest 0.12 现状脱节，需要重写 client/server 抽象层
- 跨平台风险：无
- 实现复杂度：高
- 否决：5 MB 文件 + base64 33% 膨胀 → 6.7 MB 网络流量，LAN 1 Gbps 下 < 100ms，瓶颈不在带宽；调试性收益远高于带宽节省

---

### 2.3 PeerState 数据模型

#### 选项 A：v0 散点（4 个独立 HashMap）

`peers: HashMap<device_id, PeerInfo>` + `peer_keys: HashMap<device_id, [u8;32]>` + `fail_counts: HashMap<device_id, u32>` + `last_seen_seq: HashMap<(device_id, kind), u64>`，分散在 AppState 各字段。

- 优点：v0 现状，搬迁成本零
- 缺点：4 个 HashMap 各自加锁，任何"按 device_id 视角看一台 peer 全态"的查询要拿 4 把锁；新增字段（`last_successful_sync_at` / `consecutive_send_failures`）必须在 5 个文件加 5 处；与 `peer-heartbeat.md` 第 7 节 [P1] PeerRegistry 抽象议题直接冲突
- 实现复杂度：低（保持 v0）

#### 选项 B：统一 PeerState struct + PeerRegistry（推荐）

```
peer/state.rs:

pub struct PeerState {
    pub device_id: String,
    pub device_name: String,
    pub addr: SocketAddr,                                // listen_port + remote.ip()
    pub pubkey_b64: String,
    pub aes_key: [u8; 32],

    pub last_successful_sync_at: Option<Instant>,        // 隐形掉线检测；定义见 3.7
    pub last_heartbeat_at: Option<Instant>,
    pub consecutive_heartbeat_failures: u32,             // = v0 fail_counts
    pub consecutive_send_failures: u32,                  // 隐形掉线兜底 #2 用

    pub trust_state: TrustState,                         // Approved | Banned | Pending（仅本机视角）
    pub last_seen_seq_by_kind: HashMap<&'static str, u64>,  // text / image_png / file / trust / ban / leave / delete_history / clear_history / approval
}

pub enum TrustState { Approved, Banned, Pending }

peer/mod.rs:

pub struct PeerRegistry {
    inner: parking_lot::RwLock<HashMap<String, PeerState>>,
    approved: parking_lot::RwLock<HashSet<String>>,      // 短路缓存（subject 还没成为 peer 时也要查）
    banned: parking_lot::RwLock<HashSet<String>>,
}

impl PeerRegistry {
    pub fn insert(&self, state: PeerState);
    pub fn remove(&self, device_id: &str);
    pub fn get(&self, device_id: &str) -> Option<PeerState>;
    pub fn snapshot(&self) -> Vec<PeerState>;            // 返 clone（轻量字段）
    pub fn record_heartbeat_fail(&self, id: &str) -> u32;   // 累计失败 +1 返新值
    pub fn record_heartbeat_ok(&self, id: &str);             // 重置 fail
    pub fn record_send_fail(&self, id: &str) -> u32;
    pub fn record_send_ok(&self, id: &str);                  // 同时更新 last_successful_sync_at
    pub fn is_approved(&self, id: &str) -> bool;
    pub fn is_banned(&self, id: &str) -> bool;
    pub fn approve(&self, id: &str);                          // 原子 approve+un-ban（互斥覆盖）
    pub fn ban(&self, id: &str);                              // 原子 ban+un-approve
    pub fn seen_seq_and_update(&self, id: &str, kind: &str, seq: u64) -> bool;
}
```

- 优点：所有 peer 维度的状态一处可查；`peer-heartbeat.md` 新 AC（强制重连 / 健康自检 / 上次成功同步时间）字段就位；trust gossip 互斥覆盖语义集中实现（避免 `group-trust-gossip.md` 第 5.2 节坑：互斥语义只在 handler 一行）；测试可通过 mock PeerRegistry 跑
- 缺点：初次构建工程量；`approved` / `banned` 的"非 peer 也要查"场景需要冗余存（subject 还没握手成功时也要查 banned 短路）；与 v0 概念映射要做迁移层
- 跨平台风险：无
- 实现复杂度：中

**字段必含清单**（spec 推导）：

| 字段 | 来源 | 用途 |
|---|---|---|
| `device_id` | v0 + group-discovery | 主键 |
| `addr: SocketAddr` | v0（remote.ip + listen_port） | client 发请求 |
| `pubkey_b64` | e2e-encryption | 调试 / 重新握手 |
| `aes_key: [u8; 32]` | e2e-encryption | per-pair 密钥 |
| `last_successful_sync_at: Option<Instant>` | peer-heartbeat 第 4 节 隐形掉线兜底 #3 | UI 显示 + 健康自检触发判定 |
| `last_heartbeat_at` | peer-heartbeat | 调试 |
| `consecutive_heartbeat_failures` | peer-heartbeat | FAIL_LIMIT / 强制重连 N 阈值 |
| `consecutive_send_failures` | peer-heartbeat 第 4 节 兜底 #2 | M 阈值触发健康自检 |
| `trust_state` | group-approval / group-trust-gossip | 短路审批 / 短路 ban |
| `last_seen_seq_by_kind` | clipboard-text-sync 第 5.1 节 / 所有 broadcast 端点 | seq dedupe |

#### 选项 C：单一 god struct（v0 + 字段叠加）

直接把所有上述字段塞进 v0 的 `PeerInfo`，但仍以 4 个独立 HashMap 表达。

- 否决：与选项 A 同样的"上帝结构 + 散点锁"问题，没解决教训 5.4 节的 AppState 反模式

---

### 2.4 加密层抽象边界

#### 选项 A：保持 v0 函数式（无 trait）

`crypto.rs` 暴露 6 个自由函数：`new_ephemeral / pubkey_to_b64 / pubkey_from_b64 / derive_aes_key / encrypt / decrypt`。

- 优点：v0 现状，简单
- 缺点：`derive_aes_key` 消费 `EphemeralSecret` 所有权 → 测试不便（e2e-encryption.md 5.2 节已点名）；未来切换密码学栈（如加 PSK / 切到 Noise Protocol）必须改全网调用点；不可 mock

#### 选项 B：trait 化 + 默认实现（推荐）

```
crypto/mod.rs:

pub trait KeyExchange {
    type Secret;
    type PublicKey;

    fn new_ephemeral() -> (Self::Secret, Self::PublicKey);
    fn pubkey_to_b64(pk: &Self::PublicKey) -> String;
    fn pubkey_from_b64(s: &str) -> Result<Self::PublicKey>;
    fn derive_aes_key(secret: Self::Secret, their: &Self::PublicKey) -> Result<[u8; 32]>;
}

pub trait Sealer {
    fn encrypt(&self, key: &[u8; 32], plaintext: &[u8], aad: &[u8]) -> Result<(Nonce, Ciphertext)>;
    fn decrypt(&self, key: &[u8; 32], nonce: &[u8], ciphertext: &[u8], aad: &[u8]) -> Result<Vec<u8>>;
}

pub trait Verifier {
    // 占位：security-reviewer 决定是否引入 HMAC tag / PSK challenge
    fn verify_origin(&self, claim: &str) -> Result<()>;
}

pub mod x25519;     // impl KeyExchange for X25519Provider
pub mod aes_gcm;    // impl Sealer for AesGcmSealer
```

**密钥生命周期**：

| 密钥 | 生命周期 | 持有者 | 销毁触发 |
|---|---|---|---|
| 临时 X25519 `EphemeralSecret` | 单次握手（调用 `derive_aes_key` 即消费） | 函数局部 | derive 完即 drop |
| 共享秘密（DH 输出） | 函数局部 | 函数栈 | derive_aes_key 内部 zeroize 后立即 drop |
| AES-256 per-peer key | 每 peer / 每会话 | `PeerRegistry.inner[id].aes_key` | peer 移除（leave / ban / heartbeat 剔除）/ 重新握手覆盖 / 进程退出 |
| 长期密钥 | **无** | — | — |

- 优点：trait 边界让单元测试可达（mock KeyExchange / Sealer 可跑 e2e 测试）；未来切到 PSK 或加 AAD 绑定只改 impl 不改调用点；满足 e2e-encryption.md 第 4 节 AC #6 的"单元测试覆盖 ≥ 5 条"
- 缺点：trait 抽象增加少量代码；EphemeralSecret 类型透传需要泛型或 `dyn` 取舍（建议用关联类型 + 具体 impl 注入）
- 跨平台风险：无
- 实现复杂度：中
- 与 spec 关系：直接答 `e2e-encryption.md` 第 7 节 [P1] [架构师] "derive_aes_key 是否改更易测试 trait 抽象"；为 ADR-008（security-reviewer）留 Verifier 占位，不锁死 PSK 决策

#### 选项 C：noise_protocol crate 全替换

引入 `snow` crate 实现 Noise XX pattern。

- 优点：业界标准 + 抗 MITM
- 缺点：v0 已用 X25519+HKDF+AES-GCM 跑通，重写成本 + 新依赖学习曲线；Noise 的 handshake 是多步（XX 是 1.5 RTT），与 v0 单 POST 握手协议冲突
- 否决：超出 v2 范围；如未来要做留 ADR-N supersede 本节

---

### 2.5 长生存周期任务 lifecycle owner（v5-5）

#### 选项 A：v0 散乱 spawn

每个 task 在自己模块 `tauri::async_runtime::spawn`：clipboard 是 std::thread；axum 在 `start_server_if_needed` spawn；health 在 `health::spawn`；leave 广播在 quit_app 内部 spawn 临时任务。**没有统一的启动顺序 / 关闭顺序**。

- 缺点：v0 lessons-learned 4.4 节"tokio runtime 在 Tauri main 不能 #[tokio::main]" + v0 quit_app 五步序列就是经验式补丁；新增 task（如 diagnostic-logging 文件 appender flush worker）没有归口

#### 选项 B：lifecycle.rs 集中管理（推荐）

`app/lifecycle.rs` 暴露：

```
pub struct Lifecycle {
    clipboard_thread_handle: Option<JoinHandle<()>>,    // std::thread::JoinHandle
    server_shutdown_tx: Option<oneshot::Sender<()>>,
    health_task: Option<tokio::task::JoinHandle<()>>,
    // logging_flush_task 由 log/mod.rs 注册并放进来
}

impl Lifecycle {
    pub async fn start(&mut self, app: &AppHandle, state: &AppState);
    pub async fn shutdown(&mut self, state: &AppState) -> Duration;  // 返回总耗时（用于 quit_app 1.5s 上限审计）
}
```

**启动顺序**（lifecycle.start）：

1. `tracing` init + file appender + rotation worker（log/mod.rs）— 必须最早，让后续 task 的 panic 都能进文件
2. `Config::load`（同步阻塞 ≤ 50ms）
3. `clipboard::spawn` — std::thread 持有 arboard，返回 `mpsc::Sender<ClipboardCmd>`（Tauri 主线程不能 own arboard，v0 教训）
4. `network::server::start` — axum 起在 `tauri::async_runtime` 内（v0 lessons-learned 4.4 节：禁止 `#[tokio::main]`）
5. `network::health::spawn` — 心跳 worker，复用 axum 同一 tokio runtime
6. emit `app-ready` 事件让前端开始调用命令

**关闭顺序**（lifecycle.shutdown，由 quit_app 调用）：

1. `broadcast_leave(state)` 包 `tokio::time::timeout(1500ms)` —— 已收敛的 v0 经验
2. abort `health_task`（tokio::task::JoinHandle::abort）—— 解决 peer-heartbeat.md 5.4 节"task 取消机制 v2 是否显式 abort"
3. `server_shutdown_tx.send(())` 让 axum graceful shutdown
4. clear PeerRegistry（peers / peer_keys / approved / banned 一次性清）
5. clipboard thread：通过 mpsc 发 `ClipboardCmd::Shutdown`；std::thread::join 给 100ms 软上限，过则 detach
6. log flush（确保 quit 路径的 leave / clear 都进文件）
7. `app.exit(0)`

**runtime 归属**：

| Task | 归属 runtime | 启动者 | 关闭者 |
|---|---|---|---|
| 剪切板轮询 + arboard 写入 | std::thread（独立 OS 线程，不在 tokio 内） | lifecycle.start step 3 | lifecycle.shutdown step 5（mpsc Shutdown）|
| HTTP server (axum) | `tauri::async_runtime`（即 Tauri 内置 tokio multi-thread） | step 4 | step 3（shutdown_tx） |
| 心跳 worker (`network::health`) | 同上 axum runtime | step 5 | step 2（abort handle）|
| 自检 ping / 强制重连 worker（隐形掉线 兜底 #1+#2 实现） | 同上 axum runtime | step 5（与 health 合并 task） | step 2 |
| 日志 file appender flush | 单独 `tracing-appender` 内部线程（NonBlocking guard） | step 1 | guard drop 时自动 flush |

- 优点：启动 / 关闭路径文档化 + 唯一；解决 `tray-integration.md` / `group-leave-notify.md` 反复点名的"四处退出路径不一致"；解决 `peer-heartbeat.md` 5.4 节"健康 task 取消机制"；满足 v5-5 lifecycle owner 强约束
- 缺点：lifecycle.rs 承担"启动器"职责，逻辑集中；测试 `Lifecycle` 整体困难（需要 mock AppHandle）
- 跨平台风险：无（std::thread + tokio multi-thread 在 Mac/Win 都稳）
- 实现复杂度：中

#### 选项 C：actor-style supervisor

引入一个 `Supervisor` actor 监督所有子 task 重启策略（OneForOne / OneForAll）。

- 否决：单人项目过度工程；v0 没有这类需求

---

### 2.6 错误处理 + 日志总策略

#### 选项 A：纯 anyhow（v0 现状）

所有 Result 用 `anyhow::Result<T>`；错误从源头一路 `?` 到 handler / command。

- 优点：写起来快；context 链支持好
- 缺点：handler 层无法把 anyhow 错精确映射到 HTTP 状态码（4xx vs 5xx）；前端拿到的是字符串没法做 i18n / 程序化处理；与 `group-discovery.md` 5.2 节"错误码翻译散落 client.rs"教训直接对应

#### 选项 B：分层 — anyhow 内部 + 自定义 enum 在 boundary（推荐）

**boundary 层定义** `network/error.rs`：

```rust
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("invalid request: {0}")]    BadRequest(String),       // → 400
    #[error("forbidden: {0}")]          Forbidden(&'static str),  // → 403
    #[error("approval timeout")]        ApprovalTimeout,          // → 408
    #[error("device id conflict")]      DeviceIdConflict,         // → 409
    #[error("payload too large")]       PayloadTooLarge,          // → 413
    #[error("decrypt or size mismatch")]CryptoOrSizeMismatch,     // → 422
    #[error("internal: {0}")]           Internal(#[from] anyhow::Error),  // → 500
}

impl IntoResponse for NetworkError { ... }
```

**commands.rs boundary**：

```rust
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("{0}")] User(String),       // 用户友好消息（已 i18n / 已映射）
    #[error("{0}")] Internal(#[from] anyhow::Error),
}
```

- 内部纯 `anyhow::Result` 链式 ?；只在 axum handler return 类型 / Tauri command return 类型上转 enum
- `tauri::ipc::Response` 或 `Result<T, String>` 形式由 `CommandError -> String`（用户层只看用户友好串）

**`tracing` 配置**（drives `diagnostic-logging`）：

- subscriber：`tracing_subscriber::fmt` + `EnvFilter` + `tracing_appender::rolling::daily`（按日轮转） + `tracing_appender::non_blocking`（避免文件写阻塞主线程）
- 输出格式：`time | level | target | message + structured fields`（target = module path，让 `tracing::info!(target: "network::server", ...)` 在文件可 grep）
- 同时输出到 stderr（dev）+ rolling file（release & dev 都开），以满足 `diagnostic-logging.md` 第 4 节 AC
- 默认 filter：`info,sync_copy_lib=info`；`RUST_LOG` 覆盖；运行时诊断模式开关写 `tracing_subscriber::reload::Handle` 切到 `debug`
- 日志位置（diagnostic-logging.md 第 3 节锁死）：mac `~/Library/Logs/com.synccopy.app/sync-copy.log` / Win `%LOCALAPPDATA%\com.synccopy.app\logs\sync-copy.log`
- 轮转：单文件 ≤ 10 MB（用 `tracing-appender` 的 `RollingFileAppender::new(Rotation::DAILY, dir, "sync-copy.log")` + 自定义大小 hook 或换 `tracing-rolling-file` crate；具体 crate 选型由 implementer 在 `diagnostic-logging` ADR 决定，本 ADR 仅约束行为）；保留近 7 天或 10 个文件中较大者

**v4-7 fatal error 三件套** 落地路径：

1. **写文件日志**（不依赖 stderr）：`tracing::error!(target: "fatal", panic_payload, backtrace, ...)` 走同一个 file appender；进程在 panic hook 中 sync-flush
2. **弹用户对话**：注册 `std::panic::set_hook` → 在 hook 中调用 Tauri `app.dialog().message(...)` 显示 "Sync Copy 遇到致命错误，已写入日志：<path>"；macOS 用 `osascript display dialog`、Win 用 `MessageBoxW` 兜底（如 Tauri runtime 已 panic）
3. **不静默 exit**：panic hook 末尾**不**调 `process::exit(0)` 而是 `process::abort()`；同时让 `lifecycle.shutdown` 的 quit_app 路径在 fatal 后置 `is_fatal = true` flag 关闭网络静默
4. 启动早期校验：log 文件目录不可写时 → 浮窗启动后 emit `log-write-failed` → SettingsPanel 显示 "日志写入失败" 一次性提示（diagnostic-logging.md 第 4 节 AC）

- 优点：layered 错误链 + 边界精确状态码 + 用户层友好消息；diagnostic-logging.md / fatal three-piece 全部落地路径化；v4-7 强约束实现
- 缺点：boundary 转换增加少量代码；panic hook 调 Tauri dialog 在 runtime 已死时会失败，需 OS 原生兜底
- 跨平台风险：panic hook → OS 原生 dialog 需 cfg 隔离；可接受
- 实现复杂度：中

#### 选项 C：完全 enum（thiserror 全栈 / 不用 anyhow）

每层定义自己 enum，互相 `From` 转换。

- 否决：单人项目代码量翻倍；anyhow 的 context 链丢失；不值

---

### 2.7 隐形掉线机制（_assumptions A_BUG_HIDDEN_DEAD / peer-heartbeat.md 1.1）

> 这是 v2 解决 v0 实战 bug 的核心。`peer-heartbeat.md` 第 4 节新增 3 条 AC + 第 7 节 [P1] 议题列出 5-6 个待决参数。

#### 选项 A：v0 心跳层 + 增加并发 ping

仅把 v0 串行 for-loop 改并发 `try_join_all` + N=8 并发；FAIL_LIMIT 仍是 2。**不动其它**。

- 优点：改动小
- 缺点：不解决 v0 实战 bug 根因——TCP 半死状态下心跳依然能拿 200 OK；不响应 peer-heartbeat.md 4 节新增 3 条 AC

#### 选项 B：心跳 + 强制重连 + 健康自检 + UI 锚点（推荐 — 4 件套）

**参数定锚**（参考 peer-heartbeat.md 第 7 节 [P1] 建议值 + 本 ADR 收敛）：

- `PING_INTERVAL = 10s`（v0 沿用，不变）
- `PING_TIMEOUT = 2s` / `connect_timeout = 1s`（v0 沿用）
- `FAIL_LIMIT = 2`（连续 2 次心跳失败 → 从 PeerRegistry 移除该 peer，触发 emit `status-updated`）
- **`FORCE_REBUILD_LIMIT = 3`**（连续 3 次心跳失败 → 强制重建该 peer 的 reqwest 底层 TCP 连接 + 触发一次 re-handshake；第 3 次失败时**先**强制重连，**仍**失败到 FAIL_LIMIT=2 又叠加才剔除。状态机详见下方）
- **`SEND_FAIL_THRESHOLD = 2`**（剪切板/文件广播给某 peer 失败 ≥ 2 次 → 触发"被动健康自检"——立即给该 peer 发一次 ping，结果归并到心跳计数；若仍失败立即强制重连，不等下个 PING_INTERVAL 周期）
- **`HEALTH_SELFCHECK_DEBOUNCE = 30s`**（同一 peer 30s 内最多触发一次健康自检，避免广播失败风暴下刷屏）
- **TCP keepalive**：`reqwest::ClientBuilder::tcp_keepalive(Some(Duration::from_secs(20)))`；让 OS 内核每 20s 发 keepalive probe（reqwest 0.12 支持），失败时让 connection pool 自动驱逐 dead connection；这是隐形掉线 OS 层兜底
- **强制重连实现**：reqwest::Client 不支持"驱逐单条连接"，改用 `client_pool.rs` 维护 `HashMap<device_id, Arc<reqwest::Client>>` —— 每 peer 一把 Client；强制重连 = `pool.replace(device_id, Client::builder()...build())` 让旧 Client 连同其连接池一起 drop。trade-off：每 peer 多一个 Client 实例（轻量，每个 ~ 1 KB）
- **`last_successful_sync_at` 写入时机**（PeerRegistry.record_send_ok 内）：本机广播给某 peer 拿到 200 OK 时 update；**不**在仅心跳 200 OK 时 update（peer-heartbeat.md 第 7 节用户原话："peer 表面在线但没真正同步"是隐形掉线的核心 — 仅心跳成功不算"成功同步"）
- **piggyback 策略**：暂不实现（peer-heartbeat.md 第 7 节标 P2 优化）；v2.0 用独立 ping 路径
- **UI 锚点**（floating-window 第 6 节 / floating-ball 暂留 UX 决策）：浮窗状态栏右侧或历史区头部展示 "上次同步：<相对时间>"；当某 peer `last_successful_sync_at >= 5min` 时，状态点变黄；具体视觉由 ux-designer 在 P2-3.c 补 UX 段（peer-heartbeat.md 第 6 节已留 UX 占位）

**状态机**（伪代码 — 实现层细化由 implementer）：

```
on heartbeat_tick:
  for peer in registry.snapshot():
    let r = http_get_ping(peer.addr);
    if r.is_ok():
      registry.record_heartbeat_ok(peer.id)             // reset consecutive_*_failures
    else:
      let n = registry.record_heartbeat_fail(peer.id)   // returns updated value
      if n == FORCE_REBUILD_LIMIT:                      // n == 3
        client_pool.replace(peer.id)                    // drop & rebuild Client
        spawn re_handshake(peer)                        // best-effort, don't block
      if n >= FORCE_REBUILD_LIMIT + FAIL_LIMIT:        // n == 5（即重连后又连续 2 次失败）
        registry.remove(peer.id)
        emit status-updated

on broadcast_send_fail(peer_id):
  let m = registry.record_send_fail(peer_id)
  if m >= SEND_FAIL_THRESHOLD                            // m == 2
     and last_health_selfcheck_at(peer_id) > 30s ago:
    spawn health_selfcheck(peer_id)                      // 立刻 ping + 失败立刻强制重连

on broadcast_send_ok(peer_id):
  registry.record_send_ok(peer_id)                       // 同时 update last_successful_sync_at
```

- 优点：解决 v0 实战 bug 根因（强制重连让 TCP 半死状态被周期性强制重建）+ 解决"心跳绿但实际死透"的 UX 盲区（UI 锚点 + UI 黄变 提示）+ 不依赖单一信号（OS keepalive + 心跳 + 广播 fail 三层）；映射 peer-heartbeat.md 第 4 节新 3 条 AC
- 缺点：参数都是"建议值"——实战可能需要再调；client_pool per-peer Client 引入工程量
- 跨平台风险：`tcp_keepalive` 在 reqwest 0.12 跨平台支持（mac/Win 都基于 OS socket option）；测试覆盖
- 实现复杂度：中

#### 选项 C：彻底重写为长连接（WebSocket / TCP framed）

抛弃 HTTP 短连接，每对 peer 一个长连接 + 应用层 keepalive。

- 优点：连接状态彻底显式
- 缺点：协议大改；与 axum HTTP 现状冲突；超出本 ADR 范围
- 否决：留给 v3 评估；本 ADR 4 件套已能解决报告的 bug

---

## 3. 决定（Decision）

按 7 个子决策点逐一定锚。每条结论附"为什么不选其它"的一句话理由。

### 3.1 模块切分 — 选 选项 B（分层 + 域驱动拆分）

**决议**：

- 后端按 `app / config / crypto / clipboard / history / peer / network / network::handlers / commands / log` 10 个一级目录拆分；具体子文件见 第 2.1 节 选项 B 树状图
- 前端按 `routes / lib::components / lib::stores / lib::ipc` 拆分；至少 11 个 Svelte 组件（含 BaseApprovalCard 抽象）+ 4 个 store + 3 个 ipc 模块文件
- **绝对禁止**：单文件 > 400 行（v0 教训 5.2 节"单文件膨胀"硬约束）；如某 handler 文件膨胀，进一步按 sub-action 拆

**为什么不选 A**：选项 A 不解决 AppState 上帝结构 + commands.rs 大杂烩，被 v0 教训 5.4 节多份 spec（00 总览 / floating-window / clipboard-text-sync / peer-heartbeat）反复点名。

**为什么不选 C**：actor 模式过度工程。

### 3.2 HTTP 协议总骨架 — 选 选项 B

**决议**：

- 端点列表：v0 的 12 个端点全部沿用，**不新增**；`clipboard-snapshot-sync` 复用 `/clipboard` 端点 + body 加 `is_snapshot: bool` flag（详见下文）；`clipboard-image-sync` 沿用 `/clipboard` 的 `kind=image_png` 分支
- 通用 header：`X-SC-Device-Id` + `X-SC-Seq` + `X-SC-Auth`（占位） — body 仍权威
- 状态码语义：见 2.2 选项 B 表（200/400/403/408/409/413/422/500）
- `clipboard-snapshot-sync` 决议：**复用 `/clipboard` + `is_snapshot` flag**（替代开 `/clipboard/snapshot` 新端点）—— 协议面积更小；`is_snapshot` flag 进 AAD（如 ADR-008 决定绑 AAD），自然防止 snapshot 报文被重放为普通 clipboard 投放
- **非 PNG 路由**（_assumptions A14）：剪切板里出现 JPG/GIF/WebP 时，**arboard `get_image()` 成功 → PNG 通路（即统一为 PNG 编码）**，arboard `get_image()` 失败 → 静默不处理；用户对策 = 拖拽文件走 `/file` 通路。**不**在剪切板模块里检测原始字节流并自动转走 file 通路（避免边界 case 爆炸 + 与"先 image 后 text"轮询语义冲突）
- **OS 光栅化 PNG 提示**：当 `clipboard-image-sync` 检测 `image_size > MAX_IMAGE_BYTES` 时给用户 toast `图片超过 5 MB，未同步`；用户原意是 JPG 但被光栅化为大 PNG 撞 5 MB 上限的场景视为正常体验路径（不区分提示）

**为什么不选 A**：v0 端点表沿用 + 不加 header → 未来加认证必须改 body 协议；冗余 30 字节/请求是 LAN 内可忽略代价。

**为什么不选 C**：HTTP+JSON+base64 调试性 (curl 可手测) > 33% 流量节省 (LAN 1Gbps 下不构成瓶颈)。

### 3.3 PeerState 数据模型 — 选 选项 B

**决议**：

- `peer/state.rs` 定义 `PeerState` struct，必含字段（清单见 2.3 选项 B 表）
- `peer/mod.rs` 定义 `PeerRegistry`（顶层 RwLock + approved/banned 短路缓存）
- AppState 顶层结构改为只持有 `Arc<PeerRegistry>` + `Arc<History>` + `Arc<Config>` + 通信句柄 (`mpsc::Sender<ClipboardCmd>` / `oneshot::Sender for server_shutdown`)；不直接持 4 个独立 HashMap
- **隐形掉线相关字段全部就位**（`last_successful_sync_at` / `consecutive_heartbeat_failures` / `consecutive_send_failures`）— 满足 peer-heartbeat.md 第 4 节 AC #9 #10 #11
- **trust 互斥语义集中实现**：PeerRegistry.approve 原子做 approved.insert + banned.remove；ban 反之；group-trust-gossip handler 调 registry 接口而非各自加锁

**为什么不选 A**：v0 散点 4-HashMap 拒绝任何"按 peer 视角全态查"操作，新增字段成本高，违反 peer-heartbeat.md 5.4 节抽象议题。

**为什么不选 C**：单 god struct 没解决散点锁问题。

### 3.4 加密层抽象边界 — 选 选项 B

**决议**：

- 定义三个 trait：`KeyExchange`（含关联类型 Secret/PublicKey）/ `Sealer`（encrypt/decrypt 含 aad 入参） / `Verifier`（占位，留 ADR-008 安全审阅决定）
- 默认实现：`crypto::x25519::X25519Provider` + `crypto::aes_gcm::AesGcmSealer`
- HKDF salt = `b"sync-copy-v2-salt"` / info = `b"sync-copy-v2:aes-256-gcm"`（bump 到 v2，理由：v2 协议字段可能不兼容 v0；HKDF 不同 salt/info 派生密钥不通，等于强制双方协议版本一致）
- AAD：暂传 `&[]`；**ADR-008 安全审阅会决定是否绑 `origin_device_id || seq || kind`**；本 ADR 在 trait 签名上预留 aad 入参不锁死值
- 密钥生命周期表（2.4 选项 B 完整列出）；**`zeroize` crate 引入决议**：本 ADR 不强制（避免越界覆盖 e2e-encryption.md 第 7 节 [P0] [安全] 议题），由 ADR-008 决定；trait 设计不阻碍 zeroize 引入

**为什么不选 A**：函数式 + EphemeralSecret 消费签名让单元测试不可达，违反 e2e-encryption.md 第 4 节 AC #6。

**为什么不选 C**：Noise Protocol 重写超出 v2 范围。

### 3.5 lifecycle owner — 选 选项 B

**决议**：

- `app/lifecycle.rs` 暴露 `Lifecycle::start` + `Lifecycle::shutdown`
- 启动顺序 7 步（log → config → clipboard thread → axum → health → emit ready），关闭顺序 7 步（broadcast_leave 1.5s → abort health → server shutdown → clear registry → clipboard thread join 100ms 软上限 → log flush → exit）
- runtime 归属表（2.5 选项 B 表）：剪切板 std::thread；server / health / 自检 全部 Tauri tokio runtime；日志 NonBlocking guard 内置线程
- **退出路径唯一**（`tray-integration.md` / `group-leave-notify.md` / `settings-panel.md` 反复要求）：tray quit / settings quit / Cmd+Q / OS close signal 全部路由到 `commands::group::quit_app`，内部调 `Lifecycle::shutdown`

**为什么不选 A**：散乱 spawn → 关闭顺序无文档 → 退出路径不一致是 v0 三份 spec 教训第 5.4 节同议题。

**为什么不选 C**：actor supervisor 单人项目过度工程。

### 3.6 错误处理 + 日志总策略 — 选 选项 B

**决议**：

- 内部 `anyhow::Result` 链 + boundary 转 enum（`NetworkError` / `CommandError`）
- `NetworkError → IntoResponse` 映射 7 状态码（400/403/408/409/413/422/500），见 2.6
- `CommandError -> String` 让前端拿到的 invoke 失败是用户友好串
- `tracing` + `tracing-appender::rolling` 按日轮转 + `non_blocking` + Reload Handle 让运行时切 debug；输出 stderr (dev) + 滚动文件 (always)
- 日志路径：mac `~/Library/Logs/com.synccopy.app/` / Win `%LOCALAPPDATA%\com.synccopy.app\logs\`（diagnostic-logging.md 第 3 节锁死，本 ADR 仅引用）
- **v4-7 fatal 三件套** 落地：std::panic::set_hook → tracing::error 入文件 + Tauri dialog（runtime 死时 OS 原生 MessageBox 兜底）+ process::abort 不静默
- **敏感字段黑名单**：剪切板明文 / AES key / X25519 私钥 / shared secret / HKDF 中间值 永不进 tracing fields；diagnostic-logging.md 第 7 节 [P0] [安全] 关于 device_id / device_name / IP 是否记的细化决议留给 ADR-008

**为什么不选 A**：纯 anyhow 在 boundary 无法精确状态码 + 前端无法程序化处理。

**为什么不选 C**：全 enum 代码量翻倍，单人项目不值。

### 3.7 隐形掉线机制 — 选 选项 B（4 件套）

**决议**：

| 参数 | 取值 | 说明 |
|---|---|---|
| `PING_INTERVAL` | 10s | v0 沿用 |
| `PING_TIMEOUT` | 2s | v0 沿用 |
| `connect_timeout` | 1s | v0 沿用 |
| `FAIL_LIMIT` | 2 | 剔除阈值（v0 沿用） |
| `FORCE_REBUILD_LIMIT` (N) | **3** | 连续 3 次心跳失败 → 强制重建该 peer 的 reqwest Client + 触发 re-handshake |
| `SEND_FAIL_THRESHOLD` (M) | **2** | 广播给该 peer 失败 ≥ 2 次 → 立即触发健康自检 ping，绕过 PING_INTERVAL |
| `HEALTH_SELFCHECK_DEBOUNCE` | 30s | 同 peer 30s 内最多 1 次自检（防风暴） |
| TCP keepalive | reqwest `tcp_keepalive(20s)` | OS 层兜底，让连接池自动驱逐 dead connection |

- **状态机**（2.7 选项 B 伪代码）：心跳累计 N=3 强制重连；累计 N+FAIL_LIMIT=5 才剔除；广播失败 M=2 触发健康自检
- **强制重连实现**：`network/client_pool.rs` 维护 per-peer `Arc<reqwest::Client>`；强制重连 = pool.replace 让旧 Client + connection pool 一起 drop；新 Client 重新建 TCP
- **`last_successful_sync_at` 写入时机**：仅在**广播报文（clipboard / file / trust / leave / delete_history）拿到 200 OK** 时写；**不**在心跳 200 OK 时写——因 v0 实战 bug 的核心是"心跳成功 ≠ 真同步"
- **piggyback 不实现** v2.0；留 v2.1+ 评估
- **UI 锚点** (`last_successful_sync_at` 显示 + 5min 阈值黄变)：留 P2-3.c UX 段补；后端字段就位

**为什么不选 A**：仅并发心跳不解决 TCP 半死状态。

**为什么不选 C**：长连接重写超出 v2 范围。

---

## 4. 后果（Consequences）

### 4.1 正面

- **20 份 feature spec 的实现方向收敛到统一骨架**：feature ADR（P2-1.b）只需引用 ADR-003 不再重新论证 PeerState / 加密 trait / lifecycle 等；预计后续 ADR 平均长度减半
- **隐形掉线 v0 实战 bug 有了项目层根治路径**：N=3 强制重连 + M=2 健康自检 + TCP keepalive 三层兜底 + UI 锚点 让用户在表面绿但实际死透时一眼识破
- **v0 教训第 5.2 节 6 项暴露问题全部覆盖**：单文件膨胀（3.1 拆分）/ 隐式不变式（3.3 PeerState + 3.4 trait + 3.5 lifecycle 全文档化）/ 架构演化无记录（本 ADR 即 v2 第一个论证）/ 测试覆盖率 0%（3.4 trait + 3.6 boundary enum 让 mock 与单元测试可达）
- **CLAUDE.md 第 14 节 v5 规则全部落地路径化**：v5-3 严格 SDLC（本 ADR 强制 feature ADR 不得违反 3.x）/ v5-4 依赖兼容性（本 ADR 锁现状栈不引新依赖）/ v5-5 lifecycle owner（3.5 全部）/ v5-9 registry 完整性（PeerRegistry 是 spec → impl 的中心索引）/ v5-10 三向决议（spec + ADR + lifecycle 一致）
- **v4-7 fatal 三件套** 落地：3.6 panic hook + dialog + abort 全部细到代码路径

### 4.2 负面 / 妥协

- **拆分量大**：选项 B 模块切分让初次 P0 实现成本比"v0 复制改"高 30-40%；首个 feature 实现工程量集中在 PeerRegistry / lifecycle / crypto trait 三块基础设施
- **per-peer reqwest::Client 实例**：N=8 时 8 个 Client，每个 ~ 1 KB，但一旦 N 漂到 ≥ 50（产品定义不会发生）会变堆；不预防
- **AAD / zeroize / PSK 三个安全决议本 ADR 不锁定**：完全留给 ADR-008 security-reviewer；可能的副作用是 ADR-008 决定的方向影响 trait 签名（如 AAD 必须绑则 Sealer 签名稳定，不影响；如改 KeyExchange 加 PSK 步骤则需 supersede 本 ADR 第 3.4 节）
- **`is_snapshot` flag 复用 /clipboard** vs 开新端点的 trade-off：协议面积小但 `clipboard-snapshot-sync` ADR 必须确保 snapshot 与正常 broadcast 的 dedupe 共享 PeerRegistry.seen_seq_and_update 同一 kind="text"；否则会 race
- **diagnostic-logging file appender crate 选型**未定**：tracing-appender 的内置 RollingFileAppender 仅支持按时间轮转，不支持按大小；需要 implementer 在 `diagnostic-logging` ADR 选 (a) 内置按日 + 应用层 size hook 或 (b) 第三方 `tracing-rolling-file` crate；本 ADR 仅约束行为（≤ 10 MB / 文件 + 7 天保留）

### 4.3 需要警惕的副作用

- **强制重连 N=3 / M=2 / 30s debounce 是建议值**：实测可能在弱网误重连或在某种 v0 没观察到的 bug 路径下不收敛；**对策**：在 `peer-heartbeat` ADR-N 中加可观测指标（`tracing::info!(target: "health", ...)` 把每次重连/自检/剔除事件记下来），1 个月后回看是否需要 supersede 本 3.7
- **PeerRegistry approved/banned 短路缓存与 inner 主表的双写一致性**：approve / ban 时如果只改 inner 不改短路集合（或反之）会让短路命中漏 / 误命中；**对策**：`approve` / `ban` 内部用单 RwLock<HashSet> 写两次（先 inner 后 set 或反），在 PeerRegistry 单元测试覆盖原子性
- **退出路径唯一化 (3.5)** 强制 4 处入口（tray / settings / Cmd+Q / OS close）走 quit_app；如 implementer 在 `tray-integration` ADR-N 阶段先实现 P0 简化版（直接 app.exit(0)），到 P2 再升级，过渡期会有不一致 — **对策**：`tray-integration.md` 第 3 节已明确 "P0 阶段简化为 app.exit(0)，TODO 标记"，本 ADR 在 3.5 列入 P2 升级清单
- **client_pool.rs 的 Client 生命周期与 PeerRegistry 同步**：peer leave / ban / heartbeat 剔除时必须同时 pool.remove(id)；否则 zombie Client 占内存 — **对策**：PeerRegistry.remove 触发 pool.remove，集中在 `peer::registry` 一处管理
- **HKDF salt v2 bump 让 v0 prototype 与 v2 不互通**：用户 legacy-prototype 与 v2 build 不能混跑；**这是设计选择**（v0 留底分支独立测试），但需在 v2.0.0 release notes 显式标注"与 v0 不兼容"

---

## 5. 实施提示（给 implementer）

> ≤ 5 条要点。本 ADR 不替代后续 feature ADR；feature implementer 在拿到 feature ADR 后才能开工。

1. **PeerRegistry / Lifecycle / crypto traits 三块基础设施在第一个 P0 feature（cross-platform-build 不动 src，应是 `floating-window` 或 `local-ip-display`）实现前先落地**；任何 feature 实现都假定它们存在
2. **HKDF salt/info 字面量字符串 + AAD 入参签名稳定**；ADR-008 决定 AAD 绑值后只改 Sealer impl 不动 trait，避免 break 全网调用点
3. **panic hook + Tauri dialog + OS 原生 MessageBox 三层兜底**：必须在 `lifecycle.start step 1`（log init）之前注册（panic 比 log init 还早）；mac/Win cfg 隔离调原生
4. **client_pool per-peer 实例**：lookup miss → 用默认 builder 现造；插入 PeerRegistry 时同步插 pool；移除时同步移除
5. **不要做的反模式**（v0 教训提取）：
   - ❌ 把状态散在 4-5 个 HashMap（违反 3.3）
   - ❌ 任何 long-running task 直接 `tauri::async_runtime::spawn` 不归口 lifecycle（违反 3.5）
   - ❌ handler 直接 `Result<HttpStatus, anyhow::Error>` 跳过 NetworkError boundary（违反 3.6）
   - ❌ 在 broadcast 200 OK 路径外 update `last_successful_sync_at`（违反 3.7 隐形掉线核心语义）
   - ❌ 单文件 > 400 行（违反 3.1 硬约束）

---

## 6. 验证（How to Verify）

### 6.1 怎么证决策对

- **集成测试**：`peer-heartbeat` 实现完成后，模拟 "B 进程 hang 但 OS 端口仍占用" 场景（用 `pkill -STOP` macOS / `Suspend-Process` Win） → A 应在 N=3 心跳失败后强制重连 + log `forced TCP rebuild for {device_id}` → 仍失败到 5 次后剔除（peer-heartbeat.md 第 4 节 AC #9 #10）
- **诊断模式 + 日志导出闭环**：用户报 bug 时打开诊断模式 + 复现 → 导出 zip → 开发者从日志能定位 path（diagnostic-logging.md 第 4 节 AC #1-#10 全部在 v2 上能跑通）
- **AppState 单元测试可达**：mock PeerRegistry + mock crypto trait → 所有 handler 可在不起 axum / 不起 arboard 的情况下跑（单元测试覆盖率 ≥ 0% → ≥ 30%；具体阈值由 qa-tester 在 P5 决定）
- **关闭路径在 ≤ 2 秒内**：tray quit / settings quit / Cmd+Q 三种入口路径，组员都在 ≤ 1 秒内看到 leave（group-leave-notify.md 第 4 节 AC）；本 ADR 3.5 路径唯一让这条 AC 可达
- **代码体检**：3 个月后任何 .rs 文件 > 400 行视为本 ADR 第 3.1 决议被违反；CI 加 `wc -l` linter check 即可程序化

### 6.2 怎么证决策错（什么时候 supersede 本 ADR）

- **隐形掉线兜底失败**：用户在 v2 release 后 1 个月内仍报 ≥ 1 次"表面绿但实际死透 + 仅重启程序能恢复"——说明 N=3 / M=2 / keepalive=20s 不收敛，需 supersede 3.7
- **PeerRegistry 锁竞争成为热点**：tracing 日志或 perf 显示 PeerRegistry 锁等待 > 50ms / 平均，说明选项 B 的 RwLock 不够，应改 sharded 或 actor，supersede 3.3
- **每 peer 一个 reqwest::Client 内存浮点**：实测 N=5 设备 + 长跑 1 周内存增长 > 50 MB → client_pool 抽象错误，supersede 3.7 实施细节
- **boundary enum NetworkError 在 90% handler 中只用 Internal(anyhow::Error)**：说明状态码细分没价值，2.6 选项 B 过度设计，supersede 3.6 退回选项 A
- **模块拆分被 implementer 反复抱怨"找一个东西要进 5 层目录"**：组织成本超过收益，supersede 3.1 收敛到选项 A

---

## 7. 安全审阅（占位 — security-reviewer 在 ADR-008 / 本节追加）

> 本 ADR 第 3.4 / 3.6 / 3.7 节涉及 crypto / 协议 / 网络认证。CLAUDE.md 第 9 节强约束：必须经 security-reviewer ACK。
>
> security-reviewer 待审项（与 e2e-encryption.md / file-transfer-drag.md / clipboard-text-sync.md / clipboard-image-sync.md 第 7 节 [P0] [安全] 议题对齐）：
>
> 1. AAD 是否绑 `origin_device_id || seq || kind`（trait 签名已留 aad 入参）—— e2e-encryption 7 节 [P0] / clipboard-text-sync 7 节 [P0] / history-sync-delete 第 3 节 关联议题
> 2. zeroize 是否引入 + 在 PeerRegistry.remove / re-handshake 覆盖时主动清零 —— e2e-encryption 7 节 [P0]（共 2 条）
> 3. PSK / 短口令认证防主动 MITM —— e2e-encryption 7 节 [P0] + group-discovery 7 节 [P0] [安全]
> 4. content_hash = SHA-256(plaintext) → HMAC(per-pair-key, plaintext) 替换 —— clipboard-text-sync / clipboard-image-sync 7 节 [P0]（与 history-sync-delete 联动）
> 5. filename sanitize 加固（Win 保留名 / Unicode 反向覆盖字符 / 控制字符） —— file-transfer-drag 7 节 [P0]
> 6. 明文 size 与 body 实际 size 的早期验证（防 attacker 声明小 size 灌大 body）—— file-transfer-drag 7 节 [P0]
> 7. /handshake DoS 限流（同 LAN 弹框轰炸防御）—— group-discovery 7 节 [P0] / group-approval 7 节 [P0]
> 8. device_name 字符集 / 长度限制（防恶意 UTF-8 反向覆盖伪装）—— group-discovery / group-approval / settings-panel 7 节 [P0] 共 1 议题
> 9. /ping origin 校验 —— peer-heartbeat 7 节 [P1] [安全]
> 10. 日志中 device_id / device_name / IP 是否记录 —— diagnostic-logging 7 节 [P0]
>
> 主窗口在本 ADR PROPOSED → ACCEPTED 之前**必须**调 security-reviewer 出 ADR-008 或在本节追加。

**ADR-008 接管声明**（2026-05-08，security-reviewer）：

> 已由 ADR-008 接管，本节不再扩展。10 项待审议题在 ADR-008 第 3-6 节逐一决议；implementer 必修清单见 ADR-008 第 7.2 节（8 条 MUST，含 1 严重 + 5 中 + 2 中跟进）。本 ADR status `ACCEPTED_PENDING_SECURITY_SIGNOFF` 在 ADR-008 ACCEPTED 后由主窗口推进到 `ACCEPTED`。

---

## 8. 决策卡片清单（v5-11 强制 — 让用户 5 分钟拍板）

> 7 张卡片对应 3.1-3.7 七个子决策。每张含问题 + 选项（含推荐）+ 取舍 + 不做后果 + must-fix。

---

### 卡片 1 / 7 — 模块切分粒度

**问题**：v2 后端 / 前端拆几块？v0 的"单文件膨胀"反模式（server.rs 784 行 / +page.svelte 1483 行）怎么避免？

**选项**：

- A 极薄拆分（仅拆 server.rs + 前端 6 组件）— 改动最小但仍有 AppState 上帝结构
- **B 分层 + 域驱动（推荐）** — 后端 10 一级目录 + PeerRegistry/lifecycle/crypto traits 三块基础设施 + 前端 11 组件 + 4 store + 3 ipc
- C Actor 模型 — 否决（单人项目过度工程）

**取舍**：B 比 A 多 30-40% 初期工程量；换来 v0 教训第 5.2/5.4 节 5 大反模式全部解决 + 单元测试可达

**不做后果**：v2 6-12 个月后回到 v0 单文件膨胀状态；新人 30 分钟上手不可达

**must-fix**：单文件 > 400 行 = 本决策被违反；CI 加 lint check

---

### 卡片 2 / 7 — HTTP 协议总骨架

**问题**：v0 的 12 个端点 + JSON+base64 协议要不要变？是否新增 `/clipboard/snapshot`？非 PNG 图片走哪个通路？

**选项**：

- A v0 端点表沿用 + 加 1 个 snapshot 端点 — 协议面积变大
- **B v0 端点表沿用（不新增）+ 通用 header（X-SC-Device-Id/Seq/Auth 占位）+ 7 状态码统一表（推荐）** — snapshot 复用 /clipboard 加 is_snapshot flag；非 PNG 由用户拖文件走 /file
- C 切自定义二进制 framed TCP — 否决（丧失 curl 调试性，LAN 不缺带宽）

**取舍**：B 沿用 v0 + 加 header 占位让未来加认证不 break；30 字节/请求冗余在 LAN 内可忽略

**不做后果**：未来加 PSK / HMAC / 限流 必须改 body 协议（与已发布 v2 不兼容）

**must-fix**：状态码语义表（2.6 节 / 3.2 节）必须 implementer 在 NetworkError → IntoResponse 一处实现，不允许散落

---

### 卡片 3 / 7 — PeerState 数据模型

**问题**：v0 的 4 个独立 HashMap（peers / peer_keys / fail_counts / last_seen_seq）要不要合？peer-heartbeat 新增的 `last_successful_sync_at` / `consecutive_send_failures` 字段挂哪？

**选项**：

- A 保持 v0 散点 — 任何"按 peer 全态"查询要拿 4 把锁
- **B 统一 PeerState struct + PeerRegistry（推荐）** — 字段全部一处；trust 互斥语义集中实现
- C 单 god struct + 仍 4 HashMap — 没解决散点锁问题（否决）

**取舍**：B 增加 PeerRegistry 工程量；换来 5 份 spec（peer-heartbeat / group-trust-gossip / e2e-encryption / clipboard-text-sync / group-leave-notify）相关字段一处可查

**不做后果**：peer-heartbeat.md 第 4 节 新增 3 条 AC（隐形掉线）+ peer-heartbeat.md 5.4 节 PeerRegistry 议题 没有项目层支撑

**must-fix**：approve / ban 互斥语义在 PeerRegistry 内部原子写两次（先 inner 后 short-circuit set 或反），单元测试覆盖原子性

---

### 卡片 4 / 7 — 加密层抽象边界

**问题**：crypto.rs 的 6 个自由函数要不要 trait 化？密钥生命周期在哪文档化？

**选项**：

- A 保持 v0 函数式 — derive_aes_key 消费 EphemeralSecret 让单元测试不可达
- **B trait 化（KeyExchange + Sealer + Verifier 占位）+ 默认实现（推荐）** — 单元测试可达；未来切 PSK 只改 impl
- C noise_protocol 全替换 — 重写超本范围（否决）

**取舍**：B 增少量 trait 抽象代码；满足 e2e-encryption.md 第 4 节 AC #6（5 条单测覆盖）

**不做后果**：测试覆盖率仍 0%（v0 现状）；未来 ADR-008 决议 AAD/PSK/zeroize 时必须改全网调用点（v2 拐点）

**must-fix**：HKDF salt = `b"sync-copy-v2-salt"` / info = `b"sync-copy-v2:aes-256-gcm"`；bump v2 让 v0 prototype 不互通（设计选择，须在 v2.0.0 release notes 显式说明）；AAD 入参在 trait 签名预留**不**锁定值（留 ADR-008）

---

### 卡片 5 / 7 — lifecycle owner（v5-5）

**问题**：剪切板轮询 / HTTP server / 心跳 worker / 日志 flush worker 各挂哪个 runtime？启动 / 关闭顺序谁定？4 处退出路径（tray / settings / Cmd+Q / OS close）怎么收敛唯一？

**选项**：

- A 散乱 spawn（v0 现状）— 退出路径不一致是 v0 三份 spec 教训第 5.4 节同议题
- **B `app/lifecycle.rs` 集中管理 + 启动 7 步 / 关闭 7 步 + 退出路径全部走 quit_app（推荐）** — runtime 归属表清晰；满足 v5-5 lifecycle owner
- C actor supervisor — 单人项目过度工程（否决）

**取舍**：B 让 lifecycle.rs 集中职责重；换来 tray/settings/leave/heartbeat 四份 spec 5.4 节"退出路径不一致"同议题一次性收敛

**不做后果**：v2 重蹈 v0 覆辙——托盘退出不发 leave / 心跳 task 没取消机制 / 日志写盘 hang 拖死退出

**must-fix**：std::panic::set_hook 必须在 lifecycle.start step 1 之前注册；mac/Win cfg 隔离原生 dialog 兜底（v4-7 fatal 三件套）

---

### 卡片 6 / 7 — 错误处理 + 日志总策略

**问题**：handler / command 用 anyhow 还是自定义 enum？tracing 输出怎么持久化？fatal panic 怎么处理（v4-7 三件套落地）？

**选项**：

- A 全 anyhow（v0 现状）— handler 无法精确状态码 / 前端无法程序化处理
- **B 内部 anyhow + boundary enum（NetworkError → IntoResponse / CommandError → String）+ tracing-appender rolling file + std::panic::set_hook（推荐）** — 7 状态码统一 + diagnostic-logging.md 第 4 节 AC 路径化 + v4-7 三件套落地
- C 全 enum thiserror — 单人项目代码量翻倍（否决）

**取舍**：B 增 boundary 转换代码；换来前端 ipc 错误处理一处映射 + 日志可观测 + fatal 不静默

**不做后果**：v0 现状的"用户没日志可发，开发者只能猜"——00 总览 第 4 节 项目级验收 #2 无法通过

**must-fix**：日志敏感字段黑名单（剪切板明文 / AES key / X25519 私钥 / shared secret / HKDF 中间值）永不进 tracing fields（diagnostic-logging.md 第 4 节 AC #5 硬约束）

---

### 卡片 7 / 7 — 隐形掉线机制（_assumptions A_BUG_HIDDEN_DEAD）

**问题**：v0 实战 bug "peer 表面绿但实际死透，仅重启程序能恢复" 怎么解？peer-heartbeat.md 新增 3 条 AC（强制重连 / 健康自检 / 上次同步时间）的项目层参数 N / M / TCP keepalive 取多少？

**选项**：

- A v0 心跳层 + 仅改并发 ping — 不解决 TCP 半死状态根因
- **B 4 件套（推荐）** — N=3 强制重连 / M=2 健康自检 / TCP keepalive=20s / `last_successful_sync_at` UI 锚点
- C 长连接重写（WebSocket / framed TCP）— 超 v2 范围（否决）

**取舍**：B 增 client_pool per-peer Client 工程量；换来 v0 实战 bug 三层兜底（OS keepalive + 应用层强制重连 + UX 锚点黄变）

**不做后果**：v0 实战 bug 在 v2 重现；用户体验"用一会就要重启"循环

**must-fix**：
1. `last_successful_sync_at` **仅在广播报文 200 OK 时写**（不写心跳 200 OK）—— 这是隐形掉线核心语义
2. 强制重连 = `client_pool.replace(peer_id)` drop 旧 Client 让 connection pool 一起 drop（reqwest 不支持驱逐单条连接）
3. PeerRegistry.remove 触发 client_pool.remove —— 防 zombie Client

---

> 7 张卡片拍完后：本 ADR status PROPOSED → ACCEPTED；P2-1.b 进入"feature 层 ADR 分批"阶段；建议第一批 ADR 选 PeerRegistry / Lifecycle / crypto traits 三块基础设施（对应 spec：infra layer，无 spec 但本 ADR 第 3.1 / 3.3 / 3.4 / 3.5 节即 spec）。涉及 crypto / 协议 / 网络认证的 3.4 / 3.6 / 3.7 节必须先调 security-reviewer 出 ADR-008 / 在第 7 节追加签字才能 ACCEPTED。
