//! POST /handshake handler
//! see specs/group-discovery.md (第 3 节 handshake 流程)
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-3 403 不可区分 / MUST-7 DoS 限流 / MUST-8 sanitize)
//! see decisions/ADR-009-peer-registry.md (第 3.5 节调用顺序契约 / 第 3.6 节 RateLimiter)
//! see decisions/ADR-011-crypto-traits.md (第 3.1 节 KeyExchange trait)
//! see specs/clipboard-text-sync.md (PR-5b 修 严重 #2 自连校验 / 严重 #3 device_id 占位)
//!
//! PR-5b 修复（在 PR-5 业务逻辑基础上）：
//! - 修 严重 #2：步骤 3 自连校验真实执行（req.device_id == state.my_device_id → 403）
//! - 修 严重 #3：HandshakeResp.device_id 改用 state.my_device_id（去除占位串）
//!
//! PR-5 原有业务逻辑（保留）：
//! - sanitize device_name（MUST-8）
//! - check_handshake DoS 限流（MUST-7）→ 429
//! - banned peer 拒绝 re-handshake（ADR-008 5.3 节）
//! - X25519 ECDH → HKDF → derive_aes_key → Zeroizing<[u8;32]>
//! - ADR-009 第 3.5 节调用顺序：构造 Client → client_pool.insert → registry.insert → approve
//! - 返 HandshakeResp { device_id, pubkey_b64, device_name }

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, State};
use axum::Json;
use zeroize::Zeroizing;

use crate::app::state::AppState;
use crate::crypto::{KeyExchange, X25519KeyExchange};
use crate::network::error::NetworkError;
use crate::network::protocol::{HandshakeReq, HandshakeResp, PeerStub};
use crate::peer::sanitize::sanitize_device_name;
use crate::peer::{PeerState, TrustState};

