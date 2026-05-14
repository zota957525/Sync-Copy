---
id: ADR-009
feature_id: peer-registry
title: PeerRegistry 接口契约 / trust 互斥状态机 / 锁粒度 / client_pool 与 PolicyState 耦合
status: ACCEPTED
owner: tech-architect
date: 2026-05-08
accepted_at: 2026-05-08
security_signoff: ADR-009 第 7 节追加签字（CHANGES_REQUESTED → 4 补丁已落 v1.2）2026-05-09
deciders: [tech-architect, main, user]
user_decision_summary: 3/3 决策卡片用户 2026-05-08 拍板 1A / 2B / 3B（采纳架构师推荐）；卡 1 锁粒度选 A（单 RwLock<HashMap> + 两个 RwLock<HashSet>），卡 2 trust 互斥事件入口选 B（集中在 PeerRegistry::approve / .ban），卡 3 PolicyState 归属选 B（独立 RateLimiter）
related_specs:
  - peer-heartbeat
  - group-trust-gossip
  - group-discovery
  - group-approval
  - group-leave-notify
related_adrs:
  - ADR-003
  - ADR-008
supersedes: []
superseded_by: []
revision_history:
  - version: v1
    date: 2026-05-08
    notes: 初版 — P2-1.b 第一批第一份。把 ADR-003 第 3.3 节 PeerState/PeerRegistry 决议落到接口契约 + 状态机 + 锁粒度 + client_pool 耦合 + PolicyState 归属层面；落实 ADR-008 MUST-2 (zeroize) + MUST-4 (remove 原子顺序)
  - version: v1.1
    date: 2026-05-08
    notes: 用户拍板 1A / 2B / 3B（采纳推荐）；status PROPOSED → ACCEPTED_PENDING_SECURITY_SIGNOFF；deciders 加 [main, user]；待 security-reviewer 在第 7 节追加签字段后推 ACCEPTED
  - version: v1.2
    date: 2026-05-09
    notes: 落 security-reviewer 第 7.3 节 4 条补丁（P1 snapshot/get SECURITY 注释 / P2 health.rs 反模式 / P3 RateLimiter SECURITY 段 + per_pair 过期策略 / P4 锁顺序硬约束新增 第 3.3.1 节防 AB-BA 死锁）；status ACCEPTED_PENDING_SECURITY_SIGNOFF → ACCEPTED
depends_on_artifacts:
  - path: decisions/ADR-003-project-architecture-skeleton.md
    version: ACCEPTED 2026-05-08（第 3.3 节 / 第 3.7 节 / 第 4.3 节 副作用清单）
  - path: decisions/ADR-008-security-review-of-adr003.md
    version: ACCEPTED 2026-05-08（第 7.2 节 MUST-2 / MUST-4 + 第 8.3 节 PolicyState 归属副作用）
  - path: specs/peer-heartbeat.md
    version: v3 2026-05-08（含 last_successful_sync_at / consecutive_send_failures / 隐形掉线 3 条 AC）
  - path: specs/group-trust-gossip.md
    version: 2026-05-08 SPEC_REVIEWED
  - path: specs/group-discovery.md
    version: 2026-05-08 SPEC_REVIEWED
  - path: specs/group-approval.md
    version: 2026-05-08 SPEC_REVIEWED
  - path: specs/group-leave-notify.md
    version: 2026-05-08 SPEC_REVIEWED
---

# ADR-009 — PeerRegistry 接口契约 / 互斥状态机 / 锁粒度 / 资源耦合

> 范围：把 ADR-003 第 3.3 节决议的"统一 PeerState struct + PeerRegistry"落到 **可签编 trait / impl 形态 + 状态机转移图 + 单元测试清单**。本 ADR 不重新论证 PeerState 字段方向（ADR-003 已锁），仅细化字段类型、方法契约、互斥语义、锁粒度与 client_pool / PolicyState 耦合。

---

## 1. 上下文（Context）

### 1.1 触发本 ADR 的输入

- **ADR-003 第 3.3 节**已决"选项 B"：`PeerState struct + PeerRegistry`，附 10 字段必含清单 + 11 个方法签名草案 + approved/banned 短路缓存 + trust 互斥语义。但**未细化**：(a) 锁粒度选型（是单 RwLock<HashMap> 还是分片）；(b) approve/ban 转移的原子顺序与失败处理；(c) PolicyState（DoS 限流计数）归属。
- **ADR-008 第 7.2 节** **MUST-2**（aes_key 包 `Zeroizing`）+ **MUST-4**（PeerRegistry.remove 与 client_pool.remove 钩子顺序原子化、禁止 client_pool lazy add）需要在本 ADR 落到代码契约层。
- **ADR-008 第 8.3 节 副作用 #3**："DoS 限流的 PolicyState 也必须放入 PeerRegistry 或独立 RateLimiter；implementer 在 group-discovery feature ADR 决定单独 module 或并入 PeerRegistry" — 本 ADR 给出明确归属决议，避免推迟到 group-discovery ADR 时再回过头改 PeerRegistry 接口。
- **5 份相关 spec** 全部 SPEC_REVIEWED：peer-heartbeat（强制重连 + 健康自检 + last_successful_sync_at 写入语义）/ group-trust-gossip（trust/ban 互斥覆盖）/ group-discovery（handshake 写入 PeerRegistry 路径）/ group-approval（决定后写 approved_device_ids + 派生 aes_key）/ group-leave-notify（remove 触发条件）—— 5 份都直接调 PeerRegistry 接口，本 ADR 是它们的共享底座。

### 1.2 v0 散点 4-HashMap 教训（反面教材）

`legacy-prototype:src-tauri/src/state.rs` 把 peer 状态散在 4 个独立 RwLock<HashMap>：`peers / peer_keys / fail_counts / approved_device_ids` 各自加锁；任何"按 device_id 视角看一台 peer 全态"操作要拿 4 把锁；新增字段必须在 4-5 个文件加 4-5 处。`peer-heartbeat.md` 第 5.4 节 + `00-product-overview.md` 第 5.4 节多份 spec 反复点名。本 ADR 是该教训的具体修复路径。

### 1.3 现在不决的后果

