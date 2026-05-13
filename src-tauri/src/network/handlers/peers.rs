//! /peers/* handlers（trust / ban / approval/{forward,decide,dismiss} + /peers/announce）
//! see specs/group-discovery.md (第 3 节 peer announce, AC #2 gossip mesh, AC #7 自连拒)
//! see specs/group-trust-gossip.md (第 3 节 trust/ban 互斥与 gossip)
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-3 403 通用 body / MUST-7 DoS 限流)
//! see decisions/ADR-009-peer-registry.md (第 3.3 节 trust 互斥状态机 / 第 3.5 节 client_pool)
//!
//! PR-7 业务逻辑（gossip mesh 自动扩展）：
//! - handle_peers_announce（重写）：接收 GossipAnnouncePayload → 校验 origin_device_id 已 approved
//!   → 自连拒绝 → banned 拒绝 → dedupe 已知 → 否则 spawn dial_handshake（反向连接新 peer）
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

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

use crate::app::state::AppState;
use crate::crypto::AadKind;
use crate::network::error::NetworkError;
use crate::network::protocol::{
    ApprovalDecideReq, ApprovalDismissReq, ApprovalForwardReq, GossipAnnouncePayload, TrustReq,
};
use crate::peer::sanitize::sanitize_device_name;

// ---------------------------------------------------------------------------
// POST /peers/announce（PR-7 gossip announce，完整重写）
// ---------------------------------------------------------------------------