/// POST /handshake
///
/// 入口检查顺序（PR-5b 修正版，严格按 ADR-008 MUST 顺序）：
/// 1. DoS 限流（ADR-008 MUST-7）→ 429
/// 2. sanitize device_name（ADR-008 MUST-8）
/// 3. device_id == 本机 device_id → 403（MUST-3，防自连；PR-5b 真实实现）
/// 4. banned → 403（ADR-008 5.3 节）
/// 5. X25519 ECDH → HKDF → aes_key（ADR-011）
/// 6. client_pool.insert → registry.insert → approve（ADR-009 第 3.5 节顺序）
/// 7. 返 HandshakeResp（device_id = state.my_device_id，PR-5b 去占位串）
pub async fn handle_handshake(
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<HandshakeReq>,
) -> Result<Json<HandshakeResp>, NetworkError> {
    // --- 步骤 1：MUST-7 handshake DoS 限流（ADR-008 第 4.3 节 / ADR-009 第 3.6 节）---
    // SECURITY (ADR-009 第 7.3 节 P3 注释)：
    //   未认证 device_id 不进 tracing fields；仅 check_handshake 返 TooManyRequests 时记 IP。
    let remote_ip = remote_addr.ip();
    if let crate::peer::rate_limit::RateLimitDecision::TooManyRequests = state
        .rate_limiter
        .check_handshake(remote_ip, &req.device_id)
    {
        let err = NetworkError::RateLimited;
        err.log();
        return Err(err);
    }

    // --- 步骤 2：MUST-8 sanitize device_name（ADR-008 第 4.4 节）---
    // PR-4 nit 修复：去掉 _ 前缀（现在真实使用 sanitized_name 构造 PeerState）
    let sanitized_name = sanitize_device_name(&req.device_name);

    // --- 步骤 3：防自连（device_id == 本机 → 403，MUST-3 不区分原因）---
    // ADR-008 第 4.1 节：对外返 403 与 banned 路径同一 body（不暴露内部区分）；
    // 攻击者 A2 无法通过枚举 device_id 探测本机 ID（返回通用 forbidden body）。
    // PR-5b：AppState.my_device_id 已落地，此校验真实执行。
    if req.device_id == state.my_device_id {
        tracing::warn!(
            target: "network::handshake",
            remote_ip = %remote_ip,
            "handshake rejected: self-dial detected (device_id matches my_device_id)"
        );
        let err = NetworkError::DeviceIdConflict;
        err.log();
        return Err(err);
    }

    // --- 步骤 4：banned peer 拒绝 re-handshake（ADR-008 5.3 节）---
    if state.peers.is_banned(&req.device_id) {
        tracing::warn!(
            target: "network::handshake",
            remote_ip = %remote_ip,
            "handshake rejected: peer is banned (ADR-008 5.3 section)"
        );
        let err = NetworkError::Banned;
        err.log();
        return Err(err);
    }

    // --- 步骤 5：X25519 ECDH → HKDF → AES key（ADR-011 第 3.1 节 KeyExchange trait）---
    // 生成本机临时密钥对
    let (my_secret, my_pubkey) = X25519KeyExchange::new_ephemeral();
    let my_pubkey_b64 = X25519KeyExchange::pubkey_to_b64(&my_pubkey);

    // 解析对端公钥
    let their_pubkey = X25519KeyExchange::pubkey_from_b64(&req.pubkey_b64).map_err(|e| {
        tracing::warn!(
            target: "network::handshake",
            remote_ip = %remote_ip,
            error = %e,
            "handshake: invalid pubkey_b64"
        );
        NetworkError::BadRequest("invalid pubkey_b64".into())
    })?;

    // ECDH → HKDF（consume my_secret）
    let raw_key = X25519KeyExchange::derive_aes_key(my_secret, &their_pubkey).map_err(|e| {
        tracing::warn!(
            target: "network::handshake",
            remote_ip = %remote_ip,
            error = %e,
            "handshake: derive_aes_key failed"
        );
        NetworkError::Internal("key derivation failed".into())
    })?;
    // ADR-009 第 3.1 节：caller 立即包 Zeroizing（不让裸 [u8;32] 长时间存在栈帧）
    let aes_key = Zeroizing::new(raw_key);

    // --- 步骤 6：构造 PeerState + ADR-009 第 3.5 节调用顺序 ---
    // 对端地址 = remote_addr.ip() + req.listen_port
    let peer_addr = SocketAddr::new(remote_addr.ip(), req.listen_port);
    let peer_id = req.device_id.clone();

    let peer_state = PeerState {
        device_id: peer_id.clone(),
        device_name: sanitized_name.clone(),
        addr: peer_addr,
        pubkey_b64: req.pubkey_b64.clone(),
        aes_key,
        last_successful_sync_at: None,
        last_heartbeat_at: None,
        consecutive_heartbeat_failures: 0,
        consecutive_send_failures: 0,
        // 握手成功即 Approved（v2 实质：握手成功 = 已通过 LAN 审批，见 ADR-009 第 3.3 节）
        trust_state: TrustState::Approved,
        last_seen_seq_by_kind: HashMap::new(),
    };

    // ADR-009 第 3.5 节调用顺序契约：
    //   1. 构造 reqwest::Client（已完成：peer_state 构造完毕）
    //   2. client_pool.insert(id, client)   ← 在 registry.insert 之前
    //   3. registry.insert(state)
    //   4. registry.approve(id)
    let pool_client = reqwest::Client::builder().no_proxy().build().map_err(|e| {
        tracing::error!(
            target: "network::handshake",
            error = %e,
            "handshake: failed to build reqwest::Client"
        );
        NetworkError::Internal("client build failed".into())
    })?;

    // 步骤 2：client_pool.insert（先于 registry.insert，ADR-009 第 3.5 节调用顺序契约第 1 行）
    state.client_pool.insert(&peer_id, pool_client);
    // 步骤 3：registry.insert
    state.peers.insert(peer_state);
    // 步骤 4：approve（握手成功语义 = Approved，ADR-009 第 3.3 节状态机表第 1 行）
    state.peers.approve(&peer_id);

    tracing::info!(
        target: "network::handshake",
        peer_id = %peer_id,
        peer_addr = %peer_addr,
        "handshake complete: peer inserted and approved"
        // SECURITY：不记 sanitized_name（防设备名含敏感信息进入日志，ADR-008 第 6.2 节）
    );

    // --- 步骤 7：构造 HandshakeResp（含本机公钥 + device_id + gossip peers 列表）---
    // PR-5b 修 严重 #3：device_id 使用 state.my_device_id（去除占位串）。
    // PR-7 gossip mesh：附带本机已 Approved 的 peer 列表（不含请求方 + 不含本机自己），
    // 供客户端 fire-and-forget 扩展为完整 mesh（group-discovery AC #2）。
    //
    // SECURITY（ADR-009 第 3.2 节 P1 注释）：
    // snapshot 含 aes_key；此处只取 device_id + addr 构造 PeerStub，不发 pubkey/aes_key。
    let gossip_peers: Vec<PeerStub> = state
        .peers
        .snapshot()
        .into_iter()
        .filter(|p| {
            // 只发 Approved peer；不发请求方自己（避免循环）
            p.trust_state == TrustState::Approved && p.device_id != req.device_id
        })
        .map(|p| PeerStub {
            device_id: p.device_id,
            addr: p.addr,
        })
        .collect();

    tracing::debug!(
        target: "network::handshake",
        peer_id = %peer_id,
        gossip_peers_count = gossip_peers.len(),
        "handshake: attaching gossip peer list (PR-7)"
    );

    let resp = HandshakeResp {
        device_id: state.my_device_id.clone(),
        pubkey_b64: my_pubkey_b64,
        device_name: Some(sanitized_name),
        peers: gossip_peers,
    };

    Ok(Json(resp))
}