- 后续任一 feature ADR（peer-heartbeat / group-trust-gossip / group-discovery / group-approval / group-leave-notify）都要重新论证锁粒度与 approve/ban 顺序，决策重复 + 不一致风险高。
- ADR-008 MUST-2 / MUST-4 没有"落到 trait 签名"的 ADR 兜底 → implementer 自由发挥，code-reviewer 没参照系审查。
- PolicyState 归属悬而未决，group-discovery feature ADR 落地时被迫改 PeerRegistry 接口，破坏本 ADR 的稳定性承诺。

---

## 2. 选项考虑（Options Considered）

> ADR-003 第 3.3 节已锁定"PeerState 字段集合方向 + PeerRegistry 集中管理方向"。本 ADR 仅就**两个仍有候选的子点**列选项：(a) **锁粒度**（单锁 vs 分锁 vs 分片）；(b) **PolicyState 归属**（并入 PeerRegistry vs 独立 RateLimiter）。其余子节（PeerState 字段定义 / 方法签名 / 互斥状态机 / client_pool 耦合）是 ADR-003/008 已决方向的细化，无可选项，直接进第 3 节。

### 2.1 锁粒度

#### 选项 A：单 `parking_lot::RwLock<HashMap<String, PeerState>>` + 两个独立 `RwLock<HashSet<String>>`（approved / banned 短路缓存）

- 怎么做：与 ADR-003 第 3.3 节草案一致。`inner` 一把锁；`approved` / `banned` 各一把读多写少的 RwLock；approve / ban 时按固定顺序拿两把锁完成转移。
- 优点：实现极简（< 200 行）；parking_lot 锁开销 ~ 25ns，N=8 设备场景下读路径无可观测竞争；snapshot() 返 Vec<PeerState> 用一次读锁；调试模型简单（panic 时锁状态可见）；与 v0 锁模型概念差距小，迁移层薄
- 缺点：approve / ban 的"原子覆盖"需要程序员显式按固定顺序拿锁（先 inner 后 approved/banned 或反），代码层面不变式靠注释保证；但本 ADR 第 3.3 节用集中方法封装 + 单元测试覆盖即可
- 实现复杂度：低
- 跨平台风险：无（parking_lot 跨平台）

#### 选项 B：单 `Mutex<PeerRegistryInner>`（inner / approved / banned 包成一个 struct，一把 Mutex）

- 怎么做：所有字段塞 PeerRegistryInner struct，一把 Mutex 串行所有访问；snapshot/get/insert/remove/approve/ban 都拿同一把锁
- 优点：转移**天然原子**（一把锁内连续改两个字段）；不可能错序；MUST-4 的"先 inner.remove 后 client_pool.remove"用一个临界区写完
- 缺点：**snapshot 路径会被任何写阻塞**（心跳 worker 每 10s 调一次 snapshot 遍历 N 个 peer 发 ping；同时若 approval handler 在 approve / ban，snapshot 等到 approve 完成才返）；N=8、approve 偶发场景下不可观测，但**违反 ADR-003 第 4.2 节 "PeerRegistry 锁竞争 > 50ms 即 supersede"的可观测前提**——单 Mutex 让"读 / 写竞争"完全不可区分，调试时无法证明热点是读还是写
- 实现复杂度：低
- 跨平台风险：无

#### 选项 C：sharded（按 device_id 哈希分 N 片，每片独立 RwLock）+ approved/banned 单独锁

- 怎么做：仿 `dashmap` 思路。N=4 或 N=8 片。
- 优点：高并发下吞吐高
- 缺点：v2 N 上限 8 设备（00 总览 第 5.4 节）；4 片对 8 个 key 收益 ≤ 0；引入 dashmap 是新依赖；调试复杂度上升（分片逻辑 / hash collision 推理）；snapshot 跨片要拿 N 把读锁
- 实现复杂度：中
- 跨平台风险：无
- 否决理由：N=8 不构成并发瓶颈；dashmap 引入违反 ADR-003 第 4.2 节"不引新依赖"（CLAUDE.md v5-4）；本 ADR 不引入

### 2.2 PolicyState（DoS 限流计数器）归属

> 背景：ADR-008 第 4.3 节 + MUST-7 决议 handshake DoS 限流：每对 (remote_ip, device_id) 60s 内 ≤ 3；全局 60s 内 ≤ 10 个不同 device_id。需要一个数据结构维护"过去 60s 内的 handshake 尝试时间序列 / 计数"。

#### 选项 A：并入 PeerRegistry — 在 PeerState 里加 `handshake_attempts: VecDeque<Instant>` + Registry 加 `global_handshake_attempts`

- 优点：所有 peer 维度状态一处可查；handshake handler 调一个对象拿所有信息
- 缺点：**handshake 限流的 key 是 (remote_ip, device_id)**——但 handshake 失败的请求（被限流的）**根本还没**进 PeerRegistry（PeerState 在握手成功才写入）；强行把"未成 peer 的 IP/id"塞进 PeerRegistry 等于让"陌生设备"能写本 registry，违反 PeerRegistry 的语义边界（"已认识的 peer 状态库"）；扩大 PeerRegistry 职责范围 → 对应单元测试矩阵爆炸
- 实现复杂度：中
- 否决理由：语义错配 — PeerRegistry 不该装"还没成为 peer 的 IP" 的尝试历史

#### 选项 B：独立 `network/rate_limit.rs` 模块 — `pub struct RateLimiter` 持有 `RwLock<HashMap<(IpAddr, String), VecDeque<Instant>>>` + 全局 `RwLock<VecDeque<(Instant, String)>>`

- 怎么做：`RateLimiter::check_handshake(remote_ip, device_id) -> Result<(), RateLimitDecision>`，handshake handler 第一行调；超限返 RateLimitDecision::TooManyRequests → 映射 429。RateLimiter 不依赖 PeerRegistry，不被 PeerRegistry 依赖；放在 AppState 顶层与 PeerRegistry 平行（`Arc<RateLimiter>`）。
- 优点：职责单一（handshake 限流不污染 peer 状态库）；测试隔离（RateLimiter 单测不需要 mock PeerRegistry）；group-discovery feature ADR 接管细化阈值时不动 PeerRegistry 接口；与 ADR-008 实施提示 #3"`network/rate_limit.rs`（独立单文件）"原文一致
- 缺点：AppState 多一个 Arc<RateLimiter> 字段；需要在 lifecycle.start 时构造（< 5 行代码）
- 实现复杂度：低
- 跨平台风险：无

