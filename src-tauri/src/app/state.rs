//! AppState — 应用全局状态聚合
//! see decisions/ADR-010-lifecycle.md (第 3.2 节 step 3 顺序)
//! see decisions/ADR-009-peer-registry.md (第 3.5 节 / 第 5 节 #5 构造顺序)
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.5 节 AppState struct)
//! see specs/clipboard-text-sync.md (PR-5 clipboard_apply_tx 占位)
//!
//! 构造顺序（ADR-009 第 5 节 #5 + ADR-010 第 3.2 节 step 3）：
//!   Arc<ClientPool>::new()
//!   → Arc<PeerRegistry>::new()  [PR-4 落地时传入 client_pool]
//!   → Arc<RateLimiter>::new()
//!   → Arc<Lifecycle>::new()
//!
//! PR-3 注意：PeerRegistry::new() 当前不接受 client_pool 参数（PR-2 已落接口）。
//! PR-4 Lifecycle 落地时在 PeerRegistry::remove / ban 内补充 client_pool.remove 原子顺序。
//! PR-5 新增：clipboard_apply_tx 占位（PR-6 真接 arboard 线程时填入真实 Sender）。

use std::sync::Arc;

use crate::app::client_pool::ClientPool;
use crate::app::lifecycle::Lifecycle;
use crate::peer::rate_limit::RateLimiter;
use crate::peer::PeerRegistry;

// ---------------------------------------------------------------------------
// ApplyClipboardEvt（PR-5 占位；PR-6 arboard 线程接入时使用）
// ---------------------------------------------------------------------------

/// 剪切板内容应用事件（从解密 handler 发到 arboard 线程）。
///
/// PR-5 占位：`clipboard_apply_tx` 当前为 None；
/// PR-6 起 arboard 线程时填入真实 mpsc::Sender<ApplyClipboardEvt>，
/// clipboard handler 将把解密后的明文通过此 channel 发到 arboard 线程写 OS 剪切板。
///
/// SECURITY（ADR-011 第 3.5 节）：
/// 此结构体携带剪切板明文，敏感性等同于 OS 剪切板内容；
/// 禁止 tracing 输出 / 落盘 / 跨进程传递。
#[allow(dead_code)] // PR-6 前不使用；保留字段定义供类型检查
pub struct ApplyClipboardEvt {
    /// 内容类型（"text" | "image_png"）
    pub kind: String,
    /// 明文字节（解密后）
    pub data: Vec<u8>,
}

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
    /// 剪切板内容应用 channel（PR-5 占位，PR-6 真接 arboard 线程时填入）。
    ///
    /// clipboard handler 解密成功后通过此 Sender 把明文发到 arboard 专属线程写 OS 剪切板。
    /// 当前为 None：PR-6 启动 arboard 线程后赋值真实 Sender；handler 侧已有
    /// `if let Some(tx) = &state.clipboard_apply_tx` 守卫（不会 panic）。
    ///
    /// SECURITY（ADR-011 第 3.5 节）：
    /// 此 Sender 传递剪切板明文；Receiver 端（arboard 线程）禁止落盘 / tracing 明文。
    pub clipboard_apply_tx: Option<Arc<tokio::sync::mpsc::Sender<ApplyClipboardEvt>>>,
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
            // PR-5 占位：clipboard_apply_tx = None（PR-6 arboard 线程启动后填入）
            clipboard_apply_tx: None,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
