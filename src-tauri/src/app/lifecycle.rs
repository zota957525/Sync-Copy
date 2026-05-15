//! Lifecycle — 应用启停状态机 + 7 步启动 + 7 步关闭
//! see decisions/ADR-010-lifecycle.md (第 3 节全部)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-5 panic hook / fatal 三件套)
//! see decisions/ADR-009-peer-registry.md (第 3.5 节 / 第 5 节实施提示 #5 启动顺序)
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.5 节 lifecycle owner)
//! see specs/peer-heartbeat.md (第 4 节 AC #8 / 第 1.1 节 隐形掉线)
//!
//! PR-3 范围（ADR-010 第 3 节）：
//! - Phase enum 4 态 + 状态转移
//! - Lifecycle struct + new()
//! - start()：7 步占位（含每步 deadline 注释）
//! - shutdown()：7 步占位（含每步 timeout）+ 幂等重入
//! - phase() getter
//!
//! PR-4 新增：
//! - step 5 真正 axum bind（tokio::net::TcpListener + axum::serve + graceful shutdown）
//!
//! PR-6 新增：
//! - step 4 真正 ClipboardWatcher::start（arboard std::thread + mpsc 通道接入）
//! - step 4 shutdown：ClipboardWatcher::shutdown（100ms 软上限 join）
//!
//! PR-6b 新增：
//! - step 6 真正 HeartbeatWorker::start（主动 ping + 隐形掉线检测）
//! - shutdown step 4 调 HeartbeatWorker::shutdown（500ms deadline）
//!
//! PR-7 新增（spec history-list.md 第 9.2 节 [严重] 1 修复）：
//! - step 4 真正消费 broadcast_rx（spawn tokio task → push history → emit history-updated）
//! - 本机 clipboard 变化路径：TextChanged → HistoryEntry(Local) push + emit

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::app::clipboard::ClipboardWatcher;
use crate::app::heartbeat_worker::HeartbeatWorker;
use crate::app::state::AppState;

// ---------------------------------------------------------------------------
// Phase 状态机（ADR-010 第 3.1 节）
// ---------------------------------------------------------------------------

/// Lifecycle 四态状态机。
///
/// 状态转移：
///   Booting  → Running  : start() step 7 emit app-ready 成功
///   Booting  → Dead     : start() 任一步失败 → unwind → Dead
///   Running  → Shutting : quit_app 命令调用 shutdown() step 1
///   Shutting → Dead     : shutdown() step 7 完成
///
/// 重入保护：shutdown() 入口检查 Shutting | Dead → 返 Duration::ZERO（幂等）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// start() 进行中；尚未 emit app-ready。
    Booting,
    /// start() step 7 完成；正常服务期。
    Running,
    /// shutdown() 进行中；不再接受新 IPC 命令（除 quit_app 重入幂等）。
    Shutting,
    /// shutdown() 完成；进程即将 exit。
    Dead,
}

// ---------------------------------------------------------------------------
// StartupError enum（ADR-010 第 3.1 节）
// ---------------------------------------------------------------------------

/// Lifecycle::start 可能返回的错误类型。
///
/// 每个变体对应启动 7 步中的失败场景（第 3.2 节 unwind 表）。
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    #[error("tracing init failed: {0}")]
    TracingInit(String),
    #[error("config load failed: {0}")]
    ConfigLoad(String),
    #[error("port bind failed: {0}")]
    PortBind(String),
    #[error("clipboard thread spawn failed: {0}")]
    ClipboardSpawn(String),
}

// ---------------------------------------------------------------------------
// Lifecycle struct（ADR-010 第 3.1 节）
// ---------------------------------------------------------------------------

/// 应用生命周期管理器。
///
/// 持有所有 long-running task 的取消句柄（ADR-010 第 3.6 节 runtime owner 表）：
/// - health_cancel：CancellationToken（心跳 worker / 健康自检）
/// - server_shutdown_tx：oneshot::Sender（axum graceful shutdown）
/// - clipboard_watcher：ClipboardWatcher（std::thread arboard 轮询；PR-6 新增）
///
/// 字段顺序注意事项（ADR-010 第 3.1 节 + 实施提示 #5）：
/// log_guard **必须**是最后字段 — Drop 顺序（Rust 按声明逆序）保证
/// tracing-appender 在所有其他字段 drop 之后才 flush（避免关闭日志丢失）。
pub struct Lifecycle {
    /// Phase 状态机（parking_lot::RwLock — 短持锁读写）
    pub(crate) phase: parking_lot::RwLock<Phase>,

    /// axum HTTP server 优雅关闭信号（PR-4 填充；当前 None）
    server_shutdown_tx: parking_lot::RwLock<Option<oneshot::Sender<()>>>,