---

## 3. 决定（Decision）

### 3.1 PeerState 完整字段定义（细化 ADR-003 第 3.3 节）

```rust
// peer/state.rs

use parking_lot::RwLock;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;
use zeroize::Zeroizing;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrustState {
    Approved,   // 已被本机 approve（来自本机决定 / trust gossip / 手动入组）
    Banned,     // 已被本机 ban
    Pending,    // 已知 peer 但 trust 状态未定（v2 实质不出现：握手成功即 Approved，但保留枚举值兼容未来 PSK 流程）
}

pub struct PeerState {
    // —— 身份标识（握手成功时一次性写入，之后只读）——
    pub device_id: String,                              // 主键；UUID 形式
    pub device_name: String,                            // 已 sanitize（ADR-008 MUST-8）
    pub addr: SocketAddr,                               // remote.ip() + req.listen_port

    // —— 加密层 ——
    pub pubkey_b64: String,                             // 调试 / re-handshake 时校验
    pub aes_key: Zeroizing<[u8; 32]>,                   // ADR-008 MUST-2 — Drop 时自动清零

    // —— 隐形掉线检测（peer-heartbeat.md v3 第 4 节 AC #9 #10 #11）——
    pub last_successful_sync_at: Option<Instant>,       // 仅在 broadcast 200 OK 时更新；不在心跳成功时写
    pub last_heartbeat_at: Option<Instant>,             // 仅调试用
    pub consecutive_heartbeat_failures: u32,            // 心跳失败累计；FAIL_LIMIT=2 / FORCE_REBUILD=3 阈值判定用
    pub consecutive_send_failures: u32,                 // 广播失败累计；SEND_FAIL_THRESHOLD=2 触发 health 自检

    // —— Trust 视角（PeerRegistry 在 approve/ban 路径维护此字段）——
    pub trust_state: TrustState,                        // 与 approved/banned 短路集合保持一致；冗余字段方便单 peer 视角观察

    // —— Replay 防御（ADR-008 4.2 节）——
    pub last_seen_seq_by_kind: HashMap<&'static str, u64>,
    // kind 字面量：text / image_png / file / trust / ban / leave / delete_history / clear_history / approval
}
```

**字段填值时机**（implementer 必读）：身份 5 字段 by `insert()`（握手成功最后一步，network/handlers/handshake.rs）；`last_successful_sync_at + consecutive_send_failures` by `record_send_ok/fail()`（network/client.rs broadcast_*）；`last_heartbeat_at + consecutive_heartbeat_failures` by `record_heartbeat_ok/fail()`（network/health.rs）；`trust_state` by `approve/ban()`（network/handlers/{approval, gossip}.rs）；`last_seen_seq_by_kind` by `seen_seq_and_update()`（所有 broadcast handler 入口第一行）。

### 3.2 PeerRegistry 接口契约

```rust
// peer/mod.rs
pub struct PeerRegistry {
    inner: RwLock<HashMap<String, PeerState>>,
    approved: RwLock<HashSet<String>>,           // 短路：subject 还没成为 peer 时也要查
    banned: RwLock<HashSet<String>>,
    client_pool: Arc<ClientPool>,                // 见 3.5；为保 remove 原子顺序内嵌持引用
}

impl PeerRegistry {
    pub fn new(client_pool: Arc<ClientPool>) -> Self;

    // 读（返 clone；aes_key 也 clone，调用方禁止落盘 / 写日志）
    /// SECURITY: 返回的 PeerState 含 aes_key（Zeroizing clone 拷贝字节）；
    /// 调用方禁止 Debug-print / tracing fields / 落盘 / 跨进程发送
    pub fn get(&self, id: &str) -> Option<PeerState>;
    /// SECURITY: 返回的 PeerState 含 aes_key（Zeroizing clone 拷贝字节）；
    /// 调用方禁止 Debug-print / tracing fields / 落盘 / 跨进程发送
    pub fn snapshot(&self) -> Vec<PeerState>;
    pub fn count(&self) -> usize;
    pub fn is_known(&self, id: &str) -> bool;
    pub fn is_approved(&self, id: &str) -> bool;
    pub fn is_banned(&self, id: &str) -> bool;

    // 写：身份生命周期
    /// 握手成功；若 id 已存在覆盖（re-handshake）。pre: caller 已校验非 banned。
    /// post: inner contains id；client_pool 由 caller 同步 insert（见 3.5 调用顺序契约）
    pub fn insert(&self, state: PeerState);

    /// 唯一允许 remove 的入口。inner.remove → client_pool.remove 原子（见 3.5 MUST-4）
    pub fn remove(&self, id: &str);
    pub fn clear(&self);                          // lifecycle.shutdown step 4

    // 写：trust 互斥（详见 3.3）
    pub fn approve(&self, id: &str);              // approved.insert + banned.remove + inner[id].trust_state = Approved
    pub fn ban(&self, id: &str);                  // banned.insert + approved.remove + (was_peer ? remove(id))
    // 伪代码字面顺序仅描述语义；实际 impl 必须按 第 3.3.1 节 锁顺序硬约束。

    // 写：可观测计数
    pub fn record_heartbeat_ok(&self, id: &str);
    pub fn record_heartbeat_fail(&self, id: &str) -> u32;
    pub fn record_send_ok(&self, id: &str);                     // 同时 update last_successful_sync_at
    pub fn record_send_fail(&self, id: &str) -> u32;

    // 写：seq dedupe — 返 true = 新；false = 重复（caller 应 200 OK 静默丢）
    pub fn seen_seq_and_update(&self, id: &str, kind: &'static str, seq: u64) -> bool;
}
```

**契约 invariants**：(1) `approved ∩ banned = ∅`；(2) `inner[id].trust_state == Approved ⟺ approved.contains(id)`；(3) `client_pool.contains(id) == inner.contains_key(id)`（3.5 MUST-4）；(4) 任何返 PeerState 的方法返 clone — 调用方禁止落盘 / 写日志（ADR-008 第 8.3 节 副作用 #2）；(5) `seen_seq_and_update` 必须在 handler 第一行（早于解密 / sanitize / 业务），保 replay 短路。

### 3.3 Trust 互斥状态机

