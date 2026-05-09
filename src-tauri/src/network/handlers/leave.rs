//! POST /peers/leave handler
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-3)
//!
//! PR-4 范围：
//! - 来源鉴权 is_known → 403
//! - seq dedupe → 200 静默丢
//! - 占位返 503（leave 业务逻辑留 PR-5+）
//!
//! 不在本 PR：PeerRegistry.remove / emit status-updated

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

use crate::app::state::AppState;
use crate::crypto::AadKind;
use crate::network::error::NetworkError;
use crate::network::protocol::LeaveReq;

/// POST /peers/leave
pub async fn handle_leave(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LeaveReq>,
) -> Result<StatusCode, NetworkError> {
    if !state.peers.is_known(&req.origin_device_id) {
        let err = NetworkError::NotInPeers;
        err.log();
        return Err(err);
    }

    if !state
        .peers
        .seen_seq_and_update(&req.origin_device_id, AadKind::Leave, req.seq)
    {
        return Ok(StatusCode::OK);
    }

    tracing::debug!(
        target: "network::leave",
        origin = %req.origin_device_id,
        seq = req.seq,
        "leave received (PR-4 placeholder; business logic PR-5+)"
    );

    Ok(StatusCode::SERVICE_UNAVAILABLE)
}