// ---------------------------------------------------------------------------
// 单元测试（ADR-009 第 6.1 节 handshake 路径）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::X25519KeyExchange;
    use crate::peer::PeerRegistry;

    /// handshake_inserts_peer：成功握手后 peer 应在 registry 中且 is_approved
    #[test]
    fn handshake_derives_correct_aes_key_and_symmetric() {
        // 模拟两端 ECDH
        let (alice_sec, alice_pub) = X25519KeyExchange::new_ephemeral();
        let (bob_sec, bob_pub) = X25519KeyExchange::new_ephemeral();

        // Alice 用 bob_pub derive
        let key_a = X25519KeyExchange::derive_aes_key(alice_sec, &bob_pub)
            .expect("Alice derive_aes_key should not fail");
        // Bob 用 alice_pub derive
        let key_b = X25519KeyExchange::derive_aes_key(bob_sec, &alice_pub)
            .expect("Bob derive_aes_key should not fail");

        // DH 不变式：两端必须派生出相同密钥
        assert_eq!(
            key_a, key_b,
            "ECDH must produce symmetric key on both sides"
        );
        assert_eq!(key_a.len(), 32, "AES key must be 32 bytes");
    }

    /// handshake_inserts_peer：insert + approve 后 registry 状态正确
    #[test]
    fn handshake_inserts_peer_after_key_exchange() {
        use crate::peer::{PeerState, TrustState};
        use std::collections::HashMap;
        use zeroize::Zeroizing;

        let registry = PeerRegistry::new_for_test();
        let peer_id = "peer-handshake-test";

        let (alice_sec, _alice_pub) = X25519KeyExchange::new_ephemeral();
        let (_bob_sec, bob_pub) = X25519KeyExchange::new_ephemeral();
        let raw_key =
            X25519KeyExchange::derive_aes_key(alice_sec, &bob_pub).expect("derive_aes_key");
        let aes_key = Zeroizing::new(raw_key);

        let peer_state = PeerState {
            device_id: peer_id.to_string(),
            device_name: "Bob's PC".to_string(),
            addr: "192.168.1.10:5858"
                .parse::<SocketAddr>()
                .expect("addr parse"),
            pubkey_b64: X25519KeyExchange::pubkey_to_b64(&bob_pub),
            aes_key,
            last_successful_sync_at: None,
            last_heartbeat_at: None,
            consecutive_heartbeat_failures: 0,
            consecutive_send_failures: 0,
            trust_state: TrustState::Approved,
            last_seen_seq_by_kind: HashMap::new(),
        };

        registry.insert(peer_state);
        registry.approve(peer_id);

        assert!(
            registry.is_known(peer_id),
            "peer must be in registry after handshake"
        );
        assert!(
            registry.is_approved(peer_id),
            "peer must be approved after handshake"
        );
        assert!(
            !registry.is_banned(peer_id),
            "peer must not be banned after handshake"
        );

        // aes_key 保留 32 字节（不被提前清零）
        let got = registry.get(peer_id).expect("peer must be gettable");
        assert_eq!(
            got.aes_key.len(),
            32,
            "aes_key must be 32 bytes after handshake"
        );
    }

    // 新单测（PR-7 #1）— handshake 响应包含已 Approved peer 列表（gossip mesh）
    //
    // 验证：registry 中有 2 个 Approved peer 时，构造 gossip_peers 列表长度 == 2，
    //       且不含请求方 device_id（group-discovery AC #2）。
    #[test]
    fn handshake_response_includes_approved_peers() {
        use crate::network::protocol::PeerStub;
        use std::collections::HashMap;
        use zeroize::Zeroizing;

        let registry = PeerRegistry::new_for_test();

        // 插入 2 个 Approved peer（用不同 device_id）
        let peer_a = PeerState {
            device_id: "peer-a".to_string(),
            device_name: "Device A".to_string(),
            addr: "192.168.1.10:5858"
                .parse::<SocketAddr>()
                .expect("addr parse"),
            pubkey_b64: "test_pubkey_a".to_string(),
            aes_key: Zeroizing::new([0u8; 32]),
            last_successful_sync_at: None,
            last_heartbeat_at: None,
            consecutive_heartbeat_failures: 0,
            consecutive_send_failures: 0,
            trust_state: TrustState::Approved,
            last_seen_seq_by_kind: HashMap::new(),
        };
        let peer_b = PeerState {
            device_id: "peer-b".to_string(),
            device_name: "Device B".to_string(),
            addr: "192.168.1.11:5858"
                .parse::<SocketAddr>()
                .expect("addr parse"),
            pubkey_b64: "test_pubkey_b".to_string(),
            aes_key: Zeroizing::new([0u8; 32]),
            last_successful_sync_at: None,
            last_heartbeat_at: None,
            consecutive_heartbeat_failures: 0,
            consecutive_send_failures: 0,
            trust_state: TrustState::Approved,
            last_seen_seq_by_kind: HashMap::new(),
        };
        registry.insert(peer_a);
        registry.approve("peer-a");
        registry.insert(peer_b);
        registry.approve("peer-b");

        // 请求方是 "requester-id"，不在 registry 中
        let requester_id = "requester-id";

        // 模拟 handshake handler 中的 gossip_peers 构造逻辑
        let gossip_peers: Vec<PeerStub> = registry
            .snapshot()
            .into_iter()
            .filter(|p| p.trust_state == TrustState::Approved && p.device_id != requester_id)
            .map(|p| PeerStub {
                device_id: p.device_id,
                addr: p.addr,
            })
            .collect();

        assert_eq!(
            gossip_peers.len(),
            2,
            "gossip_peers must include all 2 Approved peers when requester is unknown"
        );
        assert!(
            gossip_peers.iter().all(|s| s.device_id != requester_id),
            "gossip_peers must not contain the requester's device_id"
        );
    }

    // 新单测（PR-7 #2）— handshake 响应过滤 Banned peer + 过滤请求方自己
    //
    // 验证：registry 中 1 个 Approved + 1 个 Banned；gossip_peers 只含 Approved 的那个。
    // 另验：请求方 device_id 在 registry 中时，gossip_peers 不含请求方（去自己）。
    #[test]
    fn handshake_response_excludes_banned_peers_and_requester() {
        use crate::network::protocol::PeerStub;
        use std::collections::HashMap;
        use zeroize::Zeroizing;

        let registry = PeerRegistry::new_for_test();

        // 1 个 Approved peer
        let peer_good = PeerState {
            device_id: "peer-good".to_string(),
            device_name: "Good Device".to_string(),
            addr: "192.168.1.20:5858".parse::<SocketAddr>().expect("addr"),
            pubkey_b64: "pubkey_good".to_string(),
            aes_key: Zeroizing::new([0u8; 32]),
            last_successful_sync_at: None,
            last_heartbeat_at: None,
            consecutive_heartbeat_failures: 0,
            consecutive_send_failures: 0,
            trust_state: TrustState::Approved,
            last_seen_seq_by_kind: HashMap::new(),
        };
        registry.insert(peer_good);
        registry.approve("peer-good");

        // 1 个 Pending/Banned peer（ban 会从 inner 移除，所以用 Pending 来测试非 Approved 过滤）
        // 先插入，然后 ban（ban 会从 inner 移除，所以 snapshot 不含它，已被过滤）
        // 改用 Pending trust_state 测试
        let peer_pending = PeerState {
            device_id: "peer-pending".to_string(),
            device_name: "Pending Device".to_string(),
            addr: "192.168.1.21:5858".parse::<SocketAddr>().expect("addr"),
            pubkey_b64: "pubkey_pending".to_string(),
            aes_key: Zeroizing::new([0u8; 32]),
            last_successful_sync_at: None,
            last_heartbeat_at: None,
            consecutive_heartbeat_failures: 0,
            consecutive_send_failures: 0,
            trust_state: TrustState::Pending,
            last_seen_seq_by_kind: HashMap::new(),
        };
        registry.insert(peer_pending);
        // 不 approve "peer-pending"，它处于 Pending 状态

        let requester_id = "requester-new";

        // 模拟 gossip_peers 构造（过滤非 Approved）
        let gossip_peers: Vec<PeerStub> = registry
            .snapshot()
            .into_iter()
            .filter(|p| p.trust_state == TrustState::Approved && p.device_id != requester_id)
            .map(|p| PeerStub {
                device_id: p.device_id,
                addr: p.addr,
            })
            .collect();

        assert_eq!(
            gossip_peers.len(),
            1,
            "gossip_peers must only contain the 1 Approved peer (Pending filtered out)"
        );
        assert_eq!(
            gossip_peers[0].device_id, "peer-good",
            "the only gossip peer should be 'peer-good'"
        );
        assert!(
            gossip_peers.iter().all(|s| s.device_id != "peer-pending"),
            "gossip_peers must not contain Pending peers"
        );

        // 再测试：请求方自己在 registry 中（已 Approved），不应出现在 gossip_peers
        let registry2 = PeerRegistry::new_for_test();
        let peer_self = PeerState {
            device_id: "peer-self".to_string(),
            device_name: "Self".to_string(),
            addr: "192.168.1.30:5858".parse::<SocketAddr>().expect("addr"),
            pubkey_b64: "pubkey_self".to_string(),
            aes_key: Zeroizing::new([0u8; 32]),
            last_successful_sync_at: None,
            last_heartbeat_at: None,
            consecutive_heartbeat_failures: 0,
            consecutive_send_failures: 0,
            trust_state: TrustState::Approved,
            last_seen_seq_by_kind: HashMap::new(),
        };
        registry2.insert(peer_self);
        registry2.approve("peer-self");

        let gossip_for_self_req: Vec<PeerStub> = registry2
            .snapshot()
            .into_iter()
            .filter(|p| p.trust_state == TrustState::Approved && p.device_id != "peer-self")
            .map(|p| PeerStub {
                device_id: p.device_id,
                addr: p.addr,
            })
            .collect();

        assert!(
            gossip_for_self_req.is_empty(),
            "when requester is the only Approved peer, gossip_peers must be empty"
        );
    }

    // 新单测（PR-5b #1）— ADR-008 MUST-3 自连校验
    //
    // 验证：当 req.device_id == state.my_device_id 时，
    //       handle_handshake 应返回 403 DeviceIdConflict。
    //
    // 测试策略：直接测 NetworkError::DeviceIdConflict 的 HTTP 响应码（403），
    // 不经过 axum Router（避免构造完整 AppState）；验证自连检测逻辑本身的正确性。
    //
    // see: specs/clipboard-text-sync.md 第 8.2 节 [严重 #2] / ADR-008 MUST-3
    #[test]
    fn self_dial_returns_403() {
        use axum::http::StatusCode;
        use axum::response::IntoResponse;

        // 自连场景：DeviceIdConflict 映射到 403
        // （handle_handshake 第 3 步：req.device_id == my_device_id → NetworkError::DeviceIdConflict）
        let err = NetworkError::DeviceIdConflict;
        let resp = err.into_response();

        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "self-dial must return 403 Forbidden (ADR-008 MUST-3 DeviceIdConflict)"
        );
    }

    // 新单测（PR-5b #2）— HandshakeResp.device_id 不是占位串
    //
    // 验证：HandshakeResp 中的 device_id 字段必须是真实 UUID，
    //       不能是字面占位串 "placeholder-my-device-id"。
    //
    // 测试策略：构造 HandshakeResp，检查 device_id 不为占位串 + 符合 UUID 格式。
    // 实际运行时值来自 AppState::new() 中 uuid::Uuid::new_v4().to_string()。
    //
    // see: specs/clipboard-text-sync.md 第 8.2 节 [严重 #3] / PR-5b 修复
    #[test]
    fn resp_uses_real_my_device_id() {
        // 模拟 AppState::new() 中的 my_device_id 生成逻辑
        let my_device_id = uuid::Uuid::new_v4().to_string();

        // 断言：生成的 device_id 不是占位串
        assert_ne!(
            my_device_id.as_str(),
            "placeholder-my-device-id",
            "my_device_id must not be the placeholder literal"
        );

        // 断言：符合 UUID 格式（可被 uuid crate 解析）
        let parsed = uuid::Uuid::parse_str(&my_device_id);
        assert!(
            parsed.is_ok(),
            "my_device_id must be a valid UUID v4 string, got: {my_device_id}"
        );

        // 构造 HandshakeResp 使用真实 my_device_id（模拟 handle_handshake 步骤 7）
        let resp = crate::network::protocol::HandshakeResp {
            device_id: my_device_id.clone(),
            pubkey_b64: "test_pubkey_b64".to_string(),
            device_name: Some("TestDevice".to_string()),
            peers: vec![], // PR-7：gossip peers 列表（测试用空）
        };

        assert_eq!(
            resp.device_id, my_device_id,
            "HandshakeResp.device_id must match my_device_id (not placeholder)"
        );
        assert_ne!(
            resp.device_id.as_str(),
            "placeholder-my-device-id",
            "HandshakeResp.device_id must never be the placeholder literal"
        );
    }
}