**状态集**：`Unknown(不在 inner)` / `Approved(in approved)` / `Banned(in banned)` / `Pending`（保留为未来 PSK 流程；v2 实质不出现）。

**事件 → 转移表**（锁顺序全模块固定 `inner > approved > banned`，违反则 deadlock 检测器抓出）：

| 触发事件 | 来源 spec | 调用 | 转移 | 副作用 |
|---|---|---|---|---|
| 握手 approve（本机/forward 回流） | group-approval | `insert(state) + approve(id)` | Unknown → Approved | client_pool insert；caller emit handshake-dismissed |
| 握手 reject | group-approval | `ban(id)` | Unknown → Banned | inner 不变；仅入 banned 短路 |
| `/peers/trust` 收到 | group-trust-gossip | `approve(id)` | (Unknown/Banned) → Approved | trust 覆盖 ban |
| `/peers/ban` 收到，subject 不在 inner | group-trust-gossip | `ban(id)` | (Unknown/Approved) → Banned | ban 覆盖 trust |
| `/peers/ban` 收到，subject 在 inner | 同上 | `ban(id)` 内部触发 `remove(id)` | Approved → Banned | inner.remove + client_pool.remove + caller emit status-updated |
| `/peers/leave` 收到 | group-leave-notify | `remove(id)` | Approved → Unknown | 不动 banned/approved |
| 心跳累计 N=5 失败 | peer-heartbeat | `remove(id)` | Approved → Unknown | caller emit status-updated |
| 用户 quit_app | lifecycle.shutdown | `clear()` | 全部 → Unknown | approved/banned 不持久化 |

**原子性保证（ADR-008 MUST-4 落地）**：approve / ban 实现固定锁顺序，写锁内连续完成"互斥覆盖 + inner 字段同步"；ban 在 was_peer = true 时同步触发 inner.remove → 临界区释放后 client_pool.remove（保 invariant 3）。caller 在 ban / remove 返回后 emit status-updated（PeerRegistry 不依赖 Tauri AppHandle）。

### 3.3.1 锁顺序硬约束（防 AB-BA 死锁）

approve / ban 实现层固定按 **approved 锁先于 banned 锁** 的顺序拿
（即与字段声明顺序一致）。书写顺序：

```rust
// approve(id) 实现
let mut a = self.approved.write();
let mut b = self.banned.write();
a.insert(id.into());
b.remove(id);

// ban(id) 实现
let mut a = self.approved.write();
let mut b = self.banned.write();
a.remove(id);
b.insert(id.into());
```

注意 ban 的字面顺序虽与第 3.2 节伪代码 `banned.insert + approved.remove`
字面相反，但**锁的取得顺序必须遵循声明序**（先 approved 后 banned），
否则与 approve 形成 AB-BA 死锁——dev profile 的 parking_lot 死锁检测器
能抓，release build 会卡死。第 6.1 节单测 #13 已覆盖该 race。

### 3.4 锁粒度 — 选 选项 A（单 RwLock<HashMap> + 两个独立 RwLock<HashSet>）

**为什么不选 B**：单 Mutex 让"snapshot 读路径被任何写阻塞"成不可观测的隐形成本；ADR-003 第 4.2 节 supersede 触发条件 "锁等待 > 50ms / 平均"在单 Mutex 下无法用现有 tracing 区分读/写竞争来源。

**为什么不选 C**：N=8 设备不构成并发瓶颈；dashmap 是新依赖（违反 v5-4）；snapshot 跨片成本不必要。

**配套约束**：

- **锁顺序全局固定**：`inner > approved > banned`（按字段在 PeerRegistry 中声明顺序；implementer 在写多锁路径时按此顺序拿）
- **不允许在 RwLock 临界区内调用任何可能阻塞 I/O 的代码**（reqwest / 文件 / Tauri emit）；emit / network 操作必须在锁释放后做
- **snapshot 与 get 用读锁**；任何写路径用写锁；不混合
- **observable**：在 `peer/registry.rs` 顶部注释里维护"锁等待观测点"——code-reviewer 在 PR 阶段如发现"写锁持锁 > 100µs"路径就上报；实测靠 tracing `tracing::trace!(target: "peer::registry::lock", ...)`

### 3.5 与 client_pool 的接口契约（落实 ADR-008 MUST-4）

```rust
// network/client_pool.rs

pub struct ClientPool {
    pool: RwLock<HashMap<String, Arc<reqwest::Client>>>,
}

impl ClientPool {
    pub fn new() -> Self;

    /// 仅在握手成功路径调用；禁止 lazy add（ADR-008 5.1 节 MUST-4 第 3 条）
    pub fn insert(&self, id: &str, client: Arc<reqwest::Client>);

    /// 由 PeerRegistry::remove 内部调用（且仅由它调用）
    /// 该方法不暴露给 handler / health worker — 通过 PeerRegistry 中转
    pub(crate) fn remove(&self, id: &str);

    /// 强制重连（peer-heartbeat 第 3.7 节）：drop 旧 Client + 用 builder 现造新 Client 替换
    /// 仅由 network/health.rs 在 FORCE_REBUILD_LIMIT=3 触发时调用
    /// pre: caller（health.rs）已验证 id 仍在 PeerRegistry 且不在 banned（ADR-008 5.3 节）
    pub fn replace(&self, id: &str);

    pub fn get(&self, id: &str) -> Option<Arc<reqwest::Client>>;
}
```

**调用顺序契约**（落实 ADR-008 MUST-4）：

| 路径 | 步骤 | 调用方 |
|---|---|---|
| 握手成功 | 1. 派生 aes_key → 2. 构造 PeerState → 3. 构造 reqwest::Client → 4. `client_pool.insert(id, client)` → 5. `registry.insert(state)` | network/handlers/handshake.rs |
| Peer remove（leave / ban / heartbeat 剔除 / quit_app） | 1. `inner.remove(id)`（PeerState drop → Zeroizing 清零）→ 2. `client_pool.remove(id)` —— 在同一 `PeerRegistry::remove` 函数内严格此顺序 | PeerRegistry::remove 内部 |
| 强制重连（FORCE_REBUILD_LIMIT=3） | 1. health.rs 校验 `registry.is_known(id) && !registry.is_banned(id)` → 2. `client_pool.replace(id)` → 3. spawn re_handshake | network/health.rs |

