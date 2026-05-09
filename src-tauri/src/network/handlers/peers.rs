//! /peers/* handlers（trust / ban / approval/{forward,decide,dismiss} + /peers/announce）
//! see specs/group-discovery.md (第 3 节 peer announce)
//! see specs/group-trust-gossip.md (第 3 节 trust/ban 互斥与 gossip)
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-3 / MUST-8)
//! see decisions/ADR-009-peer-registry.md (第 3.3 节 trust 互斥状态机 / 第 3.5 节 client_pool)
//!
//! PR-5 业务逻辑：
//! - handle_peers_announce：sanitize + PeerRegistry.insert（已知 peer）
//! - handle_trust：鉴权 + seq dedupe + PeerRegistry.approve(subject)（trust gossip）
//! - handle_ban：鉴权 + seq dedupe + PeerRegistry.ban(subject)（ban gossip，互斥 trust）
//! - handle_approval_*：留 PR-6+（依赖前端弹框）→ 占位 503
//!
//! 端点列表（本文件）：
//!   POST /peers/announce
//!   POST /peers/trust
//!   POST /peers/ban
//!   POST /peers/approval/forward（占位 503）
//!   POST /peers/approval/decide（占位 503）
//!   POST /peers/approval/dismiss（占位 503）

use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::Json;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use zeroize::Zeroizing;

use crate::app::state::AppState;
use crate::crypto::AadKind;
use crate::network::error::NetworkError;
use crate::network::protocol::{
    AnnounceReq, ApprovalDecideReq, ApprovalDismissReq, ApprovalForwardReq, TrustReq,
};
use crate::peer::sanitize::sanitize_device_name;
use crate::peer::{PeerState, TrustState};

// ---------------------------------------------------------------------------
// POST /peers/announce
// ---------------------------------------------------------------------------

/// POST /peers/announce — 新 peer 宣告自身（group-discovery）。
///
/// PR-5 实现：接收宣告 → sanitize name → 若 peer 已知则更新地址；若未知则 insert Pending 状态。
/// 注意：真正的 approve 需要用户审批弹框（PR-6+）；此处仅记录 peer 信息。
///
/// 鉴权：/peers/announce 无前置鉴权（任何 LAN 设备可发宣告）。
/// DoS 防护：announce 本身不包含密钥协商，不影响 PeerRegistry.aes_key；
///            peer 处于 Pending 状态，无法收到加密 clipboard 广播（is_approved 为 false）。
pub async fn handle_peers_announce(
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<AnnounceReq>,
) -> Result<StatusCode, NetworkError> {
    // 校验：banned peer 宣告直接拒绝（ADR-008 5.3 节防 zombie）
    if state.peers.is_banned(&req.device_id) {
        let err = NetworkError::Banned;
        err.log();
        return Err(err);
    }

    // sanitize device_name（ADR-008 MUST-8）
    let sanitized_name = sanitize_device_name(&req.device_name);

    // 对端地址 = remote_addr.ip() + req.listen_port
    let peer_addr = SocketAddr::new(remote_addr.ip(), req.listen_port);
    let peer_id = req.device_id.clone();

    if state.peers.is_known(&peer_id) {
        // 已知 peer re-announce（如重启后）：addr 可能变化；
        // 真实处理需 re-handshake 更新密钥；此处仅 log
        tracing::debug!(
            target: "network::peers",
            peer_id = %peer_id,
            addr = %peer_addr,
            "announce: known peer re-announced (re-handshake needed to refresh key)"
        );
    } else {
        // 未知 peer：insert Pending 状态（等待用户审批）
        // aes_key 为全零占位（Pending 状态不用于解密；真实 key 在握手后写入）
        let peer_state = PeerState {
            device_id: peer_id.clone(),
            device_name: sanitized_name,
            addr: peer_addr,
            pubkey_b64: req.pubkey_b64.clone(),
            aes_key: Zeroizing::new([0u8; 32]), // 占位；握手完成前不使用
            last_successful_sync_at: None,
            last_heartbeat_at: None,
            consecutive_heartbeat_failures: 0,
            consecutive_send_failures: 0,
            trust_state: TrustState::Pending,
            last_seen_seq_by_kind: HashMap::new(),
        };
        state.peers.insert(peer_state);

        tracing::info!(
            target: "network::peers",
            peer_id = %peer_id,
            addr = %peer_addr,
            "announce: new peer inserted (Pending, awaiting approval)"
            // TODO PR-6：emit approval-pending 事件到前端弹框
        );
    }

    Ok(StatusCode::OK)
}

