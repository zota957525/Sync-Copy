//! PeerRegistry — 统一 peer 状态库
//! see specs/peer-heartbeat.md, decisions/ADR-009-peer-registry.md
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-2 zeroize, MUST-4 remove 原子顺序)
//! see specs/clipboard-text-sync.md (PR-5b 修 ADR-008 MUST-4 契约违反)
//!
//! 设计决策摘要（ADR-009 第 3 节）：
//! - 锁粒度选 A：单 RwLock<HashMap<String, PeerState>> + 两个独立 RwLock<HashSet<String>>
//! - trust 互斥集中在 PeerRegistry::approve / .ban（选项 B）
//! - PolicyState（DoS 限流）独立 RateLimiter（选项 B，见 peer/rate_limit.rs）
//! - client_pool 内嵌 Arc<ClientPool>：PeerRegistry::remove / ban 内部调 client_pool.remove
//!   保证 ADR-008 MUST-4 + ADR-009 第 3.5 节 invariant 3 闭环。
//!
//! 锁顺序硬约束（防 AB-BA 死锁，ADR-009 第 3.3.1 节）：
//!   inner > approved > banned（按字段声明序一致）
//! 任何同时拿多把锁的代码路径必须严格遵循此顺序。
//!
//! SECURITY 约束（ADR-009 第 3.2 节 invariants）：
//! (1) approved ∩ banned = ∅
//! (2) inner[id].trust_state == Approved ⟺ approved.contains(id)
//! (3) client_pool.contains(id) == inner.contains_key(id)（MUST-4 原子保证）
//! (4) 任何返 PeerState 的方法返 clone；调用方禁止落盘 / 写 tracing fields

pub mod rate_limit;
pub mod sanitize;

use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use zeroize::Zeroizing;

use crate::app::client_pool::ClientPool;
use crate::crypto::AadKind;

// ---------------------------------------------------------------------------
// TrustState enum（ADR-009 第 3.1 节）
// ---------------------------------------------------------------------------

/// Peer 信任状态。
///
/// - Approved：已被本机 approve（来自本机决定 / trust gossip / 手动入组）
/// - Banned：已被本机 ban
/// - Pending：已知 peer 但 trust 状态未定
///   （v2 实质不出现：握手成功即 Approved；保留枚举值兼容未来 PSK 流程）
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrustState {
    Approved,
    Banned,
    Pending,
}

// ---------------------------------------------------------------------------
// PeerState struct（ADR-009 第 3.1 节）
// ---------------------------------------------------------------------------

/// 单个 peer 的完整状态快照。
///
/// 字段填值时机（ADR-009 第 3.1 节"字段填值时机"段）：
/// - 身份 5 字段（device_id / device_name / addr / pubkey_b64 / aes_key）：
///   由 `insert()`（握手成功最后一步）写入，之后只读。
/// - last_successful_sync_at / consecutive_send_failures：
///   由 `record_send_ok/fail()`（broadcast 200 OK / 失败时）更新。
/// - last_heartbeat_at / consecutive_heartbeat_failures：
///   由 `record_heartbeat_ok/fail()`（心跳 OK / 失败时）更新。
/// - trust_state：由 `approve/ban()` 更新；与 approved/banned 短路集合保持一致。
/// - last_seen_seq_by_kind：由 `seen_seq_and_update()`（所有 broadcast handler 入口第一行）更新。
///
/// SECURITY（ADR-009 第 3.2 节 P1 补丁）：
/// Clone 操作会拷贝 aes_key 字节；调用方禁止 Debug-print / tracing fields / 落盘 / 跨进程发送。
#[derive(Clone)]
pub struct PeerState {
    // —— 身份标识（握手成功时一次性写入，之后只读）——
    /// 主键；UUID 形式。
    pub device_id: String,
    /// 已 sanitize（ADR-008 MUST-8）。
    pub device_name: String,
    /// remote.ip() + req.listen_port。
    pub addr: SocketAddr,

    // —— 加密层 ——
    /// 调试 / re-handshake 时校验。
    pub pubkey_b64: String,
    /// ADR-008 MUST-2 — Drop 时自动清零（zeroize crate）。
    /// Clone 操作会拷贝字节；调用方禁止落盘 / 写日志。
    pub aes_key: Zeroizing<[u8; 32]>,

    // —— 隐形掉线检测（peer-heartbeat.md v3 第 4 节 AC #9 #10 #11）——
    /// 仅在 broadcast 200 OK 时更新；不在心跳成功时写（ADR-008 5.2 节语义）。
    pub last_successful_sync_at: Option<Instant>,
    /// 仅调试用。
    pub last_heartbeat_at: Option<Instant>,
    /// 心跳失败累计；FAIL_LIMIT=2 / FORCE_REBUILD=3 阈值判定用。
    pub consecutive_heartbeat_failures: u32,
    /// 广播失败累计；SEND_FAIL_THRESHOLD=2 触发 health 自检。
    pub consecutive_send_failures: u32,

    // —— Trust 视角（PeerRegistry 在 approve/ban 路径维护此字段）——
    /// 与 approved/banned 短路集合保持一致；冗余字段方便单 peer 视角观察。
    pub trust_state: TrustState,