**禁止的反模式**：

- ❌ 任何 handler 在 PeerRegistry::remove 之外的路径直接调 `client_pool.remove(id)`（破坏原子顺序，破坏 invariant 3）
- ❌ `client_pool.get(id)` miss 时自动构造新 Client 插入（lazy add；ADR-008 MUST-4 第 3 条禁止；只允许在握手成功路径 insert）
- ❌ 在 reqwest 请求路径中长时间持有 `client_pool` 写锁（应该 `.get(id).clone()` 拿 Arc 后立即释放锁）

### 3.6 PolicyState（DoS 限流）归属 — 选 选项 B（独立 RateLimiter）

**决议**：`network/rate_limit.rs` 独立模块；不并入 PeerRegistry。

**为什么不选 A**：handshake 限流的 key 是 (remote_ip, device_id)，但**被限流的请求根本还没成为 peer**——把陌生 IP / 未知 device_id 的尝试历史塞进 PeerRegistry 违反 PeerRegistry "已认识的 peer 状态库" 语义边界，且让 PeerRegistry 单测矩阵爆炸（要 mock 陌生 IP 注入路径）。

**接口契约**（仅给签名草案，细化阈值由 group-discovery feature ADR 接管）：

```rust
// network/rate_limit.rs

/// SECURITY: per_pair / global 容器的 device_id 来自未认证报文；
/// group-discovery feature ADR 在锁定阈值时**必须同步定义**
/// per_pair HashMap 的容量上限与过期 retain 策略，避免
/// (IpAddr, 编造 UUID) 的 HashMap 内存放大攻击。
/// 未认证 device_id 不进 tracing fields；仅 check_handshake
/// 返 TooManyRequests 时记 IP + 计数，不记 device_id。
pub struct RateLimiter {
    per_pair: RwLock<HashMap<(IpAddr, String), VecDeque<Instant>>>,
    global: RwLock<VecDeque<(Instant, String)>>,    // (timestamp, device_id) 全局新增 device_id 的尝试历史
}

#[derive(Debug)]
pub enum RateLimitDecision {
    Ok,
    TooManyRequests,            // → 429
}

impl RateLimiter {
    pub fn new() -> Self;
    /// pre: handshake handler 第一行调；超限返 TooManyRequests → 映射 429
    /// 阈值具体值（每对 60s ≤ 3 / 全局 60s ≤ 10）由 group-discovery feature ADR 锁定
    pub fn check_handshake(&self, remote_ip: IpAddr, device_id: &str) -> RateLimitDecision;
}
```

**AppState 顶层结构**（与 PeerRegistry 平行，不耦合）：

```rust
// app/state.rs
pub struct AppState {
    pub peers: Arc<PeerRegistry>,
    pub rate_limiter: Arc<RateLimiter>,
    pub history: Arc<History>,
    pub config: Arc<RwLock<Config>>,
    // ... 通信句柄
}
```

---

## 4. 后果（Consequences）

### 4.1 正面

- **ADR-008 MUST-2 + MUST-4 闭环到 trait 签名**：implementer 拿到 ADR-009 后无解释空间——`aes_key: Zeroizing<[u8; 32]>` 写死；`PeerRegistry::remove` 内部顺序写死；`client_pool.remove` 不暴露公共接口
- **5 份 spec 共享底座**：peer-heartbeat / group-trust-gossip / group-discovery / group-approval / group-leave-notify 直接调本 ADR 的 13 个 PeerRegistry 方法 + 4 个 ClientPool 方法 + 1 个 RateLimiter 方法；feature ADR 不重复论证
- **trust 互斥语义集中实现 + 单元测试可达**：approve/ban 转移在 8 行 Rust 内完成 + 锁顺序固定；`group-trust-gossip.md` 第 5.4 节"互斥覆盖不变式必须明文 ADR" 闭环
- **PolicyState 归属决议**：group-discovery feature ADR 接手时只锁定阈值具体值（60s ≤ 3 / 60s ≤ 10），不动 PeerRegistry 接口；本 ADR 兑现 "稳定接口" 承诺
- **锁竞争可观测**：选项 A 的 inner / approved / banned 三把锁让 tracing 能区分"是 inner 写阻塞还是短路集合写阻塞"，未来 ADR-003 第 4.2 节 supersede 阈值（>50ms）触发时定位精准

### 4.2 负面 / 妥协

- **三把锁的"原子组操作"靠程序员遵守锁顺序**（inner > approved > banned）；如 implementer 误序会死锁。**对策**：approve/ban/remove 三处集中实现 + 单元测试覆盖锁顺序违反时的 deadlock 检测（用 `parking_lot::deadlock` 检测器在 dev build 跑）
- **snapshot 返 Vec<PeerState> clone**：N=8 时每次 ~ 1KB × 8 + Zeroizing<[u8;32]> clone × 8（含密钥拷贝）；心跳每 10s 一次 → 800B/s clone 带宽，不可观测；但**Zeroizing 字段被 clone 等于密钥多了一份在内存里**，调用方不能落盘 / 不能写日志（ADR-008 第 8.3 节 副作用 #2 已警告）；本 ADR 在 PeerRegistry::snapshot 文档里强制约束
- **client_pool.remove 不公开**：implementer 必须经 PeerRegistry::remove 中转；如未来出现"想 remove client 但保留 PeerState"场景需 supersede 本 ADR
- **RateLimiter 单独 module 增加 AppState 字段**：1 个 Arc<RateLimiter>；可接受
- **TrustState::Pending 实质未用**：保留为未来 PSK / 二次确认扩展位；当前路径只有 Approved / Banned / Unknown 三态有意义；冗余字段

### 4.3 需要警惕的副作用

