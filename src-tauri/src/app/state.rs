//! AppState — 应用全局状态聚合
//! see decisions/ADR-010-lifecycle.md (第 3.2 节 step 3 顺序)
//! see decisions/ADR-009-peer-registry.md (第 3.5 节 / 第 5 节 #5 构造顺序)
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.5 节 AppState struct)
//! see specs/clipboard-text-sync.md (PR-6 arboard 接入 + mpsc 通道真接)
//! see specs/settings-panel.md (PR-FE-0 Config 持久化)
//! see specs/history-list.md (PR-FE-0 in-memory HistoryStore)
//!
//! 构造顺序（ADR-009 第 5 节 #5 + ADR-010 第 3.2 节 step 3）：
//!   my_device_id = uuid::Uuid::new_v4().to_string()
//!   → Arc<ClientPool>::new()
//!   → Arc<PeerRegistry>::new(client_pool)  [PR-5b 落地：传入 client_pool]
//!   → Arc<RateLimiter>::new()
//!   → Arc<Lifecycle>::new()
//!   → mpsc::sync_channel::<String>(64)  [PR-6 新增：clipboard apply 通道]
//!   → SharedConfig (Config::load)         [PR-FE-0 新增：config 持久化]
//!   → Arc<HistoryStore>::new()            [PR-FE-0 新增：in-memory history]
//!
//! PR-6 新增：
//!   - clipboard_apply_tx：std::sync::mpsc::SyncSender<String>（handler 解密后发 plaintext）
//!   - clipboard_apply_rx：Option<std::sync::mpsc::Receiver<String>>（lifecycle step 4 取走给 watcher）
//!
//! PR-FE-0 新增：
//!   - config：SharedConfig（Arc<Mutex<Config>>，lifecycle step 2 加载）
//!   - history：Arc<HistoryStore>（in-memory，进程退出即清）
//!
//! PR-5b 保留：
//!   - my_device_id 字段（启动期生成 UUID v4）
//!   - PeerRegistry::new(client_pool) 传入 client_pool

use std::sync::{
    mpsc::{self, Receiver, SyncSender},
    Arc,
};

use parking_lot::Mutex;

use crate::app::client_pool::ClientPool;
use crate::app::config::{load_shared_config, SharedConfig};
use crate::app::history::HistoryStore;
use crate::app::lifecycle::Lifecycle;
use crate::peer::rate_limit::RateLimiter;
use crate::peer::PeerRegistry;

// ---------------------------------------------------------------------------
// AppState struct（ADR-010 第 3.2 节 step 3）
// ---------------------------------------------------------------------------

/// 应用全局状态。作为 Tauri managed state 注入（`.manage(app_state)`）。
///
/// 所有字段为 Arc / 原始 Clone — 可跨线程共享（Send + Sync）。
/// Tauri command 通过 `tauri::State<'_, AppState>` 访问。
///
/// 字段命名遵循 ADR-010 第 3.1 节 + ADR-009 第 3.6 节。
///
/// PR-6 新增：
/// - `clipboard_apply_tx`：`SyncSender<String>`（handler 解密后发 plaintext；std::sync::mpsc）
/// - `clipboard_apply_rx`：`Mutex<Option<Receiver<String>>>`（lifecycle step 4 take 给 watcher）
#[derive(Clone)]
pub struct AppState {
    /// 本机 device_id（启动期 UUID v4 生成，整个生命周期不变）。
    ///
    /// 用途（PR-5b 修 ADR-008 MUST-3 / 严重 #2 #3）：
    /// - handshake handler 自连校验：req.device_id == my_device_id → 403
    /// - HandshakeResp.device_id 填真值（替换占位 "placeholder-my-device-id"）
    /// - broadcast_leave / broadcast_clipboard my_device_id 参数来源
    ///
    /// SECURITY（ADR-008 第 4.1 节）：
    /// device_id 是 UUID 形式（非敏感），可进 tracing fields。
    /// 但禁止在 403 响应 body 中返回（让攻击者枚举本机 device_id）。
    pub my_device_id: String,
    /// 统一 peer 状态库（ADR-009 第 3.2 节）
    pub peers: Arc<PeerRegistry>,
    /// Handshake DoS 限流器（ADR-009 第 3.6 节）
    pub rate_limiter: Arc<RateLimiter>,
    /// per-peer reqwest::Client 连接池（ADR-009 第 3.5 节）
    pub client_pool: Arc<ClientPool>,
    /// 应用生命周期管理器（ADR-010 第 3.1 节）
    pub lifecycle: Arc<Lifecycle>,