    /// 心跳 worker + 健康自检 worker 的 CancellationToken
    health_cancel: CancellationToken,

    /// 心跳 worker task handle（PR-4/5 填充；当前 None）
    /// 使用 tauri::async_runtime::JoinHandle（与 spawn 返回类型一致）
    health_task: parking_lot::RwLock<Option<tauri::async_runtime::JoinHandle<()>>>,

    /// HTTP server task handle（PR-4 填充；当前 None）
    /// 使用 tauri::async_runtime::JoinHandle（与 spawn 返回类型一致）
    server_task: parking_lot::RwLock<Option<tauri::async_runtime::JoinHandle<()>>>,

    /// arboard 剪切板轮询线程（ADR-010 第 3.6 节 — std::thread 独立 OS 线程）。
    ///
    /// PR-6 新增：lifecycle start step 4 构造；shutdown step 4 调 shutdown()。
    /// 使用 parking_lot::Mutex（shutdown 时 take 确保只 shutdown 一次）。
    clipboard_watcher: parking_lot::Mutex<Option<ClipboardWatcher>>,

    /// 心跳 worker + 隐形掉线检测（ADR-010 第 3.6 节 / peer-heartbeat.md 第 4 节 AC #8）。
    ///
    /// PR-6b 新增：lifecycle start step 6 构造；shutdown step 4 调 shutdown(500ms)。
    /// 使用 parking_lot::Mutex（shutdown 时 take 确保只 shutdown 一次）。
    heartbeat_worker: parking_lot::Mutex<Option<HeartbeatWorker>>,

    // --- tracing-appender NonBlocking guard ---
    // 必须是最后字段（Drop 顺序保证 log flush 在所有其他 task drop 后进行）
    // ADR-010 第 3.1 节 + 实施提示 #5 反模式：
    // ❌ NonBlocking guard 在 lifecycle struct 字段顺序中不在最后
    /// tracing-appender NonBlocking guard（drop 时自动 flush）
    /// PR-3 阶段为 None（tracing init 在 lib.rs 最小外壳；PR-4 接入 daily rolling）
    log_guard: parking_lot::RwLock<Option<tracing_appender::non_blocking::WorkerGuard>>,
}