- **锁顺序违反在调试 build 才能用 parking_lot 死锁检测器抓出**；release 不开。**对策**：CI 在 dev profile 跑 `cargo test --features parking_lot/deadlock_detection`
- **ban + was_peer = true 路径的 emit status-updated 在 PeerRegistry 外部做**：因 PeerRegistry 不依赖 Tauri AppHandle（保持纯逻辑层）；caller（network/handlers/gossip.rs::handle_ban）必须在调 `registry.ban(id)` 后**手动 emit**；如漏 emit 则前端浮窗状态点不更新——code-reviewer 重点检查 `.ban(` 调用点的下一行是否 emit
- **client_pool.replace 路径未持 PeerRegistry 锁**：health.rs 校验 banned 与 replace 之间存在窗口期（5.3 节竞争）；窗口内若另一线程 ban 同 id 会让 replace 在已 ban 的 peer 上跑——**对策**：health.rs 内 replace 后再次校验 `registry.is_known(id) && !registry.is_banned(id)`，若失败立即 `client_pool.remove(id)` 善后
- **Zeroizing<[u8;32]> Clone 拷贝字节**（Drop 清零但 Clone 不阻）：snapshot 临时副本由 PeerRegistry::snapshot 文档强制"不写日志 / 不落盘"；RateLimiter **per_pair HashMap 过期清理 + 容量上限** 与 **global VecDeque 过期 retain 策略** 并列由 group-discovery feature ADR 实现（前者防 (IpAddr, 编造 UUID) 内存放大攻击，后者防全局尝试历史无界增长）

---

## 5. 实施提示（≤ 5 条）

1. **`peer/state.rs` + `peer/mod.rs` 两文件落地**；`peer/state.rs` 仅 PeerState struct + TrustState enum + 字段填值时机注释；`peer/mod.rs` 是 PeerRegistry impl + 单元测试。两文件合计 ≤ 400 行（ADR-003 第 3.1 节硬约束）
2. **`Zeroizing<[u8;32]>`**：`Cargo.toml` 加 `zeroize = { version = "1.8", default-features = false, features = ["zeroize_derive"] }`；仅 `peer/state.rs` `use zeroize::Zeroizing`，其它模块不直接依赖
3. **三把锁的获取始终通过 approve/ban/remove 三个集中方法**；handler 不直接拿 inner/approved/banned 锁；`pub` 暴露面 = 第 3.2 节签名清单（13 个）
4. **client_pool.remove 标 `pub(crate)`**；只让 `peer/mod.rs::PeerRegistry::remove` 调到；模块外不可见
5. **不要做的反模式**：
   - ❌ 在 handler 里同时调 `registry.remove(id)` 和 `client_pool.remove(id)`（破坏原子顺序）
   - ❌ `client_pool.get(id)` miss 时构造默认 Client 自动插入（lazy add 禁令）
   - ❌ 把 RateLimiter 状态塞进 PeerRegistry（PolicyState 归属决议第 3.6 节）
   - ❌ 在 RwLock 临界区内调 reqwest / Tauri emit（持锁 I/O 反模式）
   - ❌ 任何方法返 PeerState 或其字段后写日志 / 落盘（密钥泄露反模式）
   - ❌ health.rs 在调 client_pool.replace 之前未校验 `registry.is_known(id) && !registry.is_banned(id)`
     （A3 zombie peer 复活路径，ADR-008 第 5.3 节必修）

---

## 6. 验证（How to Verify）

### 6.1 怎么证决策对（单元 + 集成测试）

**PeerRegistry 单元测试 list（implementer 必备 ≥ 14 条）**：

1. `insert_then_get` — insert 后 get 返同字段；count == 1
2. `remove_clears_inner_and_pool` — remove 后 inner 不含；client_pool 不含；mock ClientPool 记调用顺序断言
3. `approve_atomic` — approved.contains && !banned.contains && trust_state == Approved 三 invariant 同时满足
4. `ban_atomic_was_peer` — insert 后 ban → inner 不含（即时踢出）+ banned + !approved + client_pool 不含
5. `ban_atomic_unknown` — 直接 ban 不 insert → banned + inner/pool 不变
6. `trust_overrides_ban` — ban 后 approve → approved && !banned
7. `ban_overrides_trust` — approve 后 ban → banned && !approved
8. `seen_seq_dedupe` — seq=5 返 true；再调返 false；seq=6 返 true
9. `record_send_ok_updates_last_sync` — last_successful_sync_at.is_some()
10. `record_heartbeat_ok_does_not_update_last_sync` — last_successful_sync_at 保持 None（落实 ADR-008 5.2 节语义）
11. `record_heartbeat_fail_increment` — × 3 返 1, 2, 3
12. `clear_all` — count == 0；approved/banned 都空
13. `lock_order_no_deadlock` — parking_lot deadlock 检测器在 dev profile 跑 approve/ban/remove 并发 100 次不死锁
14. `aes_key_zeroize_after_remove` — best-effort 观察 drop 后字节模式覆盖（跨平台不强制）

**ClientPool 单测**：`insert_get` / `replace_drops_old_client`（旧 Arc 计数归零）；`remove_pub_crate`（编译期保证仅 PeerRegistry 可调）

**集成测试**：与 group-trust-gossip / peer-heartbeat / group-leave-notify feature ADR 协同覆盖跨设备 broadcast、心跳剔除、leave 触发 remove 的端到端原子性

### 6.2 怎么证决策错（supersede 触发）

- **三把锁顺序错配引发死锁**（prod 中用户报"应用卡住"+parking_lot 等锁 > 5s）→ supersede 3.4 改单 Mutex
- **snapshot N=8 持锁 > 50ms**（tracing 记到）→ ADR-003 4.2 节触发；改 sharded 或拆 inner
- **Zeroizing<aes_key> clone 在 snapshot Vec 路径泄露**（implementer 漏检写日志）→ 升级类型禁止（PeerState clone 剥 aes_key 单独 KeyHandle 包装）；supersede 3.1
- **RateLimiter 独立模块被发现需查 PeerRegistry 状态**（跨模块依赖）→ supersede 3.6 改并入
- **TrustState::Pending 未来 1 年未用** → 简化为二态；轻量 supersede 3.1

---

## 7. 安全审阅 (by security-reviewer · 2026-05-08)

**结论**：CHANGES_REQUESTED（4 条小补丁；非阻塞主路径，可与 implementer PR 合并落地）

### 7.1 审阅范围

- 聚焦：MUST-2 zeroize 落地 / MUST-4 remove 原子顺序 / 强制重连 banned 校验 / RateLimiter 安全边界 / 状态机并发正确性
- 已审过的方向不重复审：算法选型 / nonce 处理 / AAD 绑值 / 状态码语义（ADR-008 第 3 / 4 节已审）
- 未涉及新威胁主体；威胁模型沿用 ADR-008 第 2 节（A1 LAN 监听 / A2 恶意 LAN peer / A3 已被踢除的 zombie peer）