// ---------------------------------------------------------------------------
// POST /peers/trust
// ---------------------------------------------------------------------------

/// POST /peers/trust — 信任传播（group-trust-gossip）。
///
/// PR-5 实现：鉴权 + seq dedupe + PeerRegistry.approve(subject_device_id)。
/// ADR-009 第 3.3 节 trust 互斥状态机：approve 覆盖 ban（trust_overrides_ban 语义）。
///
/// 注意：approve 仅影响 subject（被信任的第三方 peer），不是 origin（发送方）。
pub async fn handle_trust(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TrustReq>,
) -> Result<StatusCode, NetworkError> {
    // --- 鉴权（ADR-008 MUST-3）---
    if !state.peers.is_known(&req.origin_device_id) {
        let err = NetworkError::NotInPeers;
        err.log();
        return Err(err);
    }

    // --- banned 双重防线（ADR-008 5.3 节）---
    if state.peers.is_banned(&req.origin_device_id) {
        let err = NetworkError::Banned;
        err.log();
        return Err(err);
    }

    // --- seq 去重（ADR-009 第 3.2 节 invariant 5）---
    if !state
        .peers
        .seen_seq_and_update(&req.origin_device_id, AadKind::Trust, req.seq)
    {
        return Ok(StatusCode::OK);
    }

    // --- trust gossip：approve subject（ADR-009 第 3.3 节 trust 互斥状态机）---
    // approve 内部按 approved → banned 锁顺序（ADR-009 第 3.3.1 节 P4 补丁）：
    //   approved.insert + banned.remove + inner[id].trust_state = Approved
    state.peers.approve(&req.subject_device_id);

    tracing::info!(
        target: "network::peers",
        origin = %req.origin_device_id,
        subject = %req.subject_device_id,
        seq = req.seq,
        "trust gossip: subject approved"
        // TODO PR-7：emit status-updated 到前端
    );

    Ok(StatusCode::OK)
}

// ---------------------------------------------------------------------------
// POST /peers/ban
// ---------------------------------------------------------------------------

/// POST /peers/ban — 封禁传播（group-trust-gossip）。
///
/// PR-5 实现：鉴权 + seq dedupe + PeerRegistry.ban(subject_device_id)。
/// ADR-009 第 3.3 节 trust 互斥状态机：ban 覆盖 trust；若 was_peer=true 则同时踢出 inner。
///
/// 注意：ban 是不可逆的（从 inner 移除后需 re-handshake 才能重新加入）。
/// caller 应在 ban 返回后 emit status-updated（此处 TODO PR-7）。
pub async fn handle_ban(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TrustReq>,
) -> Result<StatusCode, NetworkError> {
    // --- 鉴权（ADR-008 MUST-3）---
    if !state.peers.is_known(&req.origin_device_id) {
        let err = NetworkError::NotInPeers;
        err.log();
        return Err(err);
    }

    // --- banned 双重防线（ADR-008 5.3 节）---
    if state.peers.is_banned(&req.origin_device_id) {
        let err = NetworkError::Banned;
        err.log();
        return Err(err);
    }

    // --- seq 去重（ADR-009 第 3.2 节 invariant 5）---
    if !state
        .peers
        .seen_seq_and_update(&req.origin_device_id, AadKind::Ban, req.seq)
    {
        return Ok(StatusCode::OK);
    }

    // --- ban gossip：ban subject（ADR-009 第 3.3 节 trust 互斥状态机）---
    // ban 内部按 approved → banned 锁顺序（ADR-009 第 3.3.1 节 P4 补丁）：
    //   approved.remove + banned.insert + (was_peer ? inner.remove + client_pool 标记)
    state.peers.ban(&req.subject_device_id);

    tracing::info!(
        target: "network::peers",
        origin = %req.origin_device_id,
        subject = %req.subject_device_id,
        seq = req.seq,
        "ban gossip: subject banned"
        // TODO PR-7：emit status-updated 到前端（ADR-009 第 4.3 节副作用 #3 caller 必须手动 emit）
    );

    Ok(StatusCode::OK)
}

// ---------------------------------------------------------------------------
// POST /peers/approval/forward（占位 503 — 依赖前端弹框 PR-6+）
// ---------------------------------------------------------------------------

