//! network — HTTP server skeleton + router
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节 12 端点 + 7 状态码统一表)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-3/6/7/8)
//! see decisions/ADR-010-lifecycle.md (第 3.2 节 step 5 axum bind + graceful shutdown)
//!
//! PR-4 范围：
//! - build_router()：12 端点注册 + DefaultBodyLimit 7MB（ADR-008 MUST-6）
//! - start_server()：tokio::net::TcpListener::bind + axum::serve + graceful shutdown
//! - 监听端口默认 5858（specs/_assumptions.md A9）
//! - shutdown 信号：oneshot::Receiver（lifecycle step 5）

pub mod client;
pub mod error;
pub mod handlers;
pub mod protocol;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::routing::post;
use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::app::state::AppState;
use crate::network::handlers::{
    clipboard::handle_clipboard,
    file::handle_file,
    handshake::handle_handshake,
    heartbeat::handle_heartbeat,
    history::{handle_clear_history, handle_delete_history},
    leave::handle_leave,
    peers::{
        handle_approval_decide, handle_approval_dismiss, handle_approval_forward, handle_ban,
        handle_peers_announce, handle_trust,
    },
};

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 默认监听端口（specs/_assumptions.md A9）。
/// settings-panel 未来可改（留 PR-5+）。
pub const DEFAULT_PORT: u16 = 5858;

/// DefaultBodyLimit：7 MB（ADR-008 MUST-6 配套）
/// 5 MB 文件 + base64 33% 膨胀 + GCM tag 16B + JSON header overhead ≈ 6.7 MB → 收紧到 7 MB。
const BODY_LIMIT_BYTES: usize = 7 * 1024 * 1024;

// ---------------------------------------------------------------------------
// build_router（ADR-003 第 3.2 节 12 端点）
// ---------------------------------------------------------------------------

/// 构造 axum Router（12 端点 + DefaultBodyLimit 7MB）。
///
/// 所有路由挂载 AppState 作为 axum State Extension。
///
/// 端点列表（ADR-003 第 3.2 节选项 B + v0 12 端点）：
///   POST /handshake
///   POST /peers/announce
///   POST /clipboard
///   POST /file
///   POST /heartbeat
///   POST /peers/leave
///   POST /peers/trust
///   POST /peers/ban
///   POST /peers/approval/forward
///   POST /peers/approval/decide
///   POST /peers/approval/dismiss
///   POST /delete_history  （+ POST /history/clear）
///
/// 注：ADR-003 第 3.2 节选项 A 列出 12 个端点包含 /ping GET（探活）；
/// v2 把 /ping 合并到心跳策略后不另立端点；/heartbeat POST 取代 /ping GET。
/// handler 入口均满足：sanitize → MUST-3/6/7 校验 → 占位返 503。
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        // --- 握手（group-discovery / group-approval）---
        .route("/handshake", post(handle_handshake))
        // --- peer 宣告（group-discovery）---
        .route("/peers/announce", post(handle_peers_announce))
        // --- 剪切板（clipboard-text-sync / clipboard-image-sync / clipboard-snapshot-sync）---
        .route("/clipboard", post(handle_clipboard))
        // --- 文件传输（file-transfer-drag）---
        .route("/file", post(handle_file))
        // --- 心跳（peer-heartbeat）---
        .route("/heartbeat", post(handle_heartbeat))
        // --- peer 离线广播（group-leave-notify）---
        .route("/peers/leave", post(handle_leave))
        // --- trust gossip（group-trust-gossip）---
        .route("/peers/trust", post(handle_trust))
        .route("/peers/ban", post(handle_ban))
        // --- 审批（group-approval）---
        .route("/peers/approval/forward", post(handle_approval_forward))
        .route("/peers/approval/decide", post(handle_approval_decide))
        .route("/peers/approval/dismiss", post(handle_approval_dismiss))
        // --- 历史（history-sync-delete）---
        .route("/delete_history", post(handle_delete_history))
        .route("/history/clear", post(handle_clear_history))
        // --- 全局 DefaultBodyLimit（ADR-008 MUST-6）---
        .layer(DefaultBodyLimit::max(BODY_LIMIT_BYTES))
        // --- AppState 注入 ---
        .with_state(state)
}

