//! ClientPool — per-peer reqwest::Client 连接池
//! see decisions/ADR-009-peer-registry.md (第 3.5 节 client_pool 接口契约)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-4 原子顺序)
//! see decisions/ADR-010-lifecycle.md (第 3 节 PR-3 范围)
//!
//! 设计要点（ADR-009 第 3.5 节）：
//! - per-peer Client：每个 peer 一个独立 reqwest::Client（连接池隔离）
//! - 禁止 lazy add：get miss 不创建新 Client（ADR-009 第 7.3 节 P2 反模式）
//! - replace()：强制重连（health worker 触发），drop 旧 Client 让连接池一并 drop
//! - remove()：pub(crate) — 仅由 PeerRegistry::remove 内部调用（原子顺序 MUST-4）
//! - Shutting 阶段禁止 replace（ADR-010 第 3.6 节 / ADR-009 第 7.3 节 P4 补丁）
//!
//! 不持锁过 await（ADR-010 编码风格）：
//!   调用方应 `.get(id).clone()` 拿 Client 后立即释放写锁，再发 reqwest 请求。

use parking_lot::RwLock;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// ClientPool struct（ADR-009 第 3.5 节接口契约）
// ---------------------------------------------------------------------------

/// per-peer reqwest::Client 连接池。
///
/// 锁粒度：单 `parking_lot::RwLock<HashMap<String, reqwest::Client>>`。
/// 写锁临界区内只做 insert/remove/replace — **不做任何 I/O**。
///
/// ADR-009 第 3.5 节调用顺序契约：
/// - insert 仅在握手成功路径（handshake.rs）调用（禁止 lazy add）
/// - remove 仅由 `PeerRegistry::remove` 内部调用（保原子顺序）
/// - replace 仅由 `network/health.rs` 在 FORCE_REBUILD_LIMIT=3 时调用
///   caller 须在调用前验证 `registry.is_known(id) && !registry.is_banned(id)`
pub struct ClientPool {
    inner: RwLock<HashMap<String, reqwest::Client>>,
}

impl ClientPool {
    /// 构造空 ClientPool。
    /// 由 lifecycle.start step 3 调用（ADR-010 第 3.2 节启动顺序）。
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    /// 握手成功时注册新 Client（仅此入口允许插入）。
    ///
    /// 若 id 已存在则覆盖（re-handshake 语义）；旧 Client drop 时连接池自动关闭。
    /// pre: caller（handshake.rs）已完成 PeerState 构造 + registry.insert。
    ///
    /// ADR-009 第 3.5 节调用顺序：
    ///   1. 构造 reqwest::Client
    ///   2. client_pool.insert(id, client)
    ///   3. registry.insert(state)
    pub fn insert(&self, id: &str, client: reqwest::Client) {
        tracing::debug!(target: "app::client_pool", id = %id, "insert client");
        self.inner.write().insert(id.to_string(), client);
    }

    /// 强制重建 Client（health worker FORCE_REBUILD_LIMIT=3 时调用）。
    ///
    /// drop 旧 Client → 旧连接池全部关闭 → 新 Client 插入（重新 TCP 握手）。
    ///
    /// ADR-010 第 3.6 节 P4 补丁 / ADR-009 第 7.3 节 P2 反模式：
    /// caller（health.rs）**必须**在调用前检查 `lifecycle.phase() != Phase::Shutting`；
    /// Shutting 阶段禁止 replace（白浪费 1 次 TCP + 与 step 6 clear 抢占）。
    ///
    /// `.no_proxy()`：按 lessons-learned 第 4.1 节禁用系统代理，避免 LAN 流量意外出网。
    pub fn replace(&self, id: &str) {
        // 构造新 Client（不持写锁 — 避免构造期阻塞读路径）
        let new_client = reqwest::Client::builder()
            .no_proxy()
            .build()
            // reqwest::ClientBuilder::build 只在 TLS 配置错误时失败；
            // 这里无自定义 TLS，不会失败；初始化期用 expect 是合法的（ADR-010 注释）
            .expect("ClientPool::replace: reqwest::Client::builder().no_proxy().build() should not fail");

        tracing::debug!(target: "app::client_pool", id = %id, "replacing client (force-rebuild)");
        // 写锁只做 HashMap 替换（drop 旧 Client），不做 I/O
        let mut guard = self.inner.write();
        guard.insert(id.to_string(), new_client);
        // 旧 Client 随 HashMap 的原有值在此 drop — 连接池随之关闭
    }