/// POST /peers/approval/forward — 审批请求转发（group-approval）。
///
/// 依赖前端弹框 UI；留 PR-6+。
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
        "approval/forward received (PR-6+ placeholder)"
    );

    // 依赖前端弹框；留 PR-6+
    Ok(StatusCode::SERVICE_UNAVAILABLE)
}

// ---------------------------------------------------------------------------
// POST /peers/approval/decide（占位 503）
// ---------------------------------------------------------------------------

/// POST /peers/approval/decide — 审批决策回流（group-approval）。
///
/// 依赖前端弹框 UI；留 PR-6+。
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
        "approval/decide received (PR-6+ placeholder)"
    );

    Ok(StatusCode::SERVICE_UNAVAILABLE)
}

// ---------------------------------------------------------------------------
// POST /peers/approval/dismiss（占位 503）
// ---------------------------------------------------------------------------

/// POST /peers/approval/dismiss — 审批取消（group-approval）。
///
/// 依赖前端弹框 UI；留 PR-6+。
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
        "approval/dismiss received (PR-6+ placeholder)"
    );

    Ok(StatusCode::SERVICE_UNAVAILABLE)
}

// ---------------------------------------------------------------------------
// 单元测试（trust / ban 互斥 + announce 插入）
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
            trust_state: TrustState::Pending,
            last_seen_seq_by_kind: HashMap::new(),
        }
    }

    /// trust_ban_mutual_exclusion_via_handler：
    /// handle_trust 调 approve → approved && !banned；
    /// handle_ban 调 ban → banned && !approved（互斥不变式，ADR-009 第 3.3 节）
    #[test]
    fn trust_ban_mutual_exclusion_via_registry() {
        let registry = PeerRegistry::new_for_test();
        let origin_id = "origin-peer";
        let subject_id = "subject-peer";

        registry.insert(make_peer(origin_id));
        registry.approve(origin_id);
        registry.insert(make_peer(subject_id));

        // trust gossip → approve subject
        registry.approve(subject_id);
        assert!(
            registry.is_approved(subject_id),
            "after trust: subject must be approved"
        );
        assert!(
            !registry.is_banned(subject_id),
            "after trust: subject must not be banned"
        );

        // ban gossip → ban subject（覆盖 trust）
        registry.ban(subject_id);
        assert!(
            !registry.is_approved(subject_id),
            "after ban: subject must not be approved"
        );
        assert!(
            registry.is_banned(subject_id),
            "after ban: subject must be banned"
        );

        // trust 覆盖 ban（approve 后 ban 集合清空）
        let registry2 = PeerRegistry::new_for_test();
        registry2.insert(make_peer("sub2"));
        registry2.ban("sub2");
        registry2.approve("sub2");
        assert!(
            registry2.is_approved("sub2"),
            "trust must override ban (approved ∩ banned = ∅ invariant)"
        );
        assert!(
            !registry2.is_banned("sub2"),
            "after trust overrides ban: banned must be empty"
        );
    }

    /// announce_inserts_unknown_peer：新 peer 宣告后应在 registry 中（Pending 状态）
    #[test]
    fn announce_inserts_unknown_peer_into_registry() {
        let registry = PeerRegistry::new_for_test();
        let peer_id = "announce-new-peer";

        // 模拟 handle_peers_announce 核心逻辑
        let peer_state = PeerState {
            device_id: peer_id.to_string(),
            device_name: "New Device".to_string(),
            addr: "192.168.1.50:5858"
                .parse::<SocketAddr>()
                .expect("addr parse"),
            pubkey_b64: "pubkey_b64_value".to_string(),
            aes_key: Zeroizing::new([0u8; 32]),
            last_successful_sync_at: None,
            last_heartbeat_at: None,
            consecutive_heartbeat_failures: 0,
            consecutive_send_failures: 0,
            trust_state: TrustState::Pending,
            last_seen_seq_by_kind: HashMap::new(),
        };
        registry.insert(peer_state);

        assert!(
            registry.is_known(peer_id),
            "announced peer must be in registry"
        );
        assert!(
            !registry.is_approved(peer_id),
            "announced peer must NOT be approved yet (waiting for user approval)"
        );
        assert!(
            !registry.is_banned(peer_id),
            "announced peer must NOT be banned"
        );
        let s = registry.get(peer_id).expect("peer must be gettable");
        assert_eq!(
            s.trust_state,
            TrustState::Pending,
            "announced peer must have Pending trust_state"
        );
    }
}
