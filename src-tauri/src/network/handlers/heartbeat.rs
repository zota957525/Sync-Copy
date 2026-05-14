//! POST /heartbeat handler
//! see specs/peer-heartbeat.md (第 3 节)
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-3 / 5.2 节 last_sync 写时机)
//! see decisions/ADR-009-peer-registry.md (第 3.2 节 record_heartbeat_ok/fail)
//!
//! PR-5 业务逻辑：
//! - 来源鉴权 is_known + !is_banned → 403
//! - record_heartbeat_ok：更新 consecutive_heartbeat_failures=0 + last_heartbeat_at
//! - 注意：**不**更新 last_successful_sync_at（ADR-008 5.2 节 + ADR-009 第 3.2 节语义）
//!   last_successful_sync_at 仅在 broadcast_clipboard 收到 200 OK 时更新
//! - 返 200 OK

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

use crate::app::state::AppState;
use crate::network::error::NetworkError;
use crate::network::protocol::HeartbeatReq;

/// POST /heartbeat
///
/// ADR-008 5.2 节 + ADR-009 第 3.2 节语义：
/// - 心跳成功 → record_heartbeat_ok（更新 last_heartbeat_at + 清零 consecutive_heartbeat_failures）
/// - **不**更新 last_successful_sync_at（仅广播 200 OK 时写，心跳不是数据同步）
pub async fn handle_heartbeat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<HeartbeatReq>,
) -> Result<StatusCode, NetworkError> {
    // --- 步骤 1：来源鉴权 is_known（ADR-008 MUST-3）---
    if !state.peers.is_known(&req.origin_device_id) {
        let err = NetworkError::NotInPeers;
        err.log();
        return Err(err);
    }

    // --- 步骤 2：banned 双重防线（ADR-008 5.3 节）---
    if state.peers.is_banned(&req.origin_device_id) {
        let err = NetworkError::Banned;
        err.log();
        return Err(err);
    }

    // --- 步骤 3：record_heartbeat_ok（ADR-009 第 3.2 节）---
    // MUST NOT 更新 last_successful_sync_at（ADR-008 5.2 节明确：
    //   "last_successful_sync_at 仅在 broadcast 200 OK 时写，不在心跳成功时写"）
    state.peers.record_heartbeat_ok(&req.origin_device_id);

    tracing::debug!(
        target: "network::heartbeat",
        origin = %req.origin_device_id,
        seq = req.seq,
        "heartbeat ok: last_heartbeat_at updated (last_successful_sync_at NOT updated, ADR-008 5.2)"
    );

    Ok(StatusCode::OK)
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::peer::{PeerRegistry, PeerState, TrustState};
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use zeroize::Zeroizing;

    fn make_peer(id: &str) -> PeerState {
        PeerState {
            device_id: id.to_string(),
            device_name: format!("device-{id}"),
            addr: "127.0.0.1:9999".parse::<SocketAddr>().expect("addr parse"),
            pubkey_b64: "test_pubkey".to_string(),
            aes_key: Zeroizing::new([0u8; 32]),
            last_successful_sync_at: None,
            last_heartbeat_at: None,
            consecutive_heartbeat_failures: 0,
            consecutive_send_failures: 0,
            trust_state: TrustState::Approved,
            last_seen_seq_by_kind: HashMap::new(),
        }
    }

    /// heartbeat_updates_last_heartbeat_not_last_sync：
    /// record_heartbeat_ok 必须更新 last_heartbeat_at，但不更新 last_successful_sync_at
    /// （ADR-008 5.2 节 + ADR-009 第 6.1 节 单测 #10）
    #[test]
    fn heartbeat_updates_last_heartbeat_not_last_sync() {
        let registry = PeerRegistry::new_for_test();
        let peer_id = "heartbeat-test-peer";
        registry.insert(make_peer(peer_id));

        // 初始状态：两个时间戳都应为 None
        {
            let s = registry.get(peer_id).expect("peer must exist");
            assert!(
                s.last_heartbeat_at.is_none(),
                "initial last_heartbeat_at must be None"
            );
            assert!(
                s.last_successful_sync_at.is_none(),
                "initial last_successful_sync_at must be None"
            );
        }

        // 调用 record_heartbeat_ok
        registry.record_heartbeat_ok(peer_id);

        // 验证结果：last_heartbeat_at 已更新，last_successful_sync_at 仍为 None
        let s = registry.get(peer_id).expect("peer must still exist");
        assert!(
            s.last_heartbeat_at.is_some(),
            "record_heartbeat_ok must update last_heartbeat_at (ADR-008 5.2)"
        );
        assert!(
            s.last_successful_sync_at.is_none(),
            "record_heartbeat_ok must NOT update last_successful_sync_at (ADR-008 5.2)"
        );
        assert_eq!(
            s.consecutive_heartbeat_failures, 0,
            "record_heartbeat_ok must reset consecutive_heartbeat_failures to 0"
        );
    }
}
