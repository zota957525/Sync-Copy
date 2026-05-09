//! POST /handshake handler
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-3 403 不可区分 / MUST-7 DoS 限流 / MUST-8 sanitize)
//! see decisions/ADR-009-peer-registry.md (第 3.6 节 RateLimiter)
//!
//! PR-4 范围：
//! - sanitize device_name（MUST-8）
//! - check_handshake DoS 限流（MUST-7）→ 429
//! - 占位返 503（业务逻辑留 PR-5+）
//!
//! 不在本 PR 实现（PR-5+）：
//! - X25519 密钥协商 + HKDF 派生
//! - PeerRegistry.insert + client_pool.insert
//! - 审批弹框 emit
//! - 已知 peer re-handshake 逻辑

use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::Json;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::app::state::AppState;
use crate::network::error::NetworkError;
use crate::network::protocol::HandshakeReq;
use crate::peer::sanitize::sanitize_device_name;

/// POST /handshake
///
/// 入口检查：
/// 1. DoS 限流（ADR-008 MUST-7）→ 429
/// 2. sanitize device_name（ADR-008 MUST-8）
/// 3. 占位返 503（业务逻辑 PR-5+）
pub async fn handle_handshake(
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<HandshakeReq>,
) -> Result<StatusCode, NetworkError> {
    // --- MUST-7：handshake DoS 限流（ADR-008 第 4.3 节 / ADR-009 第 3.6 节）---
    // SECURITY (ADR-009 第 7.3 节 P3 注释)：
    //   未认证 device_id 不进 tracing fields；仅 check_handshake 返 TooManyRequests 时记 IP。
    let remote_ip = remote_addr.ip();
    if let crate::peer::rate_limit::RateLimitDecision::TooManyRequests = state
        .rate_limiter
        .check_handshake(remote_ip, &req.device_id)
    {
        // 429：body 不区分 per-pair vs 全局（ADR-008 第 4.3 节）
        let err = NetworkError::RateLimited;
        err.log();
        return Err(err);
    }

    // --- MUST-8：sanitize device_name（ADR-008 第 4.4 节）---
    let _sanitized_name = sanitize_device_name(&req.device_name);
    // PR-5+ 将使用 _sanitized_name 构造 PeerState；当前占位

    tracing::debug!(
        target: "network::handshake",
        remote_ip = %remote_ip,
        // SECURITY: device_id 是已认证标识（UUID），可进 tracing fields
        // 但此处请求尚未认证（握手请求）；不进 fields（P3 约束）
        "handshake received (PR-4 placeholder; business logic PR-5+)"
    );

    // 占位返 503（PR-5+ 替换为真实握手业务逻辑）
    Ok(StatusCode::SERVICE_UNAVAILABLE)
}