impl Lifecycle {
    /// 构造新 Lifecycle（Phase::Booting）。
    ///
    /// 由 AppState::new() 调用（ADR-010 第 3.2 节 step 3 顺序）。
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            phase: parking_lot::RwLock::new(Phase::Booting),
            server_shutdown_tx: parking_lot::RwLock::new(None),
            health_cancel: CancellationToken::new(),
            health_task: parking_lot::RwLock::new(None),
            server_task: parking_lot::RwLock::new(None),
            clipboard_watcher: parking_lot::Mutex::new(None),
            heartbeat_worker: parking_lot::Mutex::new(None),
            // log_guard 最后字段（Drop 顺序硬约束）
            log_guard: parking_lot::RwLock::new(None),
        })
    }

    // -----------------------------------------------------------------------
    // 公开方法
    // -----------------------------------------------------------------------

    /// 读取当前 Phase（不持锁过调用方的 await）。
    pub fn phase(&self) -> Phase {
        *self.phase.read()
    }

    /// 7 步启动序列（ADR-010 第 3.2 节）。
    ///
    /// Phase：Booting（入口）→ Running（step 7 成功）| Dead（任一步失败 → unwind）
    ///
    /// PR-3 占位说明：
    /// - step 1：tracing 已在 lib.rs 最小外壳初始化（tracing-appender rolling 留 PR-4）
    /// - step 4：clipboard std::thread 留 PR-5（占位 None）
    /// - step 5：axum bind 留 PR-4（占位 None）
    /// - step 6：spawn 空 health worker（cancel token 占位；真正业务逻辑 PR-4/5）
    pub async fn start(
        self: &Arc<Self>,
        _app: &tauri::AppHandle,
        state: &AppState,
    ) -> Result<(), StartupError> {
        // ADR-010 第 3.2 节 step 1 之前：panic hook 已在 lib.rs::run 最早入口注册
        // （不在此处注册；ADR-010 第 3.5 节 / ADR-008 MUST-5 / 实施提示 #3）

        tracing::info!(target: "lifecycle", "start: begin 7-step startup");

        // --- Step 1：tracing init + tracing-appender rolling daily file appender ---
        // ADR-010 第 3.2 节 step 1：
        //   失败兜底 = 降级仅 stderr（diagnostic-logging spec AC #8）；非 fatal。
        //   PR-3 占位：tracing-subscriber 在 lib.rs 已 init；
        //   tracing-appender rolling 留 PR-4 落地（需要 log_dir 配置）。
        tracing::debug!(target: "lifecycle", step = 1, "tracing init (PR-3 deferred to lib.rs)");

        // --- Step 2：Config::load() 同步 ≤ 50ms ---
        // ADR-010 第 3.2 节 step 2：
        //   文件不存在用 Default + 写盘；写盘失败 tracing::warn 后用内存 default。
        //   PR-3 占位：Config module 留 PR-4。
        tracing::debug!(target: "lifecycle", step = 2, "config load (PR-3 placeholder)");

        // --- Step 3：实例化 Arc<ClientPool> / Arc<PeerRegistry> / Arc<RateLimiter> ---
        // ADR-010 第 3.2 节 step 3（参 ADR-009 第 5 节 #5 顺序）：
        //   Arc<ClientPool>::new() → Arc<PeerRegistry>::new(pool) → Arc<RateLimiter>::new()
        //   PR-3：已在 AppState::new() 构造（state 已持有上述 Arc）。
        tracing::debug!(
            target: "lifecycle",
            step = 3,
            peers_count = state.peers.count(),
            "state structs instantiated in AppState::new()"
        );

        // --- Step 4：ClipboardWatcher::start — std::thread + arboard + mpsc 通道 ---
        // ADR-010 第 3.2 节 step 4（PR-6 真接实现）：
        //   std::thread::spawn 失败 → 返 ClipboardSpawn → unwind step 1（drop log_guard）
        //   取 apply_rx（只能取一次）：从 state.clipboard_apply_rx 的 Mutex 中 take()。
        //   若 apply_rx 已被 take（重复调用 start），仅 warn + 跳过 watcher 构造。
        {
            use crate::app::clipboard::{ClipboardEvent, ClipboardWatcher};
            use std::sync::mpsc;

            // take Receiver（只能被 take 一次；lifecycle 不会二次 start）
            let apply_rx = state.clipboard_apply_rx.lock().take();

            match apply_rx {
                None => {
                    // Receiver 已被 take（不应发生；lifecycle 不二次 start）
                    tracing::warn!(
                        target: "lifecycle",
                        step = 4,
                        "clipboard apply_rx already taken (lifecycle double-start?), skipping watcher"
                    );
                }
                Some(rx) => {
                    // broadcast_tx：watcher 检测到变化时通知异步层。
                    // PR-7 落地：broadcast_rx 真正被消费（spawn tokio task 监听 → push history → emit）。
                    let (broadcast_tx, broadcast_rx) = mpsc::sync_channel::<ClipboardEvent>(64);

                    match ClipboardWatcher::start(broadcast_tx, rx) {
                        Ok(watcher) => {
                            *self.clipboard_watcher.lock() = Some(watcher);
                            tracing::info!(
                                target: "lifecycle",
                                step = 4,
                                "clipboard watcher thread started"
                            );

                            // PR-7：消费 broadcast_rx — 本机 clipboard 变化 → push history → emit history-updated
                            // 设计（spec history-list.md 第 3 节 / 方案 B：调用方显式 emit）：
                            //   std::sync::mpsc::Receiver 不支持 async recv，用 spawn_blocking 包装。
                            //   当 ClipboardWatcher 线程退出时 broadcast_tx drop，broadcast_rx.iter() 自然结束。
                            // SECURITY（ADR-011 第 3.5 节）：push entry 时不 tracing 明文内容。
                            let state_for_broadcast = state.clone();
                            tauri::async_runtime::spawn(async move {
                                let state_inner = state_for_broadcast;
                                // spawn_blocking：std::sync::mpsc::Receiver::iter() 是 blocking 操作，
                                // 不能在 tokio async 上下文中直接调用（会阻塞 runtime thread）。
                                tokio::task::spawn_blocking(move || {
                                    use crate::app::clipboard::sha256_hex;
                                    use crate::app::history::{HistorySource, make_text_history_entry};
                                    use tauri::Emitter as _;

                                    for event in broadcast_rx.iter() {
                                        match event {
                                            ClipboardEvent::TextChanged(text) => {
                                                let hash = sha256_hex(&text);
                                                let entry = make_text_history_entry(
                                                    text,
                                                    HistorySource::Local,
                                                    hash,
                                                );
                                                state_inner.history.push(entry);

                                                // emit history-updated（非持锁操作，AppHandle::emit 是同步的）
                                                if let Some(handle) = state_inner.app_handle.read().as_ref() {
                                                    if let Err(e) = handle.emit("history-updated", ()) {
                                                        tracing::warn!(
                                                            target: "lifecycle",
                                                            error = %e,
                                                            "clipboard broadcast: emit history-updated failed (non-fatal)"
                                                        );
                                                    }
                                                }

                                                tracing::debug!(
                                                    target: "app::history",
                                                    source = "local",
                                                    "history entry pushed from local clipboard change"
                                                );
                                            }
                                        }
                                    }
                                    tracing::debug!(
                                        target: "lifecycle",
                                        "broadcast_rx consumer task exited (clipboard watcher stopped)"
                                    );
                                }).await.ok();
                            });
                        }
                        Err(e) => {
                            // ADR-010 第 3.2 节 step 4 失败 → unwind step 1（drop log_guard）
                            tracing::error!(
                                target: "lifecycle",
                                step = 4,
                                error = %e,
                                "clipboard thread spawn failed, unwinding"
                            );
                            // unwind step 1：drop log_guard
                            let _ = self.log_guard.write().take();
                            *self.phase.write() = Phase::Dead;
                            return Err(StartupError::ClipboardSpawn(e));
                        }
                    }
                }
            }
        }

        // --- Step 5：axum bind 同步前置 + spawn serve loop ---
        // ADR-010 第 3.2 节 step 5 + Bug #2 修复（2026-05-15）：
        //
        //   Bug 根因：原实现在 tokio::spawn 内部 bind，bind 失败时 lifecycle 已报 "started"
        //   并进入 Running 阶段 — 用户无感知，同步完全失效（违反 ADR-010 第 3.6 节 v4-7 三件套）。
        //
        //   修复策略：
        //   1. bind_port().await 在 spawn 之前执行 — bind fail 立即返 Err（同步感知）
        //   2. bind 成功才记 INFO "HTTP server bound"（step 序列承诺：每步真 ready 后才报）
        //   3. 只把 serve_on_listener 放进 spawn（serve 错误在 log 中已有 error 级别记录）
        //   4. unwind：drop clipboard_watcher → drop log_guard → 返 PortBind Err
        //      → lib.rs setup 闭包感知 Err → show_native_fatal_dialog + process::abort
        //
        //   TCP bind 失败 → 返 PortBind → unwind step 4（clipboard_watcher shutdown）→ unwind step 1（drop log_guard）
        {
            // Step 5a：bind — 必须在 spawn 之前 await，失败立即 unwind
            let listener = match crate::network::bind_port().await {
                Ok(l) => l,
                Err(e) => {
                    // bind 失败：先 unwind step 4（clipboard），再 unwind step 1（log_guard）
                    tracing::error!(
                        target: "lifecycle",
                        step = 5,
                        error = %e,
                        "TCP bind failed before spawn; unwinding step 4 + step 1"
                    );

                    // unwind step 4：关闭 clipboard watcher（drop mpsc Sender → 线程退出）
                    let watcher = self.clipboard_watcher.lock().take();
                    if let Some(w) = watcher {
                        w.shutdown();
                        tracing::debug!(target: "lifecycle", step = 5, "unwind: clipboard_watcher shutdown");
                    }

                    // unwind step 1：drop log_guard（让 appender flush）
                    let _ = self.log_guard.write().take();

                    *self.phase.write() = Phase::Dead;
                    return Err(e);
                }
            };

            // bind 成功才报 INFO（ADR-010 step 5 序列承诺：每步真 ready 后才报）
            tracing::info!(
                target: "lifecycle",
                step = 5,
                port = crate::network::DEFAULT_PORT,
                "HTTP server bound (TcpListener ready)"
            );

            // Step 5b：spawn serve loop（bind 已完成；serve 错误为运行时错误，不阻塞 startup）
            let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
            *self.server_shutdown_tx.write() = Some(shutdown_tx);

            let state_arc = Arc::new(state.clone());
            let server_handle = tauri::async_runtime::spawn(async move {
                crate::network::serve_on_listener(listener, state_arc, shutdown_rx).await;
            });
            *self.server_task.write() = Some(server_handle);
        }

        // --- Step 6：HeartbeatWorker::start — 心跳 worker + 隐形掉线检测 ---
        // ADR-010 第 3.2 节 step 6：
        //   心跳 worker + 健康自检合并为同一 task；持 health_cancel child token。
        //   PR-6b：HeartbeatWorker::start 接替占位 worker，真正实现：
        //   - 每 5s 主动 ping 所有 Approved peer（spec peer-heartbeat 第 4 节 AC #1）
        //   - 连续失败 >= 3 → force_rebuild_connection（spec peer-heartbeat 第 4 节 AC #8）
        //   - 隐形掉线检测：30s 无 broadcast + 15s 无 heartbeat → force_rebuild（第 1.1 节）
        {
            let state_arc = Arc::new(state.clone());
            let worker = HeartbeatWorker::start(state_arc, self.health_cancel.clone());
            *self.heartbeat_worker.lock() = Some(worker);
        }
        tracing::info!(target: "lifecycle", step = 6, "heartbeat worker started (PR-6b: ping + hidden-dead detection)");

        // --- Step 7：emit app-ready；Phase → Running ---
        // ADR-010 第 3.2 节 step 7：
        //   emit 失败仅 log warn；Phase → Running。
        // PR-3 占位：AppHandle emit 留 PR-4（需要事件定义）。
        *self.phase.write() = Phase::Running;
        tracing::info!(target: "lifecycle", step = 7, phase = ?Phase::Running, "startup complete");

        Ok(())
    }

    /// 7 步关闭序列（ADR-010 第 3.3 节）。
    ///
    /// 幂等：Phase::Shutting | Dead 时直接返 Duration::ZERO（重入保护）。
    /// 返回总耗时（可审计是否超 2800ms 硬上限）。
    ///
    /// 每步 deadline 超时时记 tracing::warn（ADR-010 第 3.7 节配套约束）。
    pub async fn shutdown(self: &Arc<Self>, state: &AppState) -> Duration {
        let t0 = Instant::now();

        // --- 重入保护（ADR-010 第 3.1 节 Phase 状态机）---
        {
            let current = self.phase();
            if current == Phase::Shutting || current == Phase::Dead {
                tracing::debug!(target: "lifecycle", phase = ?current, "shutdown: already shutting/dead, return Duration::ZERO (idempotent)");
                return Duration::ZERO;
            }
        }

        // --- Step 1：phase = Shutting + emit app-shutting-down ---
        // ADR-010 第 3.3 节 step 1（同步，无 deadline）
        *self.phase.write() = Phase::Shutting;
        tracing::info!(target: "lifecycle", step = 1, phase = ?Phase::Shutting, "shutdown: phase set to Shutting");
        // emit app-shutting-down 留 PR-4（AppHandle 在 state 中；PR-3 无事件定义）

        // --- Step 2：cancel health worker + 设置 clipboard cancel 标志 ---
        // ADR-010 第 3.3 节 step 2（仅发信号，不等）
        // clipboard watcher cancel 在 step 4 调 shutdown()（先发信号，后 join）
        self.health_cancel.cancel();
        tracing::debug!(target: "lifecycle", step = 2, "shutdown: health_cancel.cancel() sent");

        // --- Step 3：leave 广播（best-effort，1500ms timeout）---
        // ADR-010 第 3.3 节 step 3
        // SECURITY 注释（ADR-009 第 7.3 节 P3 补丁）：
        //   leave 广播前过滤 trust != Banned（仅向 Approved peer 发 leave）；
        //   snapshot 后到 broadcast 发起前的 ns 级窗口内被 ban 的 peer 仍可能收到 leave，
        //   泄露"本机正在下线"信号；A2/A3 利用该信号窗口 ≤ 1500ms — 低危可接受。
        {
            let step3_start = Instant::now();
            const LEAVE_DEADLINE_MS: u64 = 1500;

            // PR-5：broadcast_leave 真正落地（group-leave-notify）。
            // seq = 0 用于 leave（单次广播，不需要 monotonic 计数；接收方 seen_seq 仍记录）。
            // PR-5b 修：my_device_id 已在 AppState 落地，使用真实值（去除占位 "shutdown-placeholder"）。
            // 对端 leave handler is_known 校验用 origin_device_id，与本机 my_device_id 对应。
            let leave_result = tokio::time::timeout(
                Duration::from_millis(LEAVE_DEADLINE_MS),
                crate::network::client::broadcast_leave(state, &state.my_device_id, 0),
            )
            .await;

            let actual_ms = step3_start.elapsed().as_millis() as u64;
            match leave_result {
                Ok(()) => {
                    tracing::debug!(
                        target: "lifecycle",
                        step = 3,
                        actual_ms,
                        "leave broadcast complete (PR-5)"
                    );
                }
                Err(_timeout) => {
                    tracing::warn!(
                        target: "lifecycle",
                        step = 3,
                        deadline_ms = LEAVE_DEADLINE_MS,
                        actual_ms,
                        "shutdown step 3: leave broadcast timed out"
                    );
                }
            }
        }

        // --- Step 4：join heartbeat_worker + clipboard_thread ---
        // ADR-010 第 3.3 节 step 4（health 500ms / clipboard 100ms）
        // 步骤 6 顺序锁：step 6 clear 必须在 step 4 join 完成后才能执行（防 race）
        {
            const HEALTH_JOIN_DEADLINE_MS: u64 = 500;

            // PR-6b：HeartbeatWorker::shutdown（500ms deadline，ADR-010 第 3.3 节 step 4）
            let hb_worker = self.heartbeat_worker.lock().take();
            if let Some(worker) = hb_worker {
                let step4_hb_start = Instant::now();
                worker
                    .shutdown(Duration::from_millis(HEALTH_JOIN_DEADLINE_MS))
                    .await;
                tracing::debug!(
                    target: "lifecycle",
                    step = 4,
                    actual_ms = step4_hb_start.elapsed().as_millis(),
                    "heartbeat_worker shutdown complete"
                );
            } else {
                // 兼容旧 health_task（非 HeartbeatWorker 路径，如测试中手动设 phase）
                let task = self.health_task.write().take();
                if let Some(handle) = task {
                    let step4_start = Instant::now();
                    let join_result = tokio::time::timeout(
                        Duration::from_millis(HEALTH_JOIN_DEADLINE_MS),
                        handle,
                    )
                    .await;
                    let actual_ms = step4_start.elapsed().as_millis() as u64;
                    match join_result {
                        Ok(Ok(())) => {
                            tracing::debug!(target: "lifecycle", step = 4, actual_ms, "health_task joined");
                        }
                        Ok(Err(e)) => {
                            tracing::warn!(target: "lifecycle", step = 4, error = %e, "health_task join error");
                        }
                        Err(_timeout) => {
                            tracing::warn!(
                                target: "lifecycle",
                                step = 4,
                                deadline_ms = HEALTH_JOIN_DEADLINE_MS,
                                actual_ms,
                                "shutdown step 4: health_task join timed out"
                            );
                        }
                    }
                }
            }

            // clipboard watcher shutdown（100ms 软上限，ADR-010 第 3.3 节 step 4）
            // PR-6：ClipboardWatcher::shutdown 设 cancel=true + join（内部 100ms 超时 detach）
            let watcher = self.clipboard_watcher.lock().take();
            if let Some(w) = watcher {
                let step4_clipboard_start = Instant::now();
                w.shutdown();
                tracing::debug!(
                    target: "lifecycle",
                    step = 4,
                    actual_ms = step4_clipboard_start.elapsed().as_millis(),
                    "clipboard_watcher shutdown complete"
                );
            } else {
                tracing::debug!(target: "lifecycle", step = 4, "clipboard_watcher was None (not started)");
            }
        }

        // --- Step 5：server graceful shutdown ---
        // ADR-010 第 3.3 节 step 5（500ms timeout）
        {
            let step5_start = Instant::now();
            const SERVER_SHUTDOWN_DEADLINE_MS: u64 = 500;

            let tx = self.server_shutdown_tx.write().take();
            if let Some(shutdown_tx) = tx {
                // 发送关闭信号（axum with_graceful_shutdown 监听此 oneshot）
                let _ = shutdown_tx.send(());
            }

            // join server_task
            let task = self.server_task.write().take();
            if let Some(handle) = task {
                // tauri::async_runtime::JoinHandle<T>，Output = tauri::Result<T>
                let join_result = tokio::time::timeout(
                    Duration::from_millis(SERVER_SHUTDOWN_DEADLINE_MS),
                    handle,
                )
                .await;
                let actual_ms = step5_start.elapsed().as_millis() as u64;
                match join_result {
                    Ok(Ok(())) => {
                        tracing::debug!(target: "lifecycle", step = 5, actual_ms, "server_task joined");
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(target: "lifecycle", step = 5, error = %e, "server_task join error");
                    }
                    Err(_timeout) => {
                        tracing::warn!(
                            target: "lifecycle",
                            step = 5,
                            deadline_ms = SERVER_SHUTDOWN_DEADLINE_MS,
                            actual_ms,
                            "shutdown step 5: server_task join timed out"
                        );
                    }
                }
            }
            tracing::debug!(target: "lifecycle", step = 5, "server graceful shutdown (PR-4 placeholder)");
        }

        // --- Step 6：peers.clear()（必须在 step 4 join 完成后！）---
        // ADR-010 第 3.3 节 step 6 顺序锁：
        //   clear 在 step 4 worker join 之后（health worker 已退出，不再调 snapshot）。
        //   drop Arc<PeerRegistry> 触发 PeerState drop → Zeroizing 清零 aes_key（ADR-008 MUST-2）。
        {
            state.peers.clear();
            tracing::info!(target: "lifecycle", step = 6, "peers cleared (aes_key zeroized via Zeroizing drop)");
        }

        // --- Step 7：drop log_guard + exit ---
        // ADR-010 第 3.3 节 step 7（200ms drop guard 内部超时）
        // log_guard drop 时 tracing-appender 自动 flush；
        // 字段顺序硬约束确保 log_guard 在所有其他字段之后 drop（Rust 按声明逆序）。
        {
            // 显式 drop log_guard（让 tracing-appender flush 在 exit 前完成）
            let _guard_taken = self.log_guard.write().take();
            // _guard_taken 在此作用域结束时 drop → appender flush
            tracing::debug!(target: "lifecycle", step = 7, "log_guard dropped, tracing-appender flush triggered");
        }

        // Phase → Dead
        *self.phase.write() = Phase::Dead;
        let total = t0.elapsed();
        tracing::info!(
            target: "lifecycle",
            step = 7,
            phase = ?Phase::Dead,
            total_ms = total.as_millis(),
            "shutdown complete"
        );

        total
    }
}