    // —— Replay 防御（ADR-008 4.2 节）——
    /// kind 来自 AadKind；对应字面量见 AadKind::as_bytes()。
    pub last_seen_seq_by_kind: HashMap<AadKind, u64>,
}

// ---------------------------------------------------------------------------
// PeerRegistry struct（ADR-009 第 3.2 节 / 第 3.4 节 选项 A）
// ---------------------------------------------------------------------------

/// 统一 peer 状态库。
///
/// 内部三把锁（按声明顺序 = 锁顺序硬约束）：
///   `inner`    — 主 HashMap，含完整 PeerState（含 aes_key）
///   `approved` — 短路 HashSet；查询 approved 状态免拿 inner 读锁
///   `banned`   — 短路 HashSet；查询 banned 状态免拿 inner 读锁
///   `client_pool` — 内嵌 Arc：保证 remove/ban 内部原子调 client_pool.remove
///
/// 锁顺序硬约束（ADR-009 第 3.3.1 节 P4 补丁）：
/// 任何同时持有 approved + banned 锁的路径必须按 approved 先于 banned 顺序拿。
/// 违反此顺序 → AB-BA 死锁（parking_lot dev profile 死锁检测器会抓出）。
///
/// 锁等待可观测点：
/// tracing target "peer::registry::lock" 记录写锁获取时机；
/// code-reviewer 在 PR 阶段检查"写锁持锁 > 100µs"路径。
pub struct PeerRegistry {
    /// 主状态 HashMap（锁顺序第 1）。
    inner: RwLock<HashMap<String, PeerState>>,
    /// approved 短路缓存（锁顺序第 2）。
    approved: RwLock<HashSet<String>>,
    /// banned 短路缓存（锁顺序第 3）。
    banned: RwLock<HashSet<String>>,
    /// per-peer reqwest::Client 连接池（ADR-009 第 3.5 节 / ADR-008 MUST-4）。
    /// 内嵌 Arc 保证 remove/ban 内部调 client_pool.remove 原子。
    /// client_pool.remove 仅由 PeerRegistry::remove 内部调用（pub(crate) 可见性）。
    client_pool: Arc<ClientPool>,
}