// ---------------------------------------------------------------------------
// start_server（ADR-010 第 3.2 节 step 5 真正落地）
// ---------------------------------------------------------------------------

/// Step 5a：绑定 TCP 端口（必须在 tokio::spawn 之前 await，确保 bind 成功才报 INFO）。
///
/// Bug #2 修复（ADR-010 第 3.2 节 step 5 + ADR-010 第 3.6 节 fatal 三件套）：
/// 原实现在 spawn 内部 bind，导致 bind 失败时 lifecycle 已报 "started" 并进入 Running 阶段。
/// 修复后：bind 同步前置，失败立即返 Err，lifecycle step 5 unwind，进程 abort（v4-7 三件套）。
///
/// 端口：DEFAULT_PORT(5858)；`settings-panel` PR 后可从 Config 读取。
pub async fn bind_port() -> Result<TcpListener, crate::app::lifecycle::StartupError> {
    let addr = SocketAddr::from(([0, 0, 0, 0], DEFAULT_PORT));
    TcpListener::bind(addr).await.map_err(|e| {
        tracing::error!(
            target: "network::server",
            addr = %addr,
            error = %e,
            "TCP bind failed"
        );
        crate::app::lifecycle::StartupError::PortBind(format!(
            "端口 {} 已被占用（{}）",
            DEFAULT_PORT, e
        ))
    })
}

/// Step 5b：在已绑定的 listener 上启动 axum serve 循环（在 tokio::spawn 内运行）。
///
/// 调用方（lifecycle step 5）已在 spawn 前完成 bind_port()，此函数只做 serve。
/// shutdown_rx：lifecycle shutdown step 5 通过 oneshot send(()) 触发优雅关闭。
pub async fn serve_on_listener(
    listener: TcpListener,
    state: Arc<AppState>,
    shutdown_rx: oneshot::Receiver<()>,
) {
    let router = build_router(state);

    // axum::serve + graceful shutdown（ADR-010 第 3.3 节 step 5 500ms timeout）
    let result = axum::serve(
        listener,
        // ConnectInfo 提取 remote_addr（handshake handler 使用）
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        let _ = shutdown_rx.await;
        tracing::info!(target: "network::server", "graceful shutdown signal received");
    })
    .await;

    match result {
        Ok(()) => {
            tracing::info!(target: "network::server", "HTTP server stopped");
        }
        Err(e) => {
            tracing::error!(target: "network::server", error = %e, "axum serve error during run");
        }
    }
}

/// 启动 axum HTTP server（旧接口，保持向后兼容；仅供单元测试使用）。
///
/// 注意：此函数在内部 bind，bind 失败时返 Err；但调用方必须在 spawn 之前 await 此函数，
/// 否则 bind 失败发生在 spawn 内部，lifecycle 无法感知（Bug #2 根因）。
///
/// 新代码请使用 bind_port() + serve_on_listener()。
#[allow(dead_code)]
pub async fn start_server(
    state: Arc<AppState>,
    shutdown_rx: oneshot::Receiver<()>,
) -> Result<(), crate::app::lifecycle::StartupError> {
    let listener = bind_port().await?;

    tracing::info!(
        target: "network::server",
        addr = %listener.local_addr().unwrap_or_else(|_| SocketAddr::from(([0,0,0,0], DEFAULT_PORT))),
        "HTTP server listening"
    );

    serve_on_listener(listener, state, shutdown_rx).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// 单元测试（router smoke test）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::AppState;

    /// Router smoke test：build_router 不 panic，路由表可构造。
    #[test]
    fn build_router_does_not_panic() {
        let state = Arc::new(AppState::new());
        // build_router 返回 Router（不绑定端口），只验证构造不 panic
        let _router = build_router(state);
    }
}