### 7.2 审阅意见

1. **MUST-2 zeroize 落到 PeerState** — ✅ APPROVED。第 3.1 节 `aes_key: Zeroizing<[u8; 32]>` 类型签名到位；填值时机表点出 `insert()` 写、`remove()` drop 时清零的时机；第 4.3 节副作用第 4 条 + 第 5 节实施提示 #5 + 第 6.1 节单测 #14 三处一致地约束 PeerState clone 路径"不写日志 / 不落盘"。**仅小遗漏**：snapshot() 文档串口约束散在 3 处（3.2 invariant #4 / 4.2 妥协 #2 / 5 实施提示 #5），未集中到 snapshot() 方法注释里 — 见补丁 P1。
2. **MUST-4 remove 原子顺序** — ✅ APPROVED。第 3.5 节调用顺序契约表明文写 "1. inner.remove → 2. client_pool.remove 在同一 PeerRegistry::remove 函数内严格此顺序"；client_pool.remove 标 `pub(crate)` 强制单一入口；第 5 节实施提示 #5 列出 lazy add 反模式；第 6.1 节单测 #2 用 mock 断言调用顺序。MUST-4 三条（顺序 / 单入口 / 禁 lazy add）全部闭环。
3. **强制重连前校验 banned**（ADR-008 第 5.3 节必修） — ⚠ APPROVED-with-nit。第 3.5 节 `replace` 契约 pre 写明 "caller（health.rs）已验证 id 仍在 PeerRegistry 且不在 banned"；第 4.3 节副作用第 3 条还加了 "replace 后再次校验" 兜底（覆盖 health 校验与 ban 之间的窗口期），优于 ADR-008 5.3 节原始要求。**小遗憾**：第 5 节实施提示未把这条列入"反模式黑名单"（与 lazy add 同级）— 见补丁 P2。
4. **RateLimiter 安全边界** — ⚠ CHANGES_REQUESTED。第 3.6 节 RateLimiter 收 (remote_ip, device_id)，但 device_id 来自**未认证**的 handshake 报文。审阅检查项：
   - **(a) 持久结构内存放大攻击**：`per_pair: HashMap<(IpAddr, String), VecDeque<Instant>>` 的 key 含攻击者可控 device_id；同 IP 内编造 N 个不同 UUID 可让 HashMap 无界增长（A2 威胁主体 LAN 内即可发起）。第 4.3 节副作用第 4 条仅提到 "全局 VecDeque 过期 retain"，未点 per_pair HashMap 的过期清理 / 容量上限。
   - **(b) 全局阈值（60s ≤ 10 个不同 device_id）反而放大**：攻击者编造 11 个不同 device_id → 触发全局上限 → 让正常的 11 号陌生新设备（合法用户）被拒入组。这是 ADR-008 7.2 节 MUST-7 已知行为，非本 ADR 引入，但 RateLimiter 接口层应当暴露 "global 是否被打满" 的可观测 metric 让 group-discovery feature ADR 能选 "全局降级到只信白名单 IP" 兜底。
   - **(c) 日志写盘**：device_id 是 UUID，按 ADR-008 第 6.2 节决议"可记"；但**未认证**的 device_id（限流路径上还没 handshake 成功）若进 tracing fields，会让攻击者通过编造 UUID 污染日志体积 → 见补丁 P3。
5. **状态机并发互斥** — ⚠ CHANGES_REQUESTED。第 3.3 节固定全局锁顺序 `inner > approved > banned`，approve 与 ban 并发场景在**严格按声明顺序拿锁**前提下被序列化，invariant `approved ∩ banned = ∅` 成立。但本 ADR 第 3.2 节写 `ban = banned.insert + approved.remove`（书写顺序与全局锁顺序**反向**）——若 implementer 照字面顺序写代码（先 banned 后 approved），与 approve（先 approved 后 banned）形成 AB-BA 死锁；deadlock 检测器在 dev profile 抓得出，但 release 跑会卡死。这是 implementer 落地高风险点 — 见补丁 P4。

### 7.3 必修补丁（4 条，最小修订）

- **P1（第 3.2 节）**：在 `pub fn snapshot(&self) -> Vec<PeerState>;` 与 `pub fn get(&self, id: &str) -> Option<PeerState>;` 两个方法签名上方追加注释一行：`/// SECURITY: 返回的 PeerState 含 aes_key（Zeroizing clone 拷贝字节）；调用方禁止 Debug-print / tracing fields / 落盘 / 跨进程发送`。
- **P2（第 5 节实施提示 #5 反模式列表）**：追加一条 "❌ health.rs 在调 client_pool.replace 之前未校验 `registry.is_known(id) && !registry.is_banned(id)`（A3 zombie peer 复活路径，ADR-008 第 5.3 节必修）"。
- **P3（第 3.6 节 RateLimiter 接口契约 + 第 4.3 节副作用）**：(a) 在 RateLimiter struct 注释加 SECURITY 段："per_pair / global 容器的 device_id 来自未认证报文；group-discovery feature ADR 在锁定阈值时**必须同步定义** per_pair HashMap 的容量上限与过期 retain 策略，避免 (IpAddr, 编造 UUID) 内存放大"；(b) 加注 "未认证 device_id 不进 tracing fields；仅 check_handshake 返 TooManyRequests 时记 IP + 计数，不记 device_id"；(c) 第 4.3 节副作用第 4 条把 "per_pair HashMap 过期清理 + 容量上限" 与 "global VecDeque 过期 retain" 并列写明。
- **P4（第 3.3 节状态机表 + 第 5 节实施提示）**：在第 3.3 节"原子性保证"段后追加一句："**approve / ban 实现层固定按 approved 锁先于 banned 锁的顺序拿**（即与字段声明顺序一致）。`ban` 实现 = `let mut a = approved.write(); let mut b = banned.write(); a.remove(id); b.insert(id.into());`——书写顺序虽与第 3.2 节伪代码 `banned.insert + approved.remove` 字面相反，但锁顺序必须遵循声明序，否则与 approve 形成 AB-BA 死锁。" 第 6.1 节单测 #13 已覆盖该 race，无需补单测。

### 7.4 结论

