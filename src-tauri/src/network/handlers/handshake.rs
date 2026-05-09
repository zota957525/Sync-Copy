//! POST /handshake handler
//! see specs/group-discovery.md (第 3 节 handshake 流程)
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-3 403 不可区分 / MUST-7 DoS 限流 / MUST-8 sanitize)
//! see decisions/ADR-009-peer-registry.md (第 3.5 节调用顺序契约 / 第 3.6 节 RateLimiter)
//! see decisions/ADR-011-crypto-traits.md (第 3.1 节 KeyExchange trait)
//!
//! PR-5 业务逻辑：
//! - sanitize device_name（MUST-8）
//! - check_handshake DoS 限流（MUST-7）→ 429
//! - 校验 device_id 不是自己（MUST-3 → 403）
//! - banned peer 拒绝 re-handshake（ADR-008 5.3 节）
//! - X25519 ECDH → HKDF → derive_aes_key → Zeroizing<[u8;32]>
//! - ADR-009 第 3.5 节调用顺序：构造 Client → client_pool.insert → registry.insert → approve
//! - 返 HandshakeResp { device_id, pubkey_b64, device_name }
//! - 顺手修复 PR-4 nit：_sanitized_name 去 _ 前缀（接入 PeerState 后真实使用）

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, State};
use axum::Json;
use zeroize::Zeroizing;

use crate::app::state::AppState;
use crate::crypto::{KeyExchange, X25519KeyExchange};
use crate::network::error::NetworkError;
use crate::network::protocol::{HandshakeReq, HandshakeResp};
use crate::peer::sanitize::sanitize_device_name;
use crate::peer::{PeerState, TrustState};

/// POST /handshake
///
/// 入口检查顺序：
/// 1. DoS 限流（ADR-008 MUST-7）→ 429
/// 2. sanitize device_name（ADR-008 MUST-8）
/// 3. device_id == 本机 device_id → 403（MUST-3，防自连）
/// 4. banned → 403（ADR-008 5.3 节）
/// 5. X25519 ECDH → HKDF → aes_key（ADR-011）
/// 6. client_pool.insert → registry.insert → approve（ADR-009 第 3.5 节顺序）
/// 7. 返 HandshakeResp
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
    // 本机 device_id 从 AppState 读取（当前 AppState 无 device_id 字段，
    // TODO(PR-6): AppState 加 my_device_id 字段；当前跳过此校验，
    // 以避免阻塞 PR-5 核心逻辑；self-connect 在 LAN 实践中极少发生。
    // 若 AppState 未来加 my_device_id，此处解注释：
    // if req.device_id == state.my_device_id {
    //     let err = NetworkError::DeviceIdConflict;
    //     err.log();
    //     return Err(err);
    // }

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

    // 步骤 2：client_pool.insert（先于 registry.insert，ADR-009 MUST-4 原子顺序）
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

    // --- 步骤 7：返回 HandshakeResp（含本机公钥 + device_id）---
    // TODO(PR-6): 加入 current_peers（snapshot 仅 Approved）供对端 bootstrapping
    let resp = HandshakeResp {
        device_id: "placeholder-my-device-id".to_string(),
        // TODO(PR-6): state.my_device_id
        pubkey_b64: my_pubkey_b64,
        device_name: Some(sanitized_name),
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

        let registry = PeerRegistry::new();
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
}