impl PeerRegistry {
    /// 构造空 PeerRegistry。
    ///
    /// 签名（ADR-009 第 3.2 节 / PR-5b 修 ADR-008 MUST-4 契约违反）：
    ///   `new(client_pool: Arc<ClientPool>)`
    /// 由 AppState::new() 调用，构造顺序必须先建 ClientPool 再传入（ADR-009 第 5 节 #5）。
    pub fn new(client_pool: Arc<ClientPool>) -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
            approved: RwLock::new(HashSet::new()),
            banned: RwLock::new(HashSet::new()),
            client_pool,
        }
    }

    /// 测试专用构造：内部新建独立 ClientPool，避免测试代码重复构造。
    #[cfg(test)]
    pub fn new_for_test() -> Self {
        Self::new(Arc::new(ClientPool::new()))
    }

    // -----------------------------------------------------------------------
    // 读方法
    // -----------------------------------------------------------------------

    /// 返回注册的 peer 数量。
    pub fn count(&self) -> usize {
        self.inner.read().len()
    }

    /// 查询 device_id 是否已注册（在 inner 中）。
    pub fn is_known(&self, id: &str) -> bool {
        self.inner.read().contains_key(id)
    }

    /// 查询 device_id 是否在 approved 短路集合。
    pub fn is_approved(&self, id: &str) -> bool {
        self.approved.read().contains(id)
    }

    /// 查询 device_id 是否在 banned 短路集合。
    pub fn is_banned(&self, id: &str) -> bool {
        self.banned.read().contains(id)
    }

    /// 按 device_id 取单个 peer 的状态快照（clone）。
    ///
    /// SECURITY（ADR-009 第 3.2 节 P1 补丁）：
    /// 返回的 PeerState 含 aes_key（Zeroizing clone 拷贝字节）；
    /// 调用方禁止 Debug-print / tracing fields / 落盘 / 跨进程发送。
    pub fn get(&self, id: &str) -> Option<PeerState> {
        self.inner.read().get(id).cloned()
    }

    /// 返回所有已注册 peer 的状态快照列表（每个元素均为 clone）。
    ///
    /// SECURITY（ADR-009 第 3.2 节 P1 补丁）：
    /// 返回的 PeerState 含 aes_key（Zeroizing clone 拷贝字节）；
    /// 调用方禁止 Debug-print / tracing fields / 落盘 / 跨进程发送。
    pub fn snapshot(&self) -> Vec<PeerState> {
        self.inner.read().values().cloned().collect()
    }

    // -----------------------------------------------------------------------
    // 写方法 — 身份生命周期
    // -----------------------------------------------------------------------

    /// 注册 peer（握手成功时调用）。
    ///
    /// 若 id 已存在则覆盖（re-handshake 语义）。
    /// pre: caller 已校验 id 不在 banned 短路集合。
    /// post: inner 含 id；client_pool 由 caller 在本方法返回后同步 insert（PR-3 Lifecycle 落地）。
    ///
    /// 注意：仅写 inner；不自动更新 approved/banned 集合。
    /// caller 需在 insert 后显式调用 approve(id)（握手成功 = Approved 语义）。
    pub fn insert(&self, state: PeerState) {
        let id = state.device_id.clone();
        tracing::trace!(target: "peer::registry::lock", id = %id, "insert: acquiring inner write lock");
        self.inner.write().insert(id, state);
    }

    /// 移除 peer（唯一允许 remove 的入口）。
    ///
    /// 原子顺序（ADR-008 MUST-4 / ADR-009 第 3.5 节调用顺序契约表第 2 行）：
    ///   1. inner.remove(id)     — PeerState drop → Zeroizing 自动清零 aes_key
    ///   2. approved.remove(id)
    ///   3. banned.remove(id)
    ///   4. client_pool.remove(id) — 最后清连接池（invariant 3 闭环）
    ///
    /// 保证 invariant 3：`client_pool.contains(id) == inner.contains_key(id)`
    /// 任何 handler 禁止绕过此方法直接调 client_pool.remove（ADR-009 第 5 节 #5 反模式）。
    ///
    /// caller 须在本方法返回后 emit status-updated（PeerRegistry 不持 Tauri AppHandle）。
    pub fn remove(&self, id: &str) -> Option<PeerState> {
        tracing::trace!(target: "peer::registry::lock", id = %id, "remove: acquiring inner write lock");
        // MUST-4 步骤 1：先从 inner 移除（PeerState drop 时 aes_key 被 Zeroizing 清零）
        let removed = self.inner.write().remove(id);

        // MUST-4 步骤 2：从 approved 移除
        {
            tracing::trace!(target: "peer::registry::lock", id = %id, "remove: acquiring approved write lock");
            self.approved.write().remove(id);
        }

        // MUST-4 步骤 3：从 banned 移除
        {
            tracing::trace!(target: "peer::registry::lock", id = %id, "remove: acquiring banned write lock");
            self.banned.write().remove(id);
        }

        // MUST-4 步骤 4：从 client_pool 移除（ADR-009 第 3.5 节 / ADR-008 MUST-4 原子顺序）
        // 注意：仅在 inner.remove 之后调用，保证锁释放后再操作 client_pool（不持 inner 锁过此调用）
        self.client_pool.remove(id);

        if removed.is_some() {
            tracing::info!(id = %id, "peer removed from registry (inner + pool cleared)");
        }
        removed
    }

    /// 清空所有 peer（lifecycle.shutdown step 4）。
    ///
    /// 注意：清空后 approved/banned 短路集合同步清空。
    /// approved/banned 不持久化（ADR-009 第 3.3 节状态表"用户 quit_app"行）。
    pub fn clear(&self) {
        tracing::trace!(target: "peer::registry::lock", "clear: acquiring all write locks");
        // 按锁顺序：inner > approved > banned
        self.inner.write().clear();
        self.approved.write().clear();
        self.banned.write().clear();
        tracing::info!("peer registry cleared");
    }

    // -----------------------------------------------------------------------
    // 写方法 — trust 互斥（ADR-009 第 3.3 节 / 第 3.3.1 节 P4 补丁）
    // -----------------------------------------------------------------------

    /// 将 peer 标记为 Approved（trust 覆盖 ban）。
    ///
    /// 锁顺序（ADR-009 第 3.3.1 节 P4 补丁）：
    ///   approved 先于 banned（与字段声明顺序一致）。
    ///
    /// invariant post：
    ///   approved.contains(id) && !banned.contains(id)
    ///   若 inner 中有此 id，inner[id].trust_state == Approved
    pub fn approve(&self, id: &str) {
        // 锁顺序：approved 先（声明第 2）→ banned 后（声明第 3）
        // 不持有 inner 写锁期间操作 approved/banned，避免与 snapshot 读锁竞争
        {
            tracing::trace!(target: "peer::registry::lock", id = %id, "approve: acquiring approved+banned write locks");
            let mut a = self.approved.write();
            let mut b = self.banned.write();
            a.insert(id.into());
            b.remove(id);
        }
        // 同步更新 inner[id].trust_state
        if let Some(state) = self.inner.write().get_mut(id) {
            state.trust_state = TrustState::Approved;
        }
        tracing::info!(id = %id, "peer approved");
    }

    /// 将 peer 标记为 Banned（ban 覆盖 trust）。
    ///
    /// 锁顺序（ADR-009 第 3.3.1 节 P4 补丁）：
    ///   **approved 先于 banned**（与字段声明顺序一致）。
    ///   注意：书写顺序虽与 ADR-009 第 3.2 节伪代码 "banned.insert + approved.remove"
    ///   字面相反，但**锁的取得顺序必须遵循声明序（approved → banned）**，
    ///   否则与 approve 形成 AB-BA 死锁（dev profile parking_lot 死锁检测器可抓）。
    ///
    /// 若 peer 在 inner 中（was_peer = true）：
    ///   同时从 inner 移除（踢出已连接 peer）；aes_key 被 Zeroizing 自动清零。
    ///   caller 须在本方法返回后 emit status-updated 并调 client_pool.remove(id)（PR-3 落地）。
    ///
    /// invariant post：
    ///   !approved.contains(id) && banned.contains(id)
    ///   inner 不含 id
    pub fn ban(&self, id: &str) {
        // 锁顺序：approved 先（声明第 2）→ banned 后（声明第 3）
        {
            tracing::trace!(target: "peer::registry::lock", id = %id, "ban: acquiring approved+banned write locks");
            let mut a = self.approved.write();
            let mut b = self.banned.write();
            // 注意：虽然语义是 "banned.insert + approved.remove"，
            // 但锁顺序固定为 approved 先拿，故先操作 approved 集合（a.remove），
            // 再操作 banned 集合（b.insert）
            a.remove(id);
            b.insert(id.into());
        }

        // 若 was_peer：从 inner 移除（MUST-4 原子顺序 + ADR-009 第 3.3 节状态机表）
        let removed = self.inner.write().remove(id);
        if removed.is_some() {
            // aes_key 已随 PeerState drop 被 Zeroizing 清零。
            // 同时清 client_pool（ADR-008 MUST-4 / ADR-009 第 3.5 节 invariant 3）。
            // 顺序：inner.remove（已完成）→ client_pool.remove（现在执行）
            self.client_pool.remove(id);
            tracing::info!(id = %id, "peer banned and removed from inner + pool (was_peer=true)");
        } else {
            tracing::info!(id = %id, "peer banned (was_peer=false; not in inner)");
        }
    }

    // -----------------------------------------------------------------------
    // 写方法 — 可观测计数（peer-heartbeat.md v3 第 4 节）
    // -----------------------------------------------------------------------

    /// 记录心跳成功：consecutive_heartbeat_failures 归零 + last_heartbeat_at 更新。
    ///
    /// 注意：**不**更新 last_successful_sync_at（ADR-008 5.2 节语义：
    /// last_successful_sync_at 仅在 broadcast 200 OK 时写）。
    pub fn record_heartbeat_ok(&self, id: &str) {
        if let Some(state) = self.inner.write().get_mut(id) {
            state.consecutive_heartbeat_failures = 0;
            state.last_heartbeat_at = Some(Instant::now());
        }
    }

    /// 记录心跳失败：consecutive_heartbeat_failures 递增并返回当前值。
    ///
    /// 调用方用返回值判断是否达到 FAIL_LIMIT / FORCE_REBUILD 阈值。
    pub fn record_heartbeat_fail(&self, id: &str) -> u32 {
        if let Some(state) = self.inner.write().get_mut(id) {
            state.consecutive_heartbeat_failures =
                state.consecutive_heartbeat_failures.saturating_add(1);
            state.consecutive_heartbeat_failures
        } else {
            0
        }
    }

    /// 记录广播成功：consecutive_send_failures 归零 + last_successful_sync_at 更新。
    ///
    /// last_successful_sync_at 仅此处更新（ADR-008 5.2 节语义）。
    pub fn record_send_ok(&self, id: &str) {
        if let Some(state) = self.inner.write().get_mut(id) {
            state.consecutive_send_failures = 0;
            state.last_successful_sync_at = Some(Instant::now());
        }
    }

    /// 记录广播失败：consecutive_send_failures 递增并返回当前值。
    ///
    /// 调用方用返回值判断是否达到 SEND_FAIL_THRESHOLD 阈值。
    pub fn record_send_fail(&self, id: &str) -> u32 {
        if let Some(state) = self.inner.write().get_mut(id) {
            state.consecutive_send_failures = state.consecutive_send_failures.saturating_add(1);
            state.consecutive_send_failures
        } else {
            0
        }
    }

    // -----------------------------------------------------------------------
    // 写方法 — heartbeat worker 专用（PR-6b peer-heartbeat.md 第 4 节 AC）
    // -----------------------------------------------------------------------

    /// 心跳成功：清零 consecutive_heartbeat_failures + 更新 last_heartbeat_at。
    ///
    /// ADR-008 5.2 节硬约束：**不**更新 last_successful_sync_at。
    /// last_successful_sync_at 仅在 broadcast 200 OK 时由 record_send_ok 更新。
    pub fn update_heartbeat_success(&self, id: &str) {
        self.record_heartbeat_ok(id);
    }

    /// 递增 consecutive_heartbeat_failures 并返回新计数。
    ///
    /// 调用方用返回值与 FORCE_REBUILD_LIMIT 比较（PR-6b heartbeat_worker.rs）。
    pub fn increment_heartbeat_failure(&self, id: &str) -> u32 {
        self.record_heartbeat_fail(id)
    }

    /// 读取当前 consecutive_heartbeat_failures（只读，不修改）。
    pub fn get_consecutive_heartbeat_failures(&self, id: &str) -> u32 {
        self.inner
            .read()
            .get(id)
            .map(|s| s.consecutive_heartbeat_failures)
            .unwrap_or(0)
    }

    /// 强制重连成功后归零 consecutive_heartbeat_failures。
    pub fn reset_heartbeat_failures(&self, id: &str) {
        if let Some(state) = self.inner.write().get_mut(id) {
            state.consecutive_heartbeat_failures = 0;
        }
    }

    /// 广播 200 OK 时更新 last_successful_sync_at（仅此入口写）。
    ///
    /// ADR-008 5.2 节：last_successful_sync_at 仅在广播 200 OK 确认时写，
    /// 不在心跳成功时写。调用方：network/client.rs::broadcast_clipboard 200 OK 路径。
    pub fn update_last_successful_sync_at(&self, id: &str) {
        self.record_send_ok(id);
    }

    /// 强制重连后更新 aes_key（re-handshake 路径）。
    ///
    /// SECURITY（ADR-008 MUST-2）：参数 key 为 Zeroizing 包装，
    /// 赋值后旧 key 字节随旧 Zeroizing drop 自动清零。
    pub fn update_aes_key(&self, id: &str, key: zeroize::Zeroizing<[u8; 32]>) {
        if let Some(state) = self.inner.write().get_mut(id) {
            // 旧 aes_key（Zeroizing）在此赋值时 drop → 自动清零（ADR-008 MUST-2）
            state.aes_key = key;
        }
    }

    // -----------------------------------------------------------------------
    // 写方法 — seq dedupe（ADR-008 4.2 节 replay 防御）
    // -----------------------------------------------------------------------

    /// 检查并更新 (id, kind, seq) 的去重状态。
    ///
    /// 返回值语义：
    /// - `true`  = 本次 seq 是新的，caller 可继续处理
    /// - `false` = 本次 seq 已见过（重复），caller 应 200 OK 静默丢弃
    ///
    /// 必须在 broadcast handler 入口第一行调用（早于解密 / sanitize / 业务逻辑）。
    /// 保证 replay 短路（ADR-009 第 3.2 节 invariant 5）。
    pub fn seen_seq_and_update(&self, id: &str, kind: AadKind, seq: u64) -> bool {
        if let Some(state) = self.inner.write().get_mut(id) {
            match state.last_seen_seq_by_kind.get(&kind) {
                Some(&last) if seq <= last => {
                    // seq 不大于已见最大值 → 重复或乱序，丢弃
                    false
                }
                _ => {
                    // 新 seq（包括首次出现）→ 更新并放行
                    state.last_seen_seq_by_kind.insert(kind, seq);
                    true
                }
            }
        } else {
            // 未知 peer → 视为重复（安全侧）
            false
        }
    }
}

