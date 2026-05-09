//! POST /clipboard handler
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-3 / MUST-8)
//!
//! PR-4 范围：
//! - is_known 校验 → NotInPeers 403（MUST-3）
//! - seen_seq_and_update → 重放 200 静默丢
//! - 占位返 503（crypto 解密留 PR-5+）
//!
//! 不在本 PR：crypto 真解密 / emit 剪切板更新事件

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

use crate::app::state::AppState;
use crate::crypto::AadKind;
use crate::network::error::NetworkError;
use crate::network::protocol::ClipboardReq;

/// POST /clipboard
pub async fn handle_clipboard(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ClipboardReq>,
) -> Result<StatusCode, NetworkError> {
    // --- 来源鉴权（ADR-008 MUST-3）---
    if !state.peers.is_known(&req.origin_device_id) {
        let err = NetworkError::NotInPeers;
        err.log();
        return Err(err);
    }

    // --- seq 去重（ADR-008 第 4.2 节）---
    let kind = if req.kind == "image_png" {
        AadKind::ImagePng
    } else {
        AadKind::Text
    };
    if !state
        .peers
        .seen_seq_and_update(&req.origin_device_id, kind, req.seq)
    {
        // 重放：静默 200
        tracing::debug!(
            target: "network::clipboard",
            origin = %req.origin_device_id,
            seq = req.seq,
            "clipboard seq replay, silently dropped"
        );
        return Ok(StatusCode::OK);
    }

    tracing::debug!(
        target: "network::clipboard",
        origin = %req.origin_device_id,
        seq = req.seq,
        kind = %req.kind,
        "clipboard received (PR-4 placeholder; crypto PR-5+)"
    );

    // 占位返 503
    Ok(StatusCode::SERVICE_UNAVAILABLE)
}
