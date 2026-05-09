//! AppState — 应用全局状态聚合
//! see decisions/ADR-010-lifecycle.md (第 3.2 节 step 3 顺序)
//! see decisions/ADR-009-peer-registry.md (第 3.5 节 / 第 5 节 #5 构造顺序)
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.5 节 AppState struct)
//!
//! 构造顺序（ADR-009 第 5 节 #5 + ADR-010 第 3.2 节 step 3）：
//!   Arc<ClientPool>::new()
//!   → Arc<PeerRegistry>::new()  [PR-4 落地时传入 client_pool]
//!   → Arc<RateLimiter>::new()
//!   → Arc<Lifecycle>::new()
//!
//! PR-3 注意：PeerRegistry::new() 当前不接受 client_pool 参数（PR-2 已落接口）。
//! PR-4 Lifecycle 落地时在 PeerRegistry::remove / ban 内补充 client_pool.remove 原子顺序。

use std::sync::Arc;

use crate::app::client_pool::ClientPool;
use crate::app::lifecycle::Lifecycle;
use crate::peer::rate_limit::RateLimiter;
use crate::peer::PeerRegistry;

// ---------------------------------------------------------------------------
// AppState struct（ADR-010 第 3.2 节 step 3）
// ---------------------------------------------------------------------------

/// 应用全局状态。作为 Tauri managed state 注入（`.manage(app_state)`）。
///
/// 所有字段为 Arc — 可跨线程共享（Send + Sync）。
/// Tauri command 通过 `tauri::State<'_, AppState>` 访问。
///
/// 字段命名遵循 ADR-010 第 3.1 节 + ADR-009 第 3.6 节。
#[derive(Clone)]
pub struct AppState {
    /// 统一 peer 状态库（ADR-009 第 3.2 节）
    pub peers: Arc<PeerRegistry>,
    /// Handshake DoS 限流器（ADR-009 第 3.6 节）
    pub rate_limiter: Arc<RateLimiter>,
    /// per-peer reqwest::Client 连接池（ADR-009 第 3.5 节）
    pub client_pool: Arc<ClientPool>,
    /// 应用生命周期管理器（ADR-010 第 3.1 节）
    pub lifecycle: Arc<Lifecycle>,
}

impl AppState {
    /// 构造 AppState（ADR-010 第 3.2 节 step 3 顺序）。
    ///
    /// 由 `lib.rs::run` 调用，在 tauri::Builder::default() 前完成。
    pub fn new() -> Self {
        // ADR-009 第 5 节 #5 构造顺序：
        //   1. ClientPool（无依赖）
        //   2. PeerRegistry（PR-4 落地时传入 client_pool；当前独立）
        //   3. RateLimiter（无依赖）
        //   4. Lifecycle（持有 health_cancel / task handles）
        let client_pool = Arc::new(ClientPool::new());
        let peers = Arc::new(PeerRegistry::new());
        let rate_limiter = Arc::new(RateLimiter::new());
        let lifecycle = Lifecycle::new();

        tracing::debug!(
            target: "app::state",
            "AppState::new() constructed (client_pool + peers + rate_limiter + lifecycle)"
        );

        Self {
            peers,
            rate_limiter,
            client_pool,
            lifecycle,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
