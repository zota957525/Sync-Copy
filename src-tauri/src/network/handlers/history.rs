//! DELETE /history + POST /delete_history + POST /history/clear handlers
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-3)
//!
//! PR-4 范围：
//! - 来源鉴权 is_known → 403（MUST-3）
//! - seq dedupe → 200 静默丢
//! - 占位返 503（历史清除业务逻辑留 PR-5+）
//!
//! 端点列表（本文件）：
//!   POST /delete_history
//!   POST /history/clear  （DELETE /history 按 ADR-003 第 3.2 节选项 A 可用任一方法）

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

use crate::app::state::AppState;
use crate::crypto::AadKind;
use crate::network::error::NetworkError;
use crate::network::protocol::{ClearHistoryReq, DeleteHistoryReq};

// ---------------------------------------------------------------------------
// POST /delete_history
// ---------------------------------------------------------------------------

/// POST /delete_history — 跨机删除单条历史（history-sync-delete）。
pub async fn handle_delete_history(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeleteHistoryReq>,
) -> Result<StatusCode, NetworkError> {
    if !state.peers.is_known(&req.origin_device_id) {
        let err = NetworkError::NotInPeers;
        err.log();
        return Err(err);
    }

    if !state
        .peers
        .seen_seq_and_update(&req.origin_device_id, AadKind::DeleteHistory, req.seq)
    {
        return Ok(StatusCode::OK);
    }

    tracing::debug!(
        target: "network::history",
        origin = %req.origin_device_id,
        seq = req.seq,
        "delete_history received (PR-4 placeholder; business logic PR-5+)"
    );

    Ok(StatusCode::SERVICE_UNAVAILABLE)
}

// ---------------------------------------------------------------------------
// POST /history/clear
// ---------------------------------------------------------------------------

/// POST /history/clear — 跨机清空所有历史（history-sync-delete）。
pub async fn handle_clear_history(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ClearHistoryReq>,
) -> Result<StatusCode, NetworkError> {
    if !state.peers.is_known(&req.origin_device_id) {
        let err = NetworkError::NotInPeers;
        err.log();
        return Err(err);
    }

    if !state
        .peers
        .seen_seq_and_update(&req.origin_device_id, AadKind::ClearHistory, req.seq)
    {
        return Ok(StatusCode::OK);
    }

    tracing::debug!(
        target: "network::history",
        origin = %req.origin_device_id,
        seq = req.seq,
        "history/clear received (PR-4 placeholder; business logic PR-5+)"
    );

    Ok(StatusCode::SERVICE_UNAVAILABLE)
}