CHANGES_REQUESTED — ADR-008 已审方向（MUST-2 / MUST-4 / 5.3 节）在本 ADR 全部闭环；4 条补丁均为"在已写决议旁补一段约束注释"，不动决策本身、不影响接口签名、不增加 implementer 工作面。补丁落定后即可推 ACCEPTED。

---

## 8. 决策卡片清单（v5-11 — 让用户 5 分钟拍板）

> 仅 3.3 / 3.4 / 3.6 是有可选项的关键拍板点。3.1 / 3.2 / 3.5 是 ADR-003 + ADR-008 已决方向的细化，无可选项不出卡片。

### 卡片 1 / 3 — 锁粒度（第 3.4 节）

**问题**：PeerRegistry 内部锁粒度选哪种？

**选项**：

- **A**: 单 RwLock<HashMap> + 两个独立 RwLock<HashSet>（approved / banned）
- **B**: 单 Mutex<PeerRegistryInner>（inner / approved / banned 包一起一把锁）
- **C**: sharded（dashmap 风格 N 片）

**推荐**：A

**取舍**：
- A：snapshot 读路径与 approve/ban 写路径不互斥；锁竞争可观测（tracing 区分三把锁来源）；锁顺序靠程序员按 inner > approved > banned 遵守 → 单元测试 + parking_lot 死锁检测器兜底
- B：approve/ban 转移天然原子，但 snapshot 被任何写阻塞，ADR-003 第 4.2 节 supersede 阈值（锁等待 > 50ms）触发时无法区分读/写竞争来源
- C：N=8 不构成并发瓶颈；引入 dashmap 违反 v5-4（不引新依赖）

**must-fix**：选 A 后，approve / ban / remove 三方法实现必须按固定锁顺序（inner > approved > banned）+ 加单元测试 #14（parking_lot 死锁检测 dev profile 跑并发 100 次）

### 卡片 2 / 3 — Trust 互斥状态机的事件入口（第 3.3 节）

**问题**：trust 互斥（approved/banned 永远互斥）应该在哪一层强制？

**选项**：

- **A**: 在每个 handler（handle_trust / handle_ban / handle_handshake_approve / handle_handshake_reject）内自己写 "approved.insert + banned.remove"（v0 现状）
- **B**: 集中在 PeerRegistry::approve / .ban 两个方法内；handler 只调一行（本 ADR 推荐）
- **C**: 引入 TrustStateMachine 状态机抽象（输入事件 → 输出新状态）；PeerRegistry 内嵌 SM 实例

**推荐**：B

**取舍**：
- A：v0 教训 — 互斥语义只在 handler 内一行，maintainer 漏 banned.remove 让设备同时在两个集合（group-trust-gossip 第 5.2 节明确点名）
- B：单元测试可达（test #6 + #7 直接覆盖互斥）；handler 调用面收敛到 1 行 `registry.approve(id)`；锁顺序集中在 PeerRegistry 内部
- C：状态机抽象对 4 状态 × 7 事件矩阵收益不足；增加调试与代码量；**否决**

**must-fix**：选 B 后，handle_trust / handle_ban / handle_handshake_approve / handle_handshake_reject 四 handler 必须 **只**调 registry.approve(id) 或 registry.ban(id)，不允许独立操作 approved / banned 集合

### 卡片 3 / 3 — PolicyState（DoS 限流）归属（第 3.6 节）

**问题**：handshake DoS 限流的状态（per-pair / 全局计数器）放哪？

**选项**：

- **A**: 并入 PeerRegistry（在 PeerState 加 handshake_attempts 字段 + Registry 加 global_handshake_attempts）
- **B**: 独立 `network/rate_limit.rs` 模块；AppState 顶层与 PeerRegistry 平行（本 ADR 推荐）
- **C**: 跟 PeerRegistry 同 module 但独立 struct（peer/rate_limit.rs）共享一个 mod 边界

**推荐**：B

**取舍**：
- A：限流的 key 是 (remote_ip, device_id)，但被限流的请求**还没**成为 peer；让陌生 IP 写 PeerRegistry 违反"已认识的 peer 状态库"语义；单测矩阵爆炸
- B：与 ADR-008 实施提示 #3 原文一致 (`network/rate_limit.rs` 独立单文件)；group-discovery feature ADR 接手只锁阈值不动 PeerRegistry 接口；测试隔离
- C：与 B 在物理边界上区别不大，但语义上仍把限流绑死在 peer module；如未来限流逻辑要服务 /file 端点（非 peer 维度限流），module 边界更尴尬

**must-fix**：选 B 后，AppState 加 `rate_limiter: Arc<RateLimiter>` 字段；具体阈值（每对 60s ≤ 3 / 全局 60s ≤ 10）由 group-discovery feature ADR 接手锁定，本 ADR 不锁

## 9. 自查

**过度工程**：本 ADR 行数控制在 500 行内；不重复 ADR-003 第 3.3 节字段方向论证；状态机不引入 TrustStateMachine 抽象（卡片 2 C 已 reject）；不引新依赖（zeroize 是 ADR-008 已决）；决策卡片仅 3 张（覆盖 3.4 / 3.3 / 3.6 真正可选点；3.1 / 3.2 / 3.5 不出卡 — ADR-003/008 已锁方向）。

**owner 边界**：只写 trait / struct 签名 + 状态机表 + 单测 list；未写 .rs 实现代码；未改 spec 第 1-7 节业务范围；未改 PLAN.md（建议见汇报）；未调用任何 agent。

**v5 规则镜像**（CLAUDE.md 第 14 节）：v5-3 严格 SDLC（依赖 ADR-003+008，不跳步）；v5-4 不引新依赖；v5-5 PeerRegistry 由 lifecycle.start 构造、shutdown step 4 clear；v5-9 本 ADR 即 PeerRegistry registry；v5-10 三向决议（last_successful_sync_at 仅广播 200 OK 时写在 spec / ADR-008 5.2 / 本 ADR 单测 #10 三处一致）；v5-11 决策卡片 3 张含 问题/选项/推荐/取舍/must-fix；v5-12 章节符号禁令遵守。

**状态机制**：PROPOSED → 主窗口可选调 security-reviewer 在第 7 节签字段 → ACCEPTED → P2-1.b 第二批；reviewer 若判已 ADR-008 闭环 → 直接 ACCEPTED。