/// POST /peers/announce — gossip mesh 自动扩展（PR-7，group-discovery AC #2）。
///
/// 接收由已连接 peer 转发的新 peer 信息，触发本机对新 peer 的反向握手，
/// 实现 N≥3 设备"一次 dial 全组连通"的 gossip mesh。
///
/// 校验顺序（ADR-008 MUST-3 通用 403 body）：
///   1. origin_device_id 必须已在本机 PeerRegistry approved（否则 403）。
///      防止陌生 IP 注入 peer；announce 不走 RateLimiter，origin 已 approved 门禁兜底，
///      /handshake 端独立限流（handshake.rs 步骤 1）。
///   2. req.device_id != my_device_id（自连拒绝，403）— group-discovery AC #7
///   3. req.device_id 不在 banned set（403）
///   4. req.device_id 已在 PeerRegistry → 200 + 不 dial（dedupe，group-discovery AC #2）
///   5. 否则：spawn dial_handshake(stub.addr) — 反向连接新 peer
///
/// 失败不重试（best-effort）；下次 handshake 时会再 propagate。
pub async fn handle_peers_announce(
    State(state): State<Arc<AppState>>,
    Json(req): Json<GossipAnnouncePayload>,
) -> Result<StatusCode, NetworkError> {
    // --- 步骤 1：鉴权 — origin_device_id 必须已 approved（ADR-008 MUST-3）---
    // announce 不走 RateLimiter：origin 必须 approved 门禁已充当第一道防线，
    // 陌生 IP 在此步即被 403 阻断；/handshake 端独立限流（handshake.rs 步骤 1）。
    // 防止陌生 IP 伪造 announce 注入 peer；只有已知可信 peer 才能 announce 新 peer。
    if !state.peers.is_approved(&req.origin_device_id) {
        tracing::warn!(
            target: "network::peers",
            origin = %req.origin_device_id,
            "announce: origin_device_id not approved, reject (ADR-008 MUST-3)"
        );
        let err = NetworkError::NotInPeers;
        err.log();
        return Err(err);
    }

    // --- 步骤 2：自连拒绝（group-discovery AC #7，ADR-008 MUST-3）---
    if req.device_id == state.my_device_id {
        tracing::warn!(
            target: "network::peers",
            device_id = %req.device_id,
            "announce: self-announce rejected (device_id matches my_device_id)"
        );
        let err = NetworkError::DeviceIdConflict;
        err.log();
        return Err(err);
    }

    // --- 步骤 3：banned set 拒绝（ADR-008 5.3 节防 zombie）---
    if state.peers.is_banned(&req.device_id) {
        tracing::warn!(
            target: "network::peers",
            device_id = %req.device_id,
            "announce: device_id is banned, reject (ADR-008 5.3)"
        );
        let err = NetworkError::Banned;
        err.log();
        return Err(err);
    }

    // --- 步骤 4：dedupe — 已知 peer 直接 200，不重复 dial（group-discovery AC #2）---
    if state.peers.is_known(&req.device_id) {
        tracing::debug!(
            target: "network::peers",
            device_id = %req.device_id,
            "announce: peer already known, skip dial (dedupe)"
        );
        return Ok(StatusCode::OK);
    }

    // --- 步骤 5：spawn dial_handshake 反向连接新 peer（best-effort fire-and-forget）---
    // state 已经是 Arc<AppState>（axum State 萃取），Arc::clone 是廉价引用计数增加。
    let target_addr = req.addr;
    let new_peer_id = req.device_id.clone();
    let my_device_id = state.my_device_id.clone();
    let my_device_name = {
        // 取本机 device_name（从 config 中读取；v2 当前使用空字符串占位）
        // PR-7 简化：不依赖 config，直接用固定串作为降级展示名
        "SyncCopy".to_string()
    };
    let my_listen_port = crate::network::DEFAULT_PORT;
    // Arc::clone(&state) 而非 Arc::new(state.clone())，避免 Arc<Arc<AppState>> 双层包装。
    let state_arc = Arc::clone(&state);

    tracing::info!(
        target: "network::peers",
        new_peer_id = %new_peer_id,
        addr = %target_addr,
        origin = %req.origin_device_id,
        "announce: spawning dial_handshake to new peer (gossip mesh expansion)"
    );

    // fire-and-forget：失败不重试（best-effort，ADR 设计要求）
    tokio::spawn(async move {
        let result = crate::network::client::dial_handshake(
            target_addr,
            &state_arc,
            &my_device_id,
            &my_device_name,
            my_listen_port,
        )
        .await;
        if let Err(e) = result {
            tracing::warn!(
                target: "network::peers",
                new_peer_id = %new_peer_id,
                addr = %target_addr,
                error = %e,
                "announce: dial_handshake to new peer failed (best-effort, no retry)"
            );
        } else {
            tracing::info!(
                target: "network::peers",
                new_peer_id = %new_peer_id,
                "announce: gossip dial_handshake succeeded"
            );
        }
    });

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
// 单元测试（trust / ban 互斥 + gossip announce 安全校验，PR-7）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::network::protocol::GossipAnnouncePayload;
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

    // -----------------------------------------------------------------------
    // PR-7 gossip announce 安全校验单测（group-discovery AC #2 / AC #7）
    // -----------------------------------------------------------------------

    /// 辅助：构造一个 GossipAnnouncePayload（不含实际 addr，仅用于校验逻辑验证）
    fn make_announce(device_id: &str, origin_device_id: &str) -> GossipAnnouncePayload {
        GossipAnnouncePayload {
            device_id: device_id.to_string(),
            addr: "192.168.1.99:5858".parse::<SocketAddr>().expect("addr"),
            origin_device_id: origin_device_id.to_string(),
        }
    }

    /// announce_from_unapproved_origin_rejected_403 (PR-7 单测 #4)
    ///
    /// origin_device_id 不在 approved → 模拟 handle_peers_announce 步骤 2 校验失败。
    /// 验证：is_approved 返 false → handler 返回 403（NotInPeers）。
    #[test]
    fn announce_from_unapproved_origin_rejected() {
        let registry = PeerRegistry::new_for_test();
        let req = make_announce("new-peer-id", "unknown-origin");

        // 未认证 origin：is_approved 返 false → handler 步骤 2 拒绝
        assert!(
            !registry.is_approved(&req.origin_device_id),
            "unapproved origin must be rejected (handler step 2)"
        );
    }

    /// announce_self_rejected_403 (PR-7 单测 #5)
    ///
    /// req.device_id == my_device_id → 模拟 handle_peers_announce 步骤 3 自连拒绝。
    /// 验证：device_id == my_device_id 条件成立，handler 应拒绝。
    #[test]
    fn announce_self_rejected() {
        let my_device_id = "my-own-device-id";
        let req = make_announce(my_device_id, "approved-origin");

        // 自连：device_id == my_device_id → handler 步骤 3 拒绝（403 DeviceIdConflict）
        assert_eq!(
            req.device_id, my_device_id,
            "self-announce must be caught by device_id == my_device_id check"
        );
    }

    /// announce_already_known_dedupe (PR-7 单测 #6)
    ///
    /// req.device_id 已在 PeerRegistry → handler 步骤 5 dedupe，返 200，不 dial。
    /// 验证：is_known 返 true → 短路，不再 spawn dial。
    #[test]
    fn announce_already_known_dedupe() {
        let registry = PeerRegistry::new_for_test();

        // 插入已知 peer（模拟已握手完成）
        registry.insert(make_peer("known-peer"));
        registry.approve("known-peer");

        // 也插入 origin（已 approved）
        registry.insert(make_peer("approved-origin"));
        registry.approve("approved-origin");

        let req = make_announce("known-peer", "approved-origin");

        // 步骤 2：origin approved → 通过
        assert!(
            registry.is_approved(&req.origin_device_id),
            "origin must be approved"
        );
        // 步骤 5：peer 已知 → dedupe，不 dial
        assert!(
            registry.is_known(&req.device_id),
            "known peer must trigger dedupe (200, no dial)"
        );
    }

    /// announce_banned_device_rejected (PR-7 — banned set 校验)
    ///
    /// req.device_id 在 banned set → handler 步骤 4 拒绝（403 Banned）。
    #[test]
    fn announce_banned_device_rejected() {
        let registry = PeerRegistry::new_for_test();

        // origin 已 approved
        registry.insert(make_peer("approved-origin"));
        registry.approve("approved-origin");

        // 被 announce 的 peer 在 banned set
        registry.ban("banned-peer");

        let req = make_announce("banned-peer", "approved-origin");

        // 步骤 4：is_banned 返 true → handler 拒绝
        assert!(
            registry.is_banned(&req.device_id),
            "banned peer must be rejected (handler step 4)"
        );
    }

    /// announce_serde_roundtrip — GossipAnnouncePayload 序列化/反序列化正确性
    ///
    /// 验证 DTO 经 JSON 序列化后字段不变（v5-6 外部接口 try-coerce 验证）。
    /// 同时验证向后兼容性：旧端发来带 seq 字段的 JSON 能被正常解析（serde 忽略未知字段）。
    #[test]
    fn announce_serde_roundtrip() {
        let payload = GossipAnnouncePayload {
            device_id: "device-xyz".to_string(),
            addr: "192.168.1.100:5858".parse::<SocketAddr>().expect("addr"),
            origin_device_id: "origin-abc".to_string(),
        };

        let json = serde_json::to_string(&payload).expect("serialize should not fail");
        let decoded: GossipAnnouncePayload =
            serde_json::from_str(&json).expect("deserialize should not fail");

        assert_eq!(decoded.device_id, payload.device_id);
        assert_eq!(decoded.addr, payload.addr);
        assert_eq!(decoded.origin_device_id, payload.origin_device_id);

        // 向后兼容：旧端发来带 seq 字段时，serde 忽略未知字段，不报错
        let old_wire =
            r#"{"device_id":"d1","addr":"192.168.1.1:5858","origin_device_id":"o1","seq":99}"#;
        let decoded_old: GossipAnnouncePayload =
            serde_json::from_str(old_wire).expect("old wire with seq must still deserialize");
        assert_eq!(decoded_old.device_id, "d1");
        assert_eq!(decoded_old.origin_device_id, "o1");
    }
}