    /// 剪切板明文应用 Sender（PR-6 真接 arboard 线程）。
    ///
    /// clipboard handler 解密成功后通过此 SyncSender 把 plaintext 发到 arboard 专属线程。
    /// 使用 std::sync::mpsc::SyncSender（与 arboard std::thread 自然搭配；
    /// handler 侧在 async 上下文用 try_send 非阻塞发送，不依赖 tokio）。
    ///
    /// SECURITY（ADR-011 第 3.5 节）：
    /// 此 Sender 传递剪切板明文；禁止落盘 / tracing 明文字段。
    pub clipboard_apply_tx: Arc<SyncSender<String>>,

    /// 剪切板明文应用 Receiver（lifecycle step 4 take 给 ClipboardWatcher）。
    ///
    /// 使用 Mutex<Option<...>> 包装：
    /// - Lifecycle::start step 4 调 `.take()` 取走 Receiver 给 ClipboardWatcher，之后为 None。
    /// - AppState::Clone 时 Receiver 在 Arc<Mutex<Option<...>>> 内共享（不重复 take）。
    ///
    /// 注意：Receiver 只能被 take 一次；第二次 take 返 None（lifecycle 不会二次 start）。
    pub clipboard_apply_rx: Arc<Mutex<Option<Receiver<String>>>>,

    /// 持久化配置（device_name / listen_port / peer_hint）。
    ///
    /// PR-FE-0：lifecycle step 2 加载；commands.rs set_config 保存。
    /// 使用 Arc<parking_lot::Mutex<Config>>（短持锁，不跨 await）。
    pub config: SharedConfig,

    /// in-memory 历史列表（进程退出即清，spec 00-product-overview 第 3 节已锁定不持久化）。
    ///
    /// PR-FE-0：commands.rs get_history / delete_history_item / clear_history / recopy_history_item 读写。
    pub history: Arc<HistoryStore>,
}

impl AppState {
    /// 构造 AppState（ADR-010 第 3.2 节 step 3 顺序）。
    ///
    /// 由 `lib.rs::run` 调用，在 tauri::Builder::default() 前完成。
    pub fn new() -> Self {
        // ADR-009 第 5 节 #5 构造顺序（PR-5b 修正：client_pool 先于 peers）：
        //   0. my_device_id = uuid::Uuid::new_v4().to_string()（ADR-008 严重 #2/#3 修复）
        //   1. ClientPool（无依赖）
        //   2. PeerRegistry::new(client_pool)（传入 client_pool — ADR-008 MUST-4 契约）
        //   3. RateLimiter（无依赖）
        //   4. Lifecycle（持有 health_cancel / task handles）
        //   5. mpsc::sync_channel::<String>(64)（PR-6 新增：clipboard apply 通道）
        let my_device_id = uuid::Uuid::new_v4().to_string();
        let client_pool = Arc::new(ClientPool::new());
        // ADR-009 第 3.2 节：PeerRegistry::new 接受 Arc<ClientPool>
        // 保证 remove/ban 内部原子调 client_pool.remove（ADR-008 MUST-4）
        let peers = Arc::new(PeerRegistry::new(Arc::clone(&client_pool)));
        let rate_limiter = Arc::new(RateLimiter::new());
        let lifecycle = Lifecycle::new();

        // PR-6：clipboard apply 通道（std::sync::mpsc，与 arboard std::thread 自然搭配）
        // buffer_size=64：handler 解密后 try_send 非阻塞；arboard 线程 try_recv 消费。
        // SECURITY（ADR-011 第 3.5 节）：此 channel 传递剪切板明文，不 tracing 字段。
        let (clipboard_apply_tx, clipboard_apply_rx) = mpsc::sync_channel::<String>(64);

        // PR-FE-0：Config::load（lifecycle step 2 的真正实现）
        let config = load_shared_config();

        // PR-FE-0：in-memory HistoryStore
        let history = HistoryStore::new();

        tracing::debug!(
            target: "app::state",
            my_device_id = %my_device_id,
            "AppState::new() constructed (my_device_id + client_pool + peers + rate_limiter + lifecycle + clipboard_apply_channel + config + history)"
        );

        Self {
            my_device_id,
            peers,
            rate_limiter,
            client_pool,
            lifecycle,
            clipboard_apply_tx: Arc::new(clipboard_apply_tx),
            clipboard_apply_rx: Arc::new(Mutex::new(Some(clipboard_apply_rx))),
            config,
            history,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
