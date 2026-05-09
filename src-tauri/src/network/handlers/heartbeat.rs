//! POST /heartbeat handler
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-3)
//!
//! PR-4 范围：
//! - 来源鉴权 is_known → 403
//! - 占位返 503（心跳业务逻辑留 PR-5+）
//!
//! 不在本 PR：心跳计数更新 / last_heartbeat_at 更新

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

use crate::app::state::AppState;
use crate::network::error::NetworkError;
use crate::network::protocol::HeartbeatReq;

/// POST /heartbeat
pub async fn handle_heartbeat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<HeartbeatReq>,
) -> Result<StatusCode, NetworkError> {
    if !state.peers.is_known(&req.origin_device_id) {
        let err = NetworkError::NotInPeers;
        err.log();
        return Err(err);
    }

    tracing::debug!(
        target: "network::heartbeat",
        origin = %req.origin_device_id,
        seq = req.seq,
        "heartbeat received (PR-4 placeholder; business logic PR-5+)"
    );

    Ok(StatusCode::SERVICE_UNAVAILABLE)
}