    /// 从池中移除 peer 的 Client（唯一允许 remove 的入口，仅 PeerRegistry 内部调用）。
    ///
    /// ADR-009 第 3.5 节 / ADR-008 MUST-4：
    /// `PeerRegistry::remove` 内按顺序：1. inner.remove → 2. client_pool.remove。
    /// 任何 handler 不得绕过 PeerRegistry 直接调此方法。
    ///
    /// PR-3 注：PeerRegistry 尚未集成 client_pool（PR-4 落地）；
    /// `#[allow(dead_code)]` 仅作为 PR-3 的临时标注，PR-4 后移除。
    #[allow(dead_code)]
    pub(crate) fn remove(&self, id: &str) -> Option<reqwest::Client> {
        tracing::debug!(target: "app::client_pool", id = %id, "remove client");
        self.inner.write().remove(id)
    }

    /// 取已存在的 Client（只读快照）。
    ///
    /// **不**在 miss 时创建新 Client（禁止 lazy add — ADR-009 第 7.3 节 P2 反模式）。
    /// miss 时返回 None；caller 需自行决定是否 log warn + 跳过该 peer。
    ///
    /// 使用方式：`pool.get(id)` 返回 Option<reqwest::Client>；
    /// reqwest::Client 内部是 Arc，clone 是廉价的引用计数增加。
    pub fn get(&self, id: &str) -> Option<reqwest::Client> {
        // 读锁持锁期间只做 HashMap 查找 + clone（不做任何 I/O）
        self.inner.read().get(id).cloned()
    }

    /// 返回池中 Client 数量（测试 / 诊断用）。
    pub fn len(&self) -> usize {
        self.inner.read().len()
    }

    /// 判断池是否为空。
    pub fn is_empty(&self) -> bool {
        self.inner.read().is_empty()
    }
}

impl Default for ClientPool {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// 单元测试（ADR-010 第 6 节最小集 — PR-3 client_pool 部分）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 辅助：构造一个 no_proxy reqwest::Client
    fn make_client() -> reqwest::Client {
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("test client build should not fail")
    }

    // 单测 4（ADR-010 第 6 节 — replace_drops_old_client）
    /// replace 后 get 返新 Client（旧 Client 已 drop）。
    /// 通过 len() 稳定为 1 验证没有重复插入。
    #[test]
    fn replace_drops_old_client() {
        let pool = ClientPool::new();

        // 先 insert 一个 Client
        pool.insert("peer-alpha", make_client());
        assert_eq!(pool.len(), 1, "pool should have 1 client after insert");

        // replace：应覆盖旧 Client
        pool.replace("peer-alpha");
        assert_eq!(
            pool.len(),
            1,
            "pool should still have 1 client after replace (no dup)"
        );

        // get 应返回 Some（新 Client 已在池中）
        let got = pool.get("peer-alpha");
        assert!(got.is_some(), "get should return Some after replace");
    }

    // 单测 5（ADR-010 第 6 节 — get_does_not_lazy_add）
    /// get 对不存在的 id 返 None，且不创建新 Client（禁止 lazy add）。
    /// 连续两次 get 均返 None（不因第一次 miss 而产生副作用）。
    #[test]
    fn get_does_not_lazy_add() {
        let pool = ClientPool::new();

        // 对不存在的 id，get 返 None
        let first = pool.get("nonexistent-peer");
        assert!(
            first.is_none(),
            "get on nonexistent id must return None (no lazy add)"
        );

        // 再次 get：仍 None（miss 没有副作用）
        let second = pool.get("nonexistent-peer");
        assert!(
            second.is_none(),
            "second get on same nonexistent id must still return None"
        );

        // 池大小仍为 0（没有 lazy add）
        assert_eq!(
            pool.len(),
            0,
            "pool size must remain 0 after get misses (no lazy add)"
        );
    }

    // 单测 6（ADR-010 第 6 节 — remove_then_get_returns_none）
    /// insert → remove → get 返 None。
    #[test]
    fn remove_then_get_returns_none() {
        let pool = ClientPool::new();

        // insert
        pool.insert("peer-beta", make_client());
        assert!(
            pool.get("peer-beta").is_some(),
            "should be present after insert"
        );

        // remove
        let removed = pool.remove("peer-beta");
        assert!(
            removed.is_some(),
            "remove should return Some for existing client"
        );

        // get 返 None
        assert!(
            pool.get("peer-beta").is_none(),
            "get must return None after remove"
        );
        assert_eq!(pool.len(), 0, "pool must be empty after remove");
    }

    // 额外测试：insert 覆盖（re-handshake 语义）
    #[test]
    fn insert_overwrites_existing() {
        let pool = ClientPool::new();
        pool.insert("peer-gamma", make_client());
        pool.insert("peer-gamma", make_client()); // re-handshake
        assert_eq!(
            pool.len(),
            1,
            "re-insert should overwrite, not add duplicate"
        );
        assert!(pool.get("peer-gamma").is_some());
    }

    // 额外测试：remove 不存在的 id 返 None（幂等）
    #[test]
    fn remove_nonexistent_returns_none() {
        let pool = ClientPool::new();
        let result = pool.remove("ghost-peer");
        assert!(
            result.is_none(),
            "remove on nonexistent id must return None"
        );
        assert_eq!(pool.len(), 0);
    }
}