/// PeerRegistry::default — 仅测试用（构造孤立 ClientPool，与生产 AppState.client_pool 不共享）。
///
/// [低 nit #3 PR-5b review] 若在生产路径调用，违反 invariant 3（client_pool 不共享）。
/// PR-6：加 #[cfg(test)] 限定，防止未来在生产路径意外调用。
#[cfg(test)]
impl Default for PeerRegistry {
    fn default() -> Self {
        Self::new(Arc::new(ClientPool::new()))
    }
}

// ---------------------------------------------------------------------------
// AadKind Hash + Eq 实现（用于 HashMap<AadKind, u64>）
// ---------------------------------------------------------------------------
//
// AadKind 已在 crypto/mod.rs 派生 PartialEq + Eq + Copy + Clone；
// 但未派生 Hash——需要在本模块手动实现（或在 crypto/mod.rs 补派生）。
// 为不动 PR-1 已落的 crypto 模块，在此处实现 Hash（仅对 AadKind 本身）。

impl std::hash::Hash for AadKind {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // 用 as_bytes() 的稳定字节表示作为 hash 输入
        self.as_bytes().hash(state);
    }
}

// ---------------------------------------------------------------------------
// 单元测试（ADR-009 第 6.1 节单测清单 — PR-5b 修 MUST-4 + 补新单测）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    /// 构造最小化 PeerState 用于测试。
    fn make_peer(id: &str) -> PeerState {
        PeerState {
            device_id: id.to_string(),
            device_name: format!("device-{id}"),
            addr: "127.0.0.1:9999"
                .parse::<SocketAddr>()
                .expect("test addr parse failed"),
            pubkey_b64: "test_pubkey_b64".to_string(),
            aes_key: Zeroizing::new([0u8; 32]),
            last_successful_sync_at: None,
            last_heartbeat_at: None,
            consecutive_heartbeat_failures: 0,
            consecutive_send_failures: 0,
            trust_state: TrustState::Pending,
            last_seen_seq_by_kind: HashMap::new(),
        }
    }

    // 单测 1（ADR-009 第 6.1 节 #1）
    /// insert → get 返同字段 → remove 返 Some → get 返 None
    #[test]
    fn insert_get_remove_basic() {
        let registry = PeerRegistry::new_for_test();
        let state = make_peer("peer-001");

        registry.insert(state.clone());

        // get 应返回同一 device_id
        let got = registry
            .get("peer-001")
            .expect("inserted peer should be found");
        assert_eq!(got.device_id, "peer-001");
        assert_eq!(got.device_name, "device-peer-001");

        // count 应为 1
        assert_eq!(registry.count(), 1);

        // remove 应返回 Some
        let removed = registry.remove("peer-001");
        assert!(removed.is_some(), "remove should return the PeerState");

        // get 应返回 None
        assert!(
            registry.get("peer-001").is_none(),
            "peer should be absent after remove"
        );
        assert_eq!(registry.count(), 0);
    }

    // 单测 2（ADR-009 第 6.1 节 — trust 互斥语义）
    /// approve → is_approved ✓ + is_banned ✗；ban → is_approved ✗ + is_banned ✓
    #[test]
    fn trust_mutual_exclusion() {
        let registry = PeerRegistry::new_for_test();
        let state = make_peer("peer-002");
        registry.insert(state);

        // approve
        registry.approve("peer-002");
        assert!(
            registry.is_approved("peer-002"),
            "approved set should contain peer"
        );
        assert!(
            !registry.is_banned("peer-002"),
            "banned set must not contain approved peer"
        );

        // ban（覆盖 approve）
        registry.ban("peer-002");
        assert!(
            !registry.is_approved("peer-002"),
            "approved set must not contain banned peer"
        );
        assert!(
            registry.is_banned("peer-002"),
            "banned set should contain peer"
        );
    }

    // 单测 3（ADR-009 第 6.1 节 #6 + #7 — 互斥 transition 原子性）
    /// approve → ban 过渡时 approved 集合不再有，banned 集合有（原子）
    #[test]
    fn trust_transition_atomicity() {
        let registry = PeerRegistry::new_for_test();
        let state = make_peer("peer-003");
        registry.insert(state);

        // 先 approve
        registry.approve("peer-003");
        assert!(registry.is_approved("peer-003"));
        assert!(!registry.is_banned("peer-003"));

        // 再 ban（trust → ban 过渡）
        registry.ban("peer-003");

        // 原子性：approved 集合不再有，banned 集合有
        assert!(
            !registry.is_approved("peer-003"),
            "after ban transition: approved must not contain peer"
        );
        assert!(
            registry.is_banned("peer-003"),
            "after ban transition: banned must contain peer"
        );

        // 反向：ban → approve（ban 覆盖 trust 的逆向）
        let registry2 = PeerRegistry::new_for_test();
        let state2 = make_peer("peer-003b");
        registry2.insert(state2);
        registry2.ban("peer-003b");
        registry2.approve("peer-003b"); // trust 覆盖 ban
        assert!(
            registry2.is_approved("peer-003b"),
            "trust must override ban"
        );
        assert!(
            !registry2.is_banned("peer-003b"),
            "banned must be cleared after approve"
        );
    }

    // 单测 4（ADR-009 第 6.1 节 #2 — MUST-4 原子顺序）
    /// insert + approve → remove → inner / approved / banned 三集合同时不含 id
    #[test]
    fn remove_atomic_order() {
        let registry = PeerRegistry::new_for_test();
        let state = make_peer("peer-004");
        registry.insert(state);
        registry.approve("peer-004");

        // 前置验证
        assert!(
            registry.is_known("peer-004"),
            "before remove: inner must contain peer"
        );
        assert!(
            registry.is_approved("peer-004"),
            "before remove: approved must contain peer"
        );
        assert!(
            !registry.is_banned("peer-004"),
            "before remove: banned must not contain peer"
        );

        // remove
        let removed = registry.remove("peer-004");
        assert!(removed.is_some(), "remove must return Some");

        // MUST-4 原子性：三集合同时清除
        assert!(
            !registry.is_known("peer-004"),
            "inner must not contain peer after remove"
        );
        assert!(
            !registry.is_approved("peer-004"),
            "approved must not contain peer after remove"
        );
        assert!(
            !registry.is_banned("peer-004"),
            "banned must not contain peer after remove"
        );
    }

    // 单测 5（ADR-009 第 6.1 节 #8 — seq dedupe）
    /// 同 (id, kind) 第二次 seq <= 第一次 → false；seq > 第一次 → true
    #[test]
    fn seen_seq_and_update_dedupe() {
        let registry = PeerRegistry::new_for_test();
        let state = make_peer("peer-005");
        registry.insert(state);

        // 首次 seq=5 → true（新）
        let first = registry.seen_seq_and_update("peer-005", AadKind::Text, 5);
        assert!(first, "first seq should be new");

        // 相同 seq=5 → false（重复）
        let dup = registry.seen_seq_and_update("peer-005", AadKind::Text, 5);
        assert!(!dup, "same seq must be rejected as duplicate");

        // 旧 seq=3 → false（乱序，小于已见）
        let old = registry.seen_seq_and_update("peer-005", AadKind::Text, 3);
        assert!(!old, "older seq must be rejected");

        // 新 seq=6 → true（递增）
        let newer = registry.seen_seq_and_update("peer-005", AadKind::Text, 6);
        assert!(newer, "newer seq should be accepted");

        // 不同 kind 的 seq 独立计数
        let diff_kind = registry.seen_seq_and_update("peer-005", AadKind::File, 1);
        assert!(diff_kind, "different kind first seq should be new");

        // 未知 peer → false（安全侧）
        let unknown = registry.seen_seq_and_update("nonexistent", AadKind::Text, 1);
        assert!(!unknown, "unknown peer seq should return false");
    }

    // 单测 6（ADR-009 第 6.1 节 — snapshot Zeroizing clone 独立）
    /// snapshot 返 Vec<PeerState>，每个含独立 Zeroizing<aes_key>（不与 inner 共享内存）
    #[test]
    fn snapshot_zerocopy_clone() {
        let registry = PeerRegistry::new_for_test();

        // 插入 2 个不同 aes_key 的 peer
        let mut peer_a = make_peer("peer-006a");
        peer_a.aes_key = Zeroizing::new([0xAAu8; 32]);
        let mut peer_b = make_peer("peer-006b");
        peer_b.aes_key = Zeroizing::new([0xBBu8; 32]);

        registry.insert(peer_a);
        registry.insert(peer_b);

        let snap = registry.snapshot();
        assert_eq!(snap.len(), 2, "snapshot should contain 2 peers");

        // 验证 clone 独立：修改 snap 中的字节不影响 inner
        let ids: Vec<_> = snap.iter().map(|p| &p.device_id).collect();
        assert!(ids.contains(&&"peer-006a".to_string()));
        assert!(ids.contains(&&"peer-006b".to_string()));

        // 验证 aes_key 值正确 clone（bytes 相同，但是独立副本）
        for p in &snap {
            if p.device_id == "peer-006a" {
                assert_eq!(*p.aes_key, [0xAAu8; 32], "peer-006a aes_key should be 0xAA");
            } else if p.device_id == "peer-006b" {
                assert_eq!(*p.aes_key, [0xBBu8; 32], "peer-006b aes_key should be 0xBB");
            }
        }

        // 确认 inner 仍完好（snapshot 不移动数据）
        assert_eq!(registry.count(), 2);
    }

    // 单测 7（与 rate_limit 模块协同 — per_pair 和 global 计数）
    // 实际测试在 rate_limit.rs 内；此处验证跨模块可访问性
    #[test]
    fn rate_limit_module_accessible() {
        use super::rate_limit::{RateLimitDecision, RateLimiter};
        let rl = RateLimiter::new();
        let ip: std::net::IpAddr = "127.0.0.1".parse().expect("ip parse");
        // 第一次应 Allowed
        let decision = rl.check_handshake(ip, "device-test");
        matches!(decision, RateLimitDecision::Allowed);
    }

    // 额外：验证 record_heartbeat_ok 不更新 last_successful_sync_at（ADR-008 5.2 节）
    #[test]
    fn record_heartbeat_ok_does_not_update_last_sync() {
        let registry = PeerRegistry::new_for_test();
        let state = make_peer("peer-007");
        registry.insert(state);

        registry.record_heartbeat_ok("peer-007");

        let got = registry.get("peer-007").expect("peer must exist");
        assert!(
            got.last_successful_sync_at.is_none(),
            "heartbeat ok must NOT update last_successful_sync_at (ADR-008 5.2 sec)"
        );
        assert!(
            got.last_heartbeat_at.is_some(),
            "heartbeat ok must update last_heartbeat_at"
        );
    }

    // 额外：验证 record_send_ok 更新 last_successful_sync_at（ADR-008 5.2 节）
    #[test]
    fn record_send_ok_updates_last_sync() {
        let registry = PeerRegistry::new_for_test();
        let state = make_peer("peer-008");
        registry.insert(state);

        registry.record_send_ok("peer-008");

        let got = registry.get("peer-008").expect("peer must exist");
        assert!(
            got.last_successful_sync_at.is_some(),
            "send ok must update last_successful_sync_at"
        );
    }

    // 额外：ban 一个不在 inner 的 peer（was_peer=false）不影响 inner
    #[test]
    fn ban_unknown_peer_does_not_affect_inner() {
        let registry = PeerRegistry::new_for_test();
        let state = make_peer("peer-existing");
        registry.insert(state);

        // ban 一个不存在的 peer
        registry.ban("nonexistent-peer");

        // inner 中已有的 peer 不受影响
        assert!(
            registry.is_known("peer-existing"),
            "existing peer must still be in inner"
        );
        // nonexistent 进入 banned
        assert!(
            registry.is_banned("nonexistent-peer"),
            "unknown peer should be in banned"
        );
    }

    // 单测 13（ADR-009 第 6.1 节 #13 — 锁顺序死锁活性证明）
    //
    // 验证 approve / ban / get 在多线程并发下不死锁（锁顺序硬约束 ADR-009 第 3.3.1 节）。
    // 方法：8 个线程各跑 50 次混合操作（approve / ban / get）作用于同一组 device_id；
    //       若锁顺序存在 AB-BA 反转，线程将死锁，join 无法完成（CI 超时触发失败）。
    //
    // 注意：
    // - 不使用 parking_lot/deadlock_detection feature（Cargo.toml 未启用）；
    //   活性证明依赖"8 个线程全部 join 完成"这一事实——死锁发生时 join 卡住。
    // - 用 get 替代 remove：避免第 n 次 approve/ban 作用于已被移除的 peer 产生
    //   无意义的"未知 peer"路径（影响测试可读性，不影响锁顺序验证目的）。
    //
    // see: decisions/ADR-009-peer-registry.md 第 3.3.1 节 / 第 6.1 节 #13
    #[test]
    fn lock_order_no_deadlock() {
        use std::sync::Arc;
        use std::thread;

        let registry = Arc::new(PeerRegistry::new_for_test());

        // 预先插入 5 个 peer，approve 状态
        for i in 0..5 {
            let id = format!("device-{i}");
            registry.insert(make_peer(&id));
            registry.approve(&id);
        }

        let device_ids: Vec<String> = (0..5).map(|i| format!("device-{i}")).collect();

        let mut handles = vec![];
        for thread_idx in 0..8usize {
            let registry = Arc::clone(&registry);
            let ids = device_ids.clone();
            handles.push(thread::spawn(move || {
                for iter in 0..50usize {
                    let id = &ids[(thread_idx + iter) % ids.len()];
                    // 三种操作交替，覆盖 approve（双写锁）、ban（双写锁，反向语义）、
                    // get（inner 读锁）三条锁路径；关键是 approve 与 ban 并发时
                    // 两者都按 approved → banned 顺序拿锁，不会 AB-BA 反转。
                    match (thread_idx + iter) % 3 {
                        0 => {
                            registry.approve(id);
                        }
                        1 => {
                            registry.ban(id);
                            // ban 后重新 insert + approve，保证后续 approve/ban 操作有 peer 可用
                            registry.insert(make_peer(id));
                            registry.approve(id);
                        }
                        _ => {
                            // get 走 inner 读锁，与 approve/ban 写锁并发
                            let _ = registry.get(id);
                        }
                    }
                }
            }));
        }

        // 若发生死锁，此处 join 不返回，CI 超时触发失败
        for h in handles {
            h.join().expect("thread should join without deadlock");
        }

        // 走到这里说明 8 个线程全部无死锁完成
        assert_eq!(8, 8, "all 8 threads completed without deadlock");
    }

    // 新单测（PR-5b #3）— ADR-008 MUST-4 + ADR-009 第 3.5 节 invariant 3
    //
    // 验证：insert peer + client_pool.replace（模拟握手路径写入 pool）
    //       → registry.remove(id)
    //       → 断言 client_pool.get(id) == None（invariant 3 闭环）
    //
    // see: specs/clipboard-text-sync.md 第 8.2 节 [严重 #1] / PR-5b 修复
    // see: decisions/ADR-009-peer-registry.md 第 3.5 节 / 第 6.1 节 #2
    #[test]
    fn remove_clears_client_pool_atomic() {
        use crate::app::client_pool::ClientPool;

        // 构造共享 client_pool，与 registry 共享 Arc（模拟 AppState 构造路径）
        let pool = Arc::new(ClientPool::new());
        let registry = PeerRegistry::new(Arc::clone(&pool));

        let id = "peer-pool-test";

        // 模拟握手成功路径：先写 pool（client_pool.insert），再写 registry（registry.insert）
        // ADR-009 第 3.5 节调用顺序契约表第 1 行
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("test client build should not fail");
        pool.insert(id, client);
        registry.insert(make_peer(id));
        registry.approve(id);

        // 前置断言：两者都含 id
        assert!(
            registry.is_known(id),
            "before remove: registry must contain peer"
        );
        assert!(
            pool.get(id).is_some(),
            "before remove: client_pool must contain client"
        );

        // 调 registry.remove（唯一合法入口，ADR-009 第 5 节 #5）
        let removed = registry.remove(id);
        assert!(removed.is_some(), "remove must return Some PeerState");

        // invariant 3 验证：registry 和 pool 同时不含 id
        assert!(
            !registry.is_known(id),
            "after remove: registry must NOT contain peer"
        );
        assert!(
            pool.get(id).is_none(),
            "after remove: client_pool must NOT contain client (invariant 3 MUST-4)"
        );
    }

    // 新单测（PR-5b #4）— ban was_peer=true 也清 client_pool（ADR-009 第 3.3 节状态机）
    //
    // 验证：insert peer + client 写 pool → registry.ban(id)（was_peer=true）
    //       → 断言 client_pool.get(id) == None
    //
    // see: decisions/ADR-009-peer-registry.md 第 3.3 节事件表行 "/peers/ban 收到，subject 在 inner"
    #[test]
    fn ban_clears_client_pool_when_was_peer() {
        use crate::app::client_pool::ClientPool;

        let pool = Arc::new(ClientPool::new());
        let registry = PeerRegistry::new(Arc::clone(&pool));

        let id = "peer-ban-pool-test";

        // 写入 pool + registry
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("test client build should not fail");
        pool.insert(id, client);
        registry.insert(make_peer(id));
        registry.approve(id);

        // 前置断言
        assert!(registry.is_known(id));
        assert!(pool.get(id).is_some());

        // ban（was_peer=true 路径）
        registry.ban(id);

        // invariant 3 + ban 语义：inner 不含 + pool 不含 + banned 集合含
        assert!(
            !registry.is_known(id),
            "after ban (was_peer): inner must NOT contain peer"
        );
        assert!(
            pool.get(id).is_none(),
            "after ban (was_peer): client_pool must NOT contain client (invariant 3 MUST-4)"
        );
        assert!(
            registry.is_banned(id),
            "after ban: banned set must contain peer"
        );
    }
}