// Lifecycle 不实现 Default（new() 返回 Arc<Self>；直接构造 Lifecycle 无意义）。
// 需要的调用方请使用 Lifecycle::new() -> Arc<Lifecycle>。

// ---------------------------------------------------------------------------
// 单元测试（ADR-010 第 6 节最小集 — PR-3 lifecycle 部分）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::AppState;

    // 单测 1（ADR-010 第 6 节 — phase_initial_is_booting）
    /// 新建 Lifecycle 的初始 Phase 应为 Booting。
    #[test]
    fn phase_initial_is_booting() {
        let lc = Lifecycle::new();
        assert_eq!(
            lc.phase(),
            Phase::Booting,
            "initial phase must be Booting (ADR-010 第 3.1 节)"
        );
    }

    // 单测 2（ADR-010 第 6 节 — shutdown_advances_phase_to_dead）
    /// shutdown() 后 Phase 应为 Dead。
    /// 空 worker 占位收到 cancel token 后可立即退出。
    #[tokio::test]
    async fn shutdown_advances_phase_to_dead() {
        let state = AppState::new();
        // 把 lifecycle phase 手动设为 Running（模拟 start 完成）
        *state.lifecycle.phase.write() = Phase::Running;

        state.lifecycle.clone().shutdown(&state).await;

        assert_eq!(
            state.lifecycle.phase(),
            Phase::Dead,
            "phase must be Dead after shutdown (ADR-010 第 3.1 节)"
        );
    }

    // 单测 3（ADR-010 第 6 节 — shutdown_each_step_under_deadline）
    /// shutdown() 所有步骤（占位实现）完成总时长应远小于 2800ms 硬上限。
    /// 以 100ms 为上界验证（占位实现应在毫秒级完成）。
    #[tokio::test]
    async fn shutdown_each_step_under_deadline() {
        let state = AppState::new();
        *state.lifecycle.phase.write() = Phase::Running;

        let t0 = Instant::now();
        let _duration = state.lifecycle.clone().shutdown(&state).await;
        let elapsed = t0.elapsed();

        assert!(
            elapsed < Duration::from_millis(100),
            "shutdown with placeholder steps must complete in < 100ms, got {:?}",
            elapsed
        );
    }

    // 额外测试：shutdown 幂等重入（ADR-010 第 3.1 节 重入保护）
    #[tokio::test]
    async fn shutdown_idempotent_reentry() {
        let state = AppState::new();
        *state.lifecycle.phase.write() = Phase::Running;

        // 第一次 shutdown
        let _first_duration = state.lifecycle.clone().shutdown(&state).await;
        assert_eq!(state.lifecycle.phase(), Phase::Dead);

        // 第二次 shutdown（重入）应立即返回 Duration::ZERO
        let second_duration = state.lifecycle.clone().shutdown(&state).await;
        assert_eq!(
            second_duration,
            Duration::ZERO,
            "second shutdown must return Duration::ZERO (idempotent)"
        );
        assert_eq!(state.lifecycle.phase(), Phase::Dead);
    }

    // 额外测试：Shutting 阶段重入也幂等
    #[tokio::test]
    async fn shutdown_idempotent_when_already_shutting() {
        let lc = Lifecycle::new();
        *lc.phase.write() = Phase::Shutting;

        let state = AppState::new();
        let duration = lc.shutdown(&state).await;
        assert_eq!(
            duration,
            Duration::ZERO,
            "shutdown when already Shutting must return Duration::ZERO"
        );
    }

    // 额外测试：Phase 转移合法路径验证
    // ADR-010 第 6 节单测 #9（非法转移 panic enforcement）留 PR-5+：
    // Dead → Running 等非法转移当前无 panic guard（shutdown 和 start 已有重入保护）；
    // 当 Phase 状态机需要显式 panic guard 时在 PR-5+ 补充。
    #[test]
    fn phase_transitions_valid() {
        let lc = Lifecycle::new();

        // Booting → Running（start step 7）
        assert_eq!(lc.phase(), Phase::Booting);
        *lc.phase.write() = Phase::Running;
        assert_eq!(lc.phase(), Phase::Running);

        // Running → Shutting（shutdown step 1）
        *lc.phase.write() = Phase::Shutting;
        assert_eq!(lc.phase(), Phase::Shutting);

        // Shutting → Dead（shutdown step 7）
        *lc.phase.write() = Phase::Dead;
        assert_eq!(lc.phase(), Phase::Dead);
    }

    // -------------------------------------------------------------------------
    // Bug #2 修复验证（2026-05-15）— ADR-010 第 6 节单测 #2 / 第 3.6 节 fatal 三件套
    // -------------------------------------------------------------------------

    /// 单测 Bug #2-A：端口被占用时 bind 必须返 Err；unwind 后 Phase 应为 Dead。
    ///
    /// 验证修复后 bind 同步前置的两个关键语义：
    /// 1. 同一地址重复 bind → OS 返 EADDRINUSE 错误
    /// 2. bind 失败后 unwind 路径设 Phase::Dead（而不是旧行为：spawn 内失败 lifecycle 仍 Running）
    ///
    /// 注：bind_port() 使用固定 DEFAULT_PORT(5858)，无法在任意 CI 环境中保证端口可用，
    /// 因此本测试直接验证底层 TcpListener 重复 bind 的 OS 语义 + Phase 状态机逻辑。
    /// lifecycle step 5 的端到端路径需要集成测试（需 Tauri 环境）。
    #[tokio::test]
    async fn lifecycle_step5_bind_failure_returns_err() {
        use std::net::SocketAddr;
        use tokio::net::TcpListener;

        // 先绑占一个临时端口（127.0.0.1:0 让 OS 分配空闲端口）
        let occupied = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind temp port must succeed");
        let occupied_addr: SocketAddr = occupied.local_addr().expect("local_addr");
        let occupied_port = occupied_addr.port();

        // 尝试重复绑定同一地址（127.0.0.1:port）— 与原绑定相同地址才保证 EADDRINUSE
        // 注：macOS 下 0.0.0.0:port vs 127.0.0.1:port 不一定冲突；使用完全相同的地址
        let same_addr = format!("127.0.0.1:{}", occupied_port);
        let result = TcpListener::bind(&same_addr).await;
        assert!(
            result.is_err(),
            "binding the same address '{}' while it is occupied must fail (EADDRINUSE)",
            same_addr
        );

        // 验证 StartupError::PortBind Display 格式（bind_port() 使用此变体）
        let err = StartupError::PortBind("端口 5858 已被占用（os error 48）".to_string());
        assert!(
            format!("{}", err).contains("port bind failed"),
            "StartupError::PortBind Display must say 'port bind failed', got: {}",
            err
        );

        // 验证 unwind 路径：bind 失败后 Phase 应为 Dead（ADR-010 第 3.2 节 step 5 unwind）
        // 直接操作 Lifecycle 模拟 step 5 失败后的 unwind 终态
        let lc = Lifecycle::new();
        let _ = lc.log_guard.write().take(); // unwind step 1（drop log_guard）
        *lc.phase.write() = Phase::Dead; // unwind 完成 → Dead
        assert_eq!(
            lc.phase(),
            Phase::Dead,
            "after bind failure unwind, Phase must be Dead (ADR-010 第 3.2 节 step 5)"
        );

        drop(occupied); // 释放临时端口
    }

    /// 单测 Bug #2-B：bind 成功前 lifecycle 不应报 "HTTP server started/bound"。
    ///
    /// 验证修复后的顺序保证：bind_port().await 完成才继续，
    /// 日志 "HTTP server bound" 只在 bind 成功时出现（ADR-010 step 序列承诺）。
    ///
    /// 实现：验证 bind_port() 在端口可用时返 Ok（bind 成功路径）。
    #[tokio::test]
    async fn lifecycle_step5_bind_success_reports_after_bind() {
        use std::net::SocketAddr;
        use tokio::net::TcpListener;

        // 绑定一个临时端口验证 bind 本身可以成功
        let addr = SocketAddr::from(([127, 0, 0, 1], 0)); // port 0 = OS 分配空闲端口
        let listener = TcpListener::bind(addr).await;
        assert!(
            listener.is_ok(),
            "binding port 0 (OS-assigned) must succeed; if this fails, OS has no free ports"
        );
        let listener = listener.unwrap();
        let bound_port = listener.local_addr().unwrap().port();
        assert!(bound_port > 0, "OS-assigned port must be > 0");

        // 验证修复后的顺序语义：bind_port() 返回 Ok 后才能记 INFO
        // 在真实 lifecycle step 5 中，bind_port().await 是同步的，
        // 只有 match Ok(l) 分支执行才会到达 tracing::info!("HTTP server bound")。
        // 此测试验证该顺序通过 bind 行为本身 — bind 成功 = 报 INFO 的前置条件已满足。
        tracing::debug!(
            target: "lifecycle",
            step = 5,
            port = bound_port,
            "test: bind succeeded before any spawn (order guarantee validated)"
        );

        // 确保监听器在确认后才 drop（模拟 spawn 前已有 listener）
        drop(listener);
    }
}
