//! /peers/* handlers（trust / ban / approval/{forward,decide,dismiss} + /peers/announce）
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-3 / MUST-8)
//!
//! PR-4 范围：
//! - 来源鉴权 is_known → 403（MUST-3）
//! - seq dedupe → 200 静默丢
//! - sanitize newcomer_name（MUST-8，approval/forward）
//! - 占位返 503（业务逻辑留 PR-5+）
//!
//! 端点列表（本文件）：
//!   POST /peers/announce
//!   POST /peers/trust
//!   POST /peers/ban
//!   POST /peers/approval/forward
//!   POST /peers/approval/decide
//!   POST /peers/approval/dismiss

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

use crate::app::state::AppState;
use crate::crypto::AadKind;
use crate::network::error::NetworkError;
use crate::network::protocol::{
    ApprovalDecideReq, ApprovalDismissReq, ApprovalForwardReq, TrustReq,
};
use crate::peer::sanitize::sanitize_device_name;

// ---------------------------------------------------------------------------
// POST /peers/announce
// ---------------------------------------------------------------------------

/// POST /peers/announce — 新 peer 宣告自身（group-discovery）。
///
/// PR-4 占位：入口鉴权 + 占位返 503。
/// PR-5+ 实现：接收宣告、触发审批弹框。
pub async fn handle_peers_announce(
    State(_state): State<Arc<AppState>>,
) -> Result<StatusCode, NetworkError> {
    tracing::debug!(target: "network::peers", "peers/announce received (PR-4 placeholder)");
    Ok(StatusCode::SERVICE_UNAVAILABLE)
}

// ---------------------------------------------------------------------------
// POST /peers/trust
// ---------------------------------------------------------------------------

/// POST /peers/trust — 信任传播（group-trust-gossip）。
pub async fn handle_trust(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TrustReq>,
) -> Result<StatusCode, NetworkError> {
    if !state.peers.is_known(&req.origin_device_id) {
        let err = NetworkError::NotInPeers;
        err.log();
        return Err(err);
    }

    if !state
        .peers
        .seen_seq_and_update(&req.origin_device_id, AadKind::Trust, req.seq)
    {
        return Ok(StatusCode::OK);
    }

    tracing::debug!(
        target: "network::peers",
        origin = %req.origin_device_id,
        seq = req.seq,
        "trust received (PR-4 placeholder; business logic PR-5+)"
    );

    Ok(StatusCode::SERVICE_UNAVAILABLE)
}

// ---------------------------------------------------------------------------
// POST /peers/ban
// ---------------------------------------------------------------------------

/// POST /peers/ban — 封禁传播（group-trust-gossip）。
pub async fn handle_ban(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TrustReq>,
) -> Result<StatusCode, NetworkError> {
    if !state.peers.is_known(&req.origin_device_id) {
        let err = NetworkError::NotInPeers;
        err.log();
        return Err(err);
    }

    if !state
        .peers
        .seen_seq_and_update(&req.origin_device_id, AadKind::Ban, req.seq)
    {
        return Ok(StatusCode::OK);
    }

    tracing::debug!(
        target: "network::peers",
        origin = %req.origin_device_id,
        seq = req.seq,
        "ban received (PR-4 placeholder; business logic PR-5+)"
    );

    Ok(StatusCode::SERVICE_UNAVAILABLE)
}

// ---------------------------------------------------------------------------
// POST /peers/approval/forward
// ---------------------------------------------------------------------------

/// POST /peers/approval/forward — 审批请求转发（group-approval）。
pub async fn handle_approval_forward(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ApprovalForwardReq>,
) -> Result<StatusCode, NetworkError> {
    if !state.peers.is_known(&req.origin_device_id) {
        let err = NetworkError::NotInPeers;
        err.log();
        return Err(err);
    }

    if !state
        .peers
        .seen_seq_and_update(&req.origin_device_id, AadKind::Approval, req.seq)
    {
        return Ok(StatusCode::OK);
    }

    // MUST-8：sanitize newcomer_name（防 Bidi / 控制字符在审批弹框 UI 欺骗）
    let _safe_name = sanitize_device_name(&req.newcomer_name);

    tracing::debug!(
        target: "network::peers",
        origin = %req.origin_device_id,
        seq = req.seq,
        "approval/forward received (PR-4 placeholder; business logic PR-5+)"
    );

    Ok(StatusCode::SERVICE_UNAVAILABLE)
}

// ---------------------------------------------------------------------------
// POST /peers/approval/decide
// ---------------------------------------------------------------------------

/// POST /peers/approval/decide — 审批决策回流（group-approval）。
pub async fn handle_approval_decide(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ApprovalDecideReq>,
) -> Result<StatusCode, NetworkError> {
    if !state.peers.is_known(&req.origin_device_id) {
        let err = NetworkError::NotInPeers;
        err.log();
        return Err(err);
    }

    tracing::debug!(
        target: "network::peers",
        origin = %req.origin_device_id,
        seq = req.seq,
        approved = req.approved,
        "approval/decide received (PR-4 placeholder; business logic PR-5+)"
    );

    Ok(StatusCode::SERVICE_UNAVAILABLE)
}

// ---------------------------------------------------------------------------
// POST /peers/approval/dismiss
// ---------------------------------------------------------------------------

/// POST /peers/approval/dismiss — 审批取消（group-approval）。
pub async fn handle_approval_dismiss(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ApprovalDismissReq>,
) -> Result<StatusCode, NetworkError> {
    if !state.peers.is_known(&req.origin_device_id) {
        let err = NetworkError::NotInPeers;
        err.log();
        return Err(err);
    }

    tracing::debug!(
        target: "network::peers",
        origin = %req.origin_device_id,
        seq = req.seq,
        "approval/dismiss received (PR-4 placeholder; business logic PR-5+)"
    );

    Ok(StatusCode::SERVICE_UNAVAILABLE)
}
