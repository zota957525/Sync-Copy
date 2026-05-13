//! POST /peers/leave handler
//! see specs/group-leave-notify.md (第 3 节)
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-3 / MUST-4)
//! see decisions/ADR-009-peer-registry.md (第 3.2 节 remove 接口 / 第 3.5 节 MUST-4 原子顺序)
//!
//! PR-5 业务逻辑：
//! - 来源鉴权 is_known → 403（MUST-3）
//! - seq dedupe → 200 静默丢
//! - PeerRegistry.remove(id)：原子完成 inner.remove → approved.remove → banned.remove → client_pool.remove
//!   （MUST-4 原子顺序；client_pool.remove 已在 PeerRegistry::remove 内部调用）
//! - 返 200 OK

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

use crate::app::state::AppState;
use crate::crypto::AadKind;
use crate::network::error::NetworkError;
use crate::network::protocol::LeaveReq;

/// POST /peers/leave
///
/// leave 收到后立即移除 peer：
/// - PeerRegistry::remove(id) 原子完成 inner + approved + banned + client_pool 四步移除
///   （ADR-009 第 3.5 节调用顺序契约；client_pool.remove 由 PeerRegistry::remove 内部调用）
/// - 不 emit status-updated（PeerRegistry 不持 Tauri AppHandle；TODO PR-7 前端接入时加 emit）
pub async fn handle_leave(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LeaveReq>,
) -> Result<StatusCode, NetworkError> {
    // --- 步骤 1：来源鉴权 is_known（ADR-008 MUST-3）---
    if !state.peers.is_known(&req.origin_device_id) {
        let err = NetworkError::NotInPeers;
        err.log();
        return Err(err);
    }

    // --- 步骤 2：seq 去重（ADR-009 第 3.2 节 invariant 5）---
    if !state
        .peers
        .seen_seq_and_update(&req.origin_device_id, AadKind::Leave, req.seq)
    {
        tracing::debug!(
            target: "network::leave",
            origin = %req.origin_device_id,
            seq = req.seq,
            "leave seq replay, silently dropped"
        );
        return Ok(StatusCode::OK);
    }

    // --- 步骤 3：PeerRegistry.remove（MUST-4 原子顺序）---
    // ADR-009 第 3.5 节调用顺序契约（落实 ADR-008 MUST-4）：
    //   PeerRegistry::remove 内部按顺序：
    //     1. inner.remove(id)         → PeerState drop → aes_key Zeroizing 清零
    //     2. approved.remove(id)
    //     3. banned.remove(id)
    //     4. client_pool.remove(id)   → 由 peer/mod.rs 在 PeerRegistry::remove 中调用
    //
    // 注意：任何 handler 不得在 PeerRegistry::remove 之外直接调 client_pool.remove（反模式）
    let removed = state.peers.remove(&req.origin_device_id);

    if removed.is_some() {
        tracing::info!(
            target: "network::leave",
            origin = %req.origin_device_id,
            seq = req.seq,
            "leave: peer removed from registry (aes_key zeroized, client_pool entry removed)"
            // TODO PR-7：emit status-updated 到前端（PeerRegistry 不持 AppHandle）
        );
    } else {
        // peer 在 is_known 通过后 remove 无果 → 罕见 race（已被其他路径移除）
        tracing::debug!(
            target: "network::leave",
            origin = %req.origin_device_id,
            seq = req.seq,
            "leave: peer not found in inner during remove (already removed by another path)"
        );
    }

    Ok(StatusCode::OK)
}

// ---------------------------------------------------------------------------
// 单元测试（leave handler 鉴权逻辑）
// ---------------------------------------------------------------------------
//
// [低 nit #4 PR-5b review] leave_atomic_remove_inner_and_pool 已在 PR-5b 删除：
// 该测试使用孤立 pool（与 registry.client_pool 不共享），无法验证 invariant 3；
// 等效覆盖已由 peer::tests::remove_clears_client_pool_atomic 提供（peer/mod.rs）。

#[cfg(test)]
mod tests {
    use crate::peer::PeerRegistry;

    /// leave_rejects_unknown：未知 peer 的 leave → is_known false → 403 路径
    #[test]
    fn leave_rejects_unknown_peer_logic() {
        let registry = PeerRegistry::new_for_test();
        assert!(
            !registry.is_known("ghost-peer"),
            "is_known must return false for unknown peer"
        );
    }
}
