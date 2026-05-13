//! network/client.rs — 对外广播 / 拨号函数集
//! see specs/clipboard-text-sync.md, specs/group-discovery.md, specs/group-leave-notify.md
//! see decisions/ADR-009-peer-registry.md (第 3.5 节 client_pool 接口契约)
//! see decisions/ADR-011-crypto-traits.md (第 3.3 节 build_aad 调用契约)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-4 remove 原子顺序 / 5.2 节 last_sync 写时机)
//!
//! PR-5 范围：
//! - broadcast_clipboard：加密 + 向所有 Approved peer POST /clipboard + record_send_ok/fail
//! - broadcast_leave：best-effort 向所有 Approved peer POST /peers/leave（1500ms timeout）
//! - dial_handshake：主动向目标 peer 发起握手
//!
//! PR-7 新增（group-discovery AC #2 gossip mesh）：
//! - dial_handshake 成功后：从 resp.peers 中过滤未知 peer，限并发 ≤ 3 发起 gossip 握手
//! - broadcast_announce：向已知 Approved peer 广播新 peer 的 GossipAnnouncePayload

use std::sync::Arc;
use std::time::Duration;

use crate::app::state::AppState;
use crate::crypto::{build_aad, AadKind, AesGcmSealer, Sealer};
use crate::network::protocol::{
    ClipboardReq, GossipAnnouncePayload, HandshakeReq, HandshakeResp, LeaveReq,
};
use crate::peer::{PeerState, TrustState};

/// 握手超时（ADR-010 第 3.2 节 — 握手需在合理时间内完成）
const HANDSHAKE_TIMEOUT_MS: u64 = 5000;

/// leave 广播单个 peer 超时
const LEAVE_PER_PEER_TIMEOUT_MS: u64 = 1500;

/// 广播单次请求超时（文本 / 图片）
const BROADCAST_TIMEOUT_MS: u64 = 3000;

/// gossip 握手并发上限（防止 N=8 cascade 风暴，group-discovery spec 第 5.2 节）
const GOSSIP_MAX_CONCURRENT: usize = 3;

/// gossip 单次握手超时（独立于主握手超时；gossip 路径 fire-and-forget，超时更短）
const GOSSIP_HANDSHAKE_TIMEOUT_MS: u64 = 5000;

/// gossip announce 广播单个 peer 超时（best-effort）
const ANNOUNCE_PER_PEER_TIMEOUT_MS: u64 = 1500;

// ---------------------------------------------------------------------------
// broadcast_clipboard — 向所有 Approved peer 广播剪切板内容
// ---------------------------------------------------------------------------

/// 向所有 Approved peer 广播加密剪切板内容（text 或 image_png）。
///
/// 流程（ADR-011 第 3.3 节调用契约表）：
///   1. snapshot peers（仅 Approved，不向 Banned peer 发送）
///   2. 对每个 peer：取 aes_key → build_aad → encrypt → POST /clipboard
///   3. 200 OK → record_send_ok（更新 last_successful_sync_at，ADR-008 5.2 节）
///   4. 非 200 → record_send_fail
///
/// SECURITY（ADR-009 第 3.2 节 P1 注释）：
/// 从 PeerState 克隆出来的 aes_key 字节不进 tracing fields / 不落盘。
///
/// 注意：本函数不写 OS 剪切板（那是 PR-6 arboard 线程的工作）。
pub async fn broadcast_clipboard(
    state: &AppState,
    kind: AadKind,
    plaintext: Vec<u8>,
    seq: u64,
    my_device_id: &str,
) -> anyhow::Result<()> {
    // snapshot 后立即释放锁（不持锁过 await）
    let peers: Vec<PeerState> = state
        .peers
        .snapshot()
        .into_iter()
        .filter(|p| p.trust_state == TrustState::Approved)
        .collect();

    if peers.is_empty() {
        tracing::debug!(
            target: "network::client",
            "broadcast_clipboard: no approved peers, skip"
        );
        return Ok(());
    }

    let sealer = AesGcmSealer;
    let mut tasks = Vec::with_capacity(peers.len());

    for peer in peers {
        let peer_id = peer.device_id.clone();
        let peer_addr = peer.addr;
        let aes_key: [u8; 32] = *peer.aes_key;

        // build_aad（ADR-011 第 3.3 节）：调用方必须在 encrypt 前调 build_aad
        let aad = build_aad(kind, my_device_id, seq);

        // encrypt（不持锁）
        let encrypt_result = sealer.encrypt(&aes_key, &plaintext, &aad);
        let (nonce_b64, ciphertext_b64) = match encrypt_result {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!(
                    target: "network::client",
                    peer_id = %peer_id,
                    error = %e,
                    "broadcast_clipboard: encrypt failed, skip peer"
                );
                state.peers.record_send_fail(&peer_id);
                continue;
            }
        };

        // [低 nit #1 PR-5b review] 原 `_ => "text"` 会让 Trust/Ban/Leave 等 kind 静默降级，
        // 与 build_aad 的 kind 字面不匹配（对端 decrypt 失败，且日志归因困难）。
        // broadcast_clipboard 只允许 Text / ImagePng；其它 kind 属编程错误。
        let kind_str = match kind {
            AadKind::Text => "text",
            AadKind::ImagePng => "image_png",
            _ => unreachable!(
                "broadcast_clipboard only supports Text / ImagePng; got unexpected AadKind"
            ),
        };

        let req_body = ClipboardReq {
            origin_device_id: my_device_id.to_string(),
            seq,
            kind: kind_str.to_string(),
            nonce_b64,
            ciphertext_b64,
            is_snapshot: false,
        };

        // 取 client（不持锁）
        let client = state.client_pool.get(&peer_id);
        let peers_ref = Arc::clone(&state.peers);
        let peer_id_clone = peer_id.clone();

        tasks.push(tokio::spawn(async move {
            let client = match client {
                Some(c) => c,
                None => {
                    tracing::warn!(
                        target: "network::client",
                        peer_id = %peer_id_clone,
                        "broadcast_clipboard: no client in pool for peer, skip"
                    );
                    peers_ref.record_send_fail(&peer_id_clone);
                    return;
                }
            };

            let url = format!("http://{}:{}/clipboard", peer_addr.ip(), peer_addr.port());
            let result = tokio::time::timeout(
                Duration::from_millis(BROADCAST_TIMEOUT_MS),
                client.post(&url).json(&req_body).send(),
            )
            .await;

            match result {
                Ok(Ok(resp)) if resp.status().is_success() => {
                    // ADR-008 5.2 节：仅广播 200 OK 时更新 last_successful_sync_at
                    peers_ref.record_send_ok(&peer_id_clone);
                    tracing::debug!(
                        target: "network::client",
                        peer_id = %peer_id_clone,
                        "broadcast_clipboard: 200 OK"
                    );
                }
                Ok(Ok(resp)) => {
                    tracing::warn!(
                        target: "network::client",
                        peer_id = %peer_id_clone,
                        status = %resp.status(),
                        "broadcast_clipboard: non-2xx response"
                    );
                    peers_ref.record_send_fail(&peer_id_clone);
                }
                Ok(Err(e)) => {
                    tracing::warn!(
                        target: "network::client",
                        peer_id = %peer_id_clone,
                        error = %e,
                        "broadcast_clipboard: request error"
                    );
                    peers_ref.record_send_fail(&peer_id_clone);
                }
                Err(_timeout) => {
                    tracing::warn!(
                        target: "network::client",
                        peer_id = %peer_id_clone,
                        timeout_ms = BROADCAST_TIMEOUT_MS,
                        "broadcast_clipboard: timeout"
                    );
                    peers_ref.record_send_fail(&peer_id_clone);
                }
            }
        }));
    }

    // 等待所有任务完成（不需要收集结果，结果已在各 task 内记录）
    for task in tasks {
        let _ = task.await;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// broadcast_leave — best-effort 向所有 Approved peer 发 POST /peers/leave
// ---------------------------------------------------------------------------

/// best-effort 向所有 Approved peer 广播 leave（ADR-010 shutdown step 3）。
///
/// SECURITY（ADR-009 第 7.3 节 P3 注释）：
/// 仅向 Approved peer 发送（防 banned peer 知悉本机下线信号）。
/// snapshot 到 broadcast 之间存在 ns 级窗口，期间被 ban 的 peer 仍可能收到 leave，
/// 泄露"本机正在下线"信号；A2/A3 利用该信号窗口 ≤ 1500ms — 低危可接受。
pub async fn broadcast_leave(state: &AppState, my_device_id: &str, seq: u64) {
    // snapshot 后释放锁
    let peers: Vec<PeerState> = state
        .peers
        .snapshot()
        .into_iter()
        .filter(|p| p.trust_state == TrustState::Approved)
        .collect();

    if peers.is_empty() {
        tracing::debug!(target: "network::client", "broadcast_leave: no approved peers");
        return;
    }

    let req_body = LeaveReq {
        origin_device_id: my_device_id.to_string(),
        seq,
    };

    let mut tasks = Vec::with_capacity(peers.len());
    for peer in peers {
        let peer_id = peer.device_id.clone();
        let peer_addr = peer.addr;
        let client = state.client_pool.get(&peer_id);
        let req_body_clone = LeaveReq {
            origin_device_id: req_body.origin_device_id.clone(),
            seq: req_body.seq,
        };

        tasks.push(tokio::spawn(async move {
            let client = match client {
                Some(c) => c,
                None => {
                    tracing::debug!(
                        target: "network::client",
                        peer_id = %peer_id,
                        "broadcast_leave: no client in pool, skip"
                    );
                    return;
                }
            };

            let url = format!("http://{}:{}/peers/leave", peer_addr.ip(), peer_addr.port());
            let result = tokio::time::timeout(
                Duration::from_millis(LEAVE_PER_PEER_TIMEOUT_MS),
                client.post(&url).json(&req_body_clone).send(),
            )
            .await;

            match result {
                Ok(Ok(resp)) => {
                    tracing::debug!(
                        target: "network::client",
                        peer_id = %peer_id,
                        status = %resp.status(),
                        "broadcast_leave: sent"
                    );
                }
                Ok(Err(e)) => {
                    tracing::debug!(
                        target: "network::client",
                        peer_id = %peer_id,
                        error = %e,
                        "broadcast_leave: request error (best-effort, ignore)"
                    );
                }
                Err(_timeout) => {
                    tracing::debug!(
                        target: "network::client",
                        peer_id = %peer_id,
                        "broadcast_leave: timeout (best-effort, ignore)"
                    );
                }
            }
        }));
    }

    // best-effort：等待所有任务，但整体受上层 1500ms timeout 控制（lifecycle step 3）
    for task in tasks {
        let _ = task.await;
    }
}

// ---------------------------------------------------------------------------
// dial_handshake — 主动向目标 peer 发起握手
// ---------------------------------------------------------------------------

/// 主动向目标 peer 发起握手（group-discovery 主动连接路径）。
///
/// 流程（ADR-011 第 3.1 节 KeyExchange trait / ADR-009 第 3.5 节调用顺序）：
///   1. 生成本机临时密钥对（EphemeralSecret）
///   2. POST /handshake → 收对端公钥 + device_id + device_name
///   3. X25519 ECDH → HKDF → AES key（Zeroizing 包装）
///   4. 构造 PeerState → client_pool.insert → registry.insert → registry.approve
///
/// 注意：
/// - client_pool.insert 必须在 registry.insert **之前**（ADR-009 第 3.5 节调用顺序契约）
/// - 返回错误不 panic；caller 决定是否重试（PR-6 heartbeat worker 会重试）
pub async fn dial_handshake(
    target_addr: std::net::SocketAddr,
    state: &AppState,
    my_device_id: &str,
    my_device_name: &str,
    my_listen_port: u16,
) -> anyhow::Result<()> {
    use crate::crypto::{KeyExchange, X25519KeyExchange};
    use crate::peer::sanitize::sanitize_device_name;
    use crate::peer::{PeerState, TrustState};
    use std::collections::HashMap;
    use zeroize::Zeroizing;

    // 生成临时密钥对
    let (my_secret, my_pubkey) = X25519KeyExchange::new_ephemeral();
    let my_pubkey_b64 = X25519KeyExchange::pubkey_to_b64(&my_pubkey);

    let req = HandshakeReq {
        device_id: my_device_id.to_string(),
        device_name: my_device_name.to_string(),
        pubkey_b64: my_pubkey_b64.clone(),
        listen_port: my_listen_port,
    };

    // 发起握手请求（使用默认 Client，不从 pool 取——握手时 peer 还不在 pool 里）
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| anyhow::anyhow!("dial_handshake: build client failed: {e}"))?;

    let url = format!(
        "http://{}:{}/handshake",
        target_addr.ip(),
        target_addr.port()
    );

    let resp = tokio::time::timeout(
        Duration::from_millis(HANDSHAKE_TIMEOUT_MS),
        client.post(&url).json(&req).send(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("dial_handshake: timeout after {HANDSHAKE_TIMEOUT_MS}ms"))?
    .map_err(|e| anyhow::anyhow!("dial_handshake: request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!(
            "dial_handshake: peer returned {}",
            resp.status()
        ));
    }

    let handshake_resp: HandshakeResp = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("dial_handshake: parse resp failed: {e}"))?;

    let peer_id = handshake_resp.device_id.clone();

    // 校验：不允许与自己握手（在 ECDH 之前短路，避免无谓计算）
    if peer_id == my_device_id {
        return Err(anyhow::anyhow!(
            "dial_handshake: peer returned our own device_id, reject"
        ));
    }

    // 校验：banned peer 拒绝 re-handshake（ADR-008 5.3 节 / ADR-009 第 3.5 节）
    // [低 nit #2 PR-5b review] 前移到 derive_aes_key 之前，避免对 banned peer 浪费 ECDH 计算
    // 以及派生密钥短暂存在栈上（已 Zeroizing 包装，但更早短路更好）
    if state.peers.is_banned(&peer_id) {
        return Err(anyhow::anyhow!(
            "dial_handshake: peer {} is banned, skip",
            peer_id
        ));
    }

    // 解析对端公钥
    let their_pubkey = X25519KeyExchange::pubkey_from_b64(&handshake_resp.pubkey_b64)
        .map_err(|e| anyhow::anyhow!("dial_handshake: parse peer pubkey failed: {e}"))?;

    // ECDH → HKDF → AES key
    let raw_key = X25519KeyExchange::derive_aes_key(my_secret, &their_pubkey)
        .map_err(|e| anyhow::anyhow!("dial_handshake: derive_aes_key failed: {e}"))?;
    let aes_key = Zeroizing::new(raw_key);

    let safe_name = sanitize_device_name(&handshake_resp.device_name.unwrap_or_default());

    let peer_state = PeerState {
        device_id: peer_id.clone(),
        device_name: safe_name,
        addr: target_addr,
        pubkey_b64: handshake_resp.pubkey_b64.clone(),
        aes_key,
        last_successful_sync_at: None,
        last_heartbeat_at: None,
        consecutive_heartbeat_failures: 0,
        consecutive_send_failures: 0,
        trust_state: TrustState::Approved,
        last_seen_seq_by_kind: HashMap::new(),
    };

    // ADR-009 第 3.5 节调用顺序：
    //   1. 构造 reqwest::Client
    //   2. client_pool.insert(id, client)  ← 必须在 registry.insert 之前
    //   3. registry.insert(state)
    let pool_client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| anyhow::anyhow!("dial_handshake: build pool client failed: {e}"))?;

    state.client_pool.insert(&peer_id, pool_client);
    state.peers.insert(peer_state);
    state.peers.approve(&peer_id);

    tracing::info!(
        target: "network::client",
        peer_id = %peer_id,
        addr = %target_addr,
        "dial_handshake: handshake complete, peer inserted and approved"
    );

    // PR-7 gossip mesh：从响应 peers 列表中 fire-and-forget 扩展到未知 peer
    // 限并发 ≤ GOSSIP_MAX_CONCURRENT，防 N 大时 cascade 连接风暴。
    // 策略：收集未知 peer stubs → 使用 gossip_dial_stub 而非递归调 dial_handshake，
    //       避免 !Send 嵌套 spawn（dial_handshake 含 rand::thread_rng，该局部变量跨 await 时 !Send）。
    let unknown_peers: Vec<_> = handshake_resp
        .peers
        .into_iter()
        .filter(|stub| {
            // 过滤自己（兜底，服务端不应返回自己）+ 过滤已知 peer
            stub.device_id != my_device_id && !state.peers.is_known(&stub.device_id)
        })
        .take(GOSSIP_MAX_CONCURRENT)
        .collect();

    if !unknown_peers.is_empty() {
        tracing::info!(
            target: "network::client",
            count = unknown_peers.len(),
            "dial_handshake: gossip expanding to {} unknown peer(s) (PR-7)",
            unknown_peers.len()
        );

        let state_arc = Arc::new(state.clone());
        let my_device_id_owned = my_device_id.to_string();
        let my_device_name_owned = my_device_name.to_string();

        for stub in unknown_peers {
            let state_clone = Arc::clone(&state_arc);
            let did = my_device_id_owned.clone();
            let dname = my_device_name_owned.clone();
            let stub_id = stub.device_id.clone();
            let stub_addr = stub.addr;
            let port = my_listen_port;

            // fire-and-forget：每个 gossip 握手独立 spawn
            // gossip_dial_stub 是 dial_handshake 的精简版，明确 Send（避免嵌套 dial_handshake 的 !Send 问题）
            tokio::spawn(gossip_dial_stub(
                stub_addr,
                stub_id,
                did,
                dname,
                port,
                state_clone,
            ));
        }
    }

    // PR-7 gossip announce broadcast：
    // 向本机已知的 Approved peer（除了刚握手的目标 peer）广播新 peer 信息，
    // 触发它们也去 dial 新 peer，完成全组 mesh。
    // seq=0：announce 路径不参与 monotonic seq dedupe（best-effort，不需要保序）
    broadcast_announce(state, &peer_id, target_addr, my_device_id, 0).await;

    Ok(())
}

// ---------------------------------------------------------------------------
// gossip_dial_stub — gossip 场景下的精简握手（PR-7，Send + 不递归 gossip）
// ---------------------------------------------------------------------------

/// gossip 专用握手：向单个 stub addr 发起握手，不再触发二次 gossip（防 cascade）。
///
/// 与 dial_handshake 的关键区别：
/// 1. 不递归调 gossip_dial_stub / dial_handshake（防 cascade + 避免无限 mesh 扩张）
/// 2. 明确设计为 `Send + 'static`（可直接 tokio::spawn）
/// 3. 超时统一使用 GOSSIP_HANDSHAKE_TIMEOUT_MS（独立于主握手超时）
///
/// 调用者：dial_handshake 的 gossip 扩展循环（fire-and-forget spawn）。
async fn gossip_dial_stub(
    target_addr: std::net::SocketAddr,
    expected_peer_id: String, // 期望的 device_id（用于日志 + banned 检查）
    my_device_id: String,
    my_device_name: String,
    my_listen_port: u16,
    state: Arc<AppState>,
) {
    use crate::crypto::{KeyExchange, X25519KeyExchange};
    use crate::peer::sanitize::sanitize_device_name;
    use crate::peer::{PeerState, TrustState};
    use std::collections::HashMap;
    use zeroize::Zeroizing;

    // 再次检查（异步 spawn 可能有 race，stub 在 spawn 后可能已被 insert）
    if state.peers.is_known(&expected_peer_id) || state.peers.is_banned(&expected_peer_id) {
        tracing::debug!(
            target: "network::client",
            peer_id = %expected_peer_id,
            "gossip_dial_stub: peer already known or banned at spawn time, skip"
        );
        return;
    }

    // 生成临时密钥对（在 await 前完成，不跨 await 持有 !Send 临时值）
    let (my_secret, my_pubkey) = X25519KeyExchange::new_ephemeral();
    let my_pubkey_b64 = X25519KeyExchange::pubkey_to_b64(&my_pubkey);

    let req = HandshakeReq {
        device_id: my_device_id.clone(),
        device_name: my_device_name.clone(),
        pubkey_b64: my_pubkey_b64,
        listen_port: my_listen_port,
    };

    // 构建 client（no_proxy，避免代理拦截 LAN 请求）
    let client = match reqwest::Client::builder().no_proxy().build() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                target: "network::client",
                peer_id = %expected_peer_id,
                error = %e,
                "gossip_dial_stub: build client failed"
            );
            return;
        }
    };

    let url = format!(
        "http://{}:{}/handshake",
        target_addr.ip(),
        target_addr.port()
    );

    // POST /handshake（带超时）
    let send_result = tokio::time::timeout(
        Duration::from_millis(GOSSIP_HANDSHAKE_TIMEOUT_MS),
        client.post(&url).json(&req).send(),
    )
    .await;

    let raw_resp = match send_result {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            tracing::warn!(
                target: "network::client",
                peer_id = %expected_peer_id,
                error = %e,
                "gossip_dial_stub: request failed"
            );
            return;
        }
        Err(_) => {
            tracing::warn!(
                target: "network::client",
                peer_id = %expected_peer_id,
                timeout_ms = GOSSIP_HANDSHAKE_TIMEOUT_MS,
                "gossip_dial_stub: timeout"
            );
            return;
        }
    };

    if !raw_resp.status().is_success() {
        tracing::warn!(
            target: "network::client",
            peer_id = %expected_peer_id,
            status = %raw_resp.status(),
            "gossip_dial_stub: non-2xx response"
        );
        return;
    }

    let handshake_resp: HandshakeResp = match raw_resp.json().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                target: "network::client",
                peer_id = %expected_peer_id,
                error = %e,
                "gossip_dial_stub: parse response failed"
            );
            return;
        }
    };

    let peer_id = handshake_resp.device_id.clone();

    // 自连 + banned 检查
    if peer_id == my_device_id || state.peers.is_banned(&peer_id) {
        tracing::warn!(
            target: "network::client",
            peer_id = %peer_id,
            "gossip_dial_stub: self-dial or banned peer, reject"
        );
        return;
    }

    // dedupe：如果在握手期间已被另一路径 insert，跳过
    if state.peers.is_known(&peer_id) {
        tracing::debug!(
            target: "network::client",
            peer_id = %peer_id,
            "gossip_dial_stub: peer was inserted concurrently, dedupe skip"
        );
        return;
    }

    // 解析对端公钥 + ECDH → HKDF → AES key
    let their_pubkey = match X25519KeyExchange::pubkey_from_b64(&handshake_resp.pubkey_b64) {
        Ok(pk) => pk,
        Err(e) => {
            tracing::warn!(
                target: "network::client",
                peer_id = %peer_id,
                error = %e,
                "gossip_dial_stub: invalid pubkey"
            );
            return;
        }
    };

    let raw_key = match X25519KeyExchange::derive_aes_key(my_secret, &their_pubkey) {
        Ok(k) => k,
        Err(e) => {
            tracing::warn!(
                target: "network::client",
                peer_id = %peer_id,
                error = %e,
                "gossip_dial_stub: key derivation failed"
            );
            return;
        }
    };
    let aes_key = Zeroizing::new(raw_key);

    let safe_name = sanitize_device_name(&handshake_resp.device_name.unwrap_or_default());

    let peer_state = PeerState {
        device_id: peer_id.clone(),
        device_name: safe_name,
        addr: target_addr,
        pubkey_b64: handshake_resp.pubkey_b64,
        aes_key,
        last_successful_sync_at: None,
        last_heartbeat_at: None,
        consecutive_heartbeat_failures: 0,
        consecutive_send_failures: 0,
        trust_state: TrustState::Approved,
        last_seen_seq_by_kind: HashMap::new(),
    };

    // ADR-009 第 3.5 节调用顺序
    let pool_client = match reqwest::Client::builder().no_proxy().build() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                target: "network::client",
                peer_id = %peer_id,
                error = %e,
                "gossip_dial_stub: build pool client failed"
            );
            return;
        }
    };

    state.client_pool.insert(&peer_id, pool_client);
    state.peers.insert(peer_state);
    state.peers.approve(&peer_id);

    tracing::info!(
        target: "network::client",
        peer_id = %peer_id,
        addr = %target_addr,
        "gossip_dial_stub: gossip handshake complete"
    );
    // 注意：gossip_dial_stub 不再触发二次 gossip/announce，防 cascade。
}

// ---------------------------------------------------------------------------
// broadcast_announce — 向已知 Approved peer 广播新 peer 的 gossip announce（PR-7）
// ---------------------------------------------------------------------------

/// 向本机所有已 Approved peer（除了 new_peer_id 自身）广播 GossipAnnouncePayload。
///
/// 用于 dial_handshake 成功后，告知其他 peer 有新成员加入，
/// 实现 N≥3 全组 mesh（group-discovery AC #2 gossip）。
///
/// 失败不重试（best-effort）；接收端收到后若 peer 已知则 dedupe 200 不 dial。
///
/// SECURITY（ADR-008 MUST-3）：
/// broadcast_announce 只向已 approved peer 发送（不向 banned peer 泄露新 peer 信息）。
pub async fn broadcast_announce(
    state: &AppState,
    new_peer_id: &str,
    new_peer_addr: std::net::SocketAddr,
    my_device_id: &str,
    seq: u64,
) {
    // snapshot 后立即释放锁（不持锁过 await）
    let peers: Vec<PeerState> = state
        .peers
        .snapshot()
        .into_iter()
        .filter(|p| {
            // 只向 Approved peer 发；不发给 new_peer 自己（它已握手，不需要 announce 自己）
            p.trust_state == TrustState::Approved && p.device_id != new_peer_id
        })
        .collect();

    if peers.is_empty() {
        tracing::debug!(
            target: "network::client",
            "broadcast_announce: no other approved peers to notify"
        );
        return;
    }

    let payload = GossipAnnouncePayload {
        device_id: new_peer_id.to_string(),
        addr: new_peer_addr,
        origin_device_id: my_device_id.to_string(),
        seq,
    };

    let mut tasks = Vec::with_capacity(peers.len());
    for peer in peers {
        let peer_id = peer.device_id.clone();
        let peer_addr = peer.addr;
        let client = state.client_pool.get(&peer_id);
        let payload_clone = GossipAnnouncePayload {
            device_id: payload.device_id.clone(),
            addr: payload.addr,
            origin_device_id: payload.origin_device_id.clone(),
            seq: payload.seq,
        };

        tasks.push(tokio::spawn(async move {
            let client = match client {
                Some(c) => c,
                None => {
                    tracing::debug!(
                        target: "network::client",
                        peer_id = %peer_id,
                        "broadcast_announce: no client in pool, skip"
                    );
                    return;
                }
            };

            let url = format!(
                "http://{}:{}/peers/announce",
                peer_addr.ip(),
                peer_addr.port()
            );
            let result = tokio::time::timeout(
                Duration::from_millis(ANNOUNCE_PER_PEER_TIMEOUT_MS),
                client.post(&url).json(&payload_clone).send(),
            )
            .await;

            match result {
                Ok(Ok(resp)) => {
                    tracing::debug!(
                        target: "network::client",
                        peer_id = %peer_id,
                        status = %resp.status(),
                        "broadcast_announce: sent"
                    );
                }
                Ok(Err(e)) => {
                    tracing::debug!(
                        target: "network::client",
                        peer_id = %peer_id,
                        error = %e,
                        "broadcast_announce: request error (best-effort, ignore)"
                    );
                }
                Err(_timeout) => {
                    tracing::debug!(
                        target: "network::client",
                        peer_id = %peer_id,
                        "broadcast_announce: timeout (best-effort, ignore)"
                    );
                }
            }
        }));
    }

    for task in tasks {
        let _ = task.await;
    }
}

// ---------------------------------------------------------------------------
// ping — heartbeat worker 专用，向单个 peer POST /heartbeat
// ---------------------------------------------------------------------------

/// 专用心跳超时（heartbeat worker 要求快速失败）
const PING_TIMEOUT_MS: u64 = 2000;

/// 向单个 peer POST /heartbeat（heartbeat worker 主循环调用）。
///
/// 使用 client_pool 中已有的 per-peer Client（不 lazy-add）。
/// client_pool miss → 返 Err（peer 可能已被移除，调用方按失败处理）。
///
/// 200 OK → Ok(())
/// 非 200 / timeout / 连接错误 → Err（调用方 increment_heartbeat_failure）
///
/// SECURITY（ADR-009 第 3.2 节 P1）：
/// peer.aes_key 不进 tracing fields；仅记 peer_id + status。
pub async fn ping(
    state: &AppState,
    peer_id: &str,
    peer_addr: std::net::SocketAddr,
) -> anyhow::Result<()> {
    use crate::network::protocol::HeartbeatReq;

    let client = state.client_pool.get(peer_id).ok_or_else(|| {
        anyhow::anyhow!("ping: no client in pool for peer {peer_id} (peer may have been removed)")
    })?;

    let url = format!("http://{}:{}/heartbeat", peer_addr.ip(), peer_addr.port());

    // seq=0：心跳探活不参与 monotonic seq dedupe（接收端 record_heartbeat_ok 不写 seq）
    let req_body = HeartbeatReq {
        origin_device_id: state.my_device_id.clone(),
        seq: 0,
    };

    let result = tokio::time::timeout(
        Duration::from_millis(PING_TIMEOUT_MS),
        client.post(&url).json(&req_body).send(),
    )
    .await;

    match result {
        Ok(Ok(resp)) if resp.status().is_success() => {
            tracing::debug!(
                target: "network::client::ping",
                peer_id = %peer_id,
                addr = %peer_addr,
                "ping: 200 OK"
            );
            Ok(())
        }
        Ok(Ok(resp)) => {
            let status = resp.status();
            tracing::warn!(
                target: "network::client::ping",
                peer_id = %peer_id,
                addr = %peer_addr,
                status = %status,
                "ping: non-2xx response"
            );
            Err(anyhow::anyhow!("ping: peer {peer_id} returned {status}"))
        }
        Ok(Err(e)) => {
            tracing::warn!(
                target: "network::client::ping",
                peer_id = %peer_id,
                addr = %peer_addr,
                error = %e,
                "ping: request error"
            );
            Err(anyhow::anyhow!("ping: request failed for {peer_id}: {e}"))
        }
        Err(_timeout) => {
            tracing::warn!(
                target: "network::client::ping",
                peer_id = %peer_id,
                addr = %peer_addr,
                timeout_ms = PING_TIMEOUT_MS,
                "ping: timeout"
            );
            Err(anyhow::anyhow!(
                "ping: timeout for peer {peer_id} after {PING_TIMEOUT_MS}ms"
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::AppState;

    /// broadcast_clipboard 在无 Approved peer 时不 panic，静默返 Ok
    #[tokio::test]
    async fn broadcast_clipboard_no_approved_peers_returns_ok() {
        let state = AppState::new();
        let result =
            broadcast_clipboard(&state, AadKind::Text, b"hello".to_vec(), 1, "my-device").await;
        assert!(result.is_ok(), "no peers → should return Ok without panic");
    }

    /// broadcast_leave 在无 Approved peer 时不 panic
    #[tokio::test]
    async fn broadcast_leave_no_approved_peers_silent() {
        let state = AppState::new();
        // 不 panic，不返回错误
        broadcast_leave(&state, "my-device", 1).await;
    }

    /// broadcast_clipboard 有 Approved peer 但 client_pool miss 时降级 send_fail
    #[tokio::test]
    async fn broadcast_clipboard_pool_miss_records_send_fail() {
        use crate::peer::{PeerState, TrustState};
        use std::collections::HashMap;
        use std::net::SocketAddr;
        use zeroize::Zeroizing;

        let state = AppState::new();
        let peer_id = "test-peer-001";

        // 插入 Approved peer，但 client_pool 不 insert（模拟 miss）
        let peer = PeerState {
            device_id: peer_id.to_string(),
            device_name: "Test Peer".to_string(),
            addr: "127.0.0.1:19999".parse::<SocketAddr>().expect("addr parse"),
            pubkey_b64: "test_pubkey".to_string(),
            aes_key: Zeroizing::new([0x42u8; 32]),
            last_successful_sync_at: None,
            last_heartbeat_at: None,
            consecutive_heartbeat_failures: 0,
            consecutive_send_failures: 0,
            trust_state: TrustState::Approved,
            last_seen_seq_by_kind: HashMap::new(),
        };
        state.peers.insert(peer);
        state.peers.approve(peer_id);

        // broadcast：client_pool miss → record_send_fail
        let result = broadcast_clipboard(
            &state,
            AadKind::Text,
            b"test payload".to_vec(),
            1,
            "my-device",
        )
        .await;
        assert!(result.is_ok(), "client_pool miss should not return Err");

        // send_failures 应增加（pool miss 路径记录失败）
        let peer_state = state.peers.get(peer_id).expect("peer still in registry");
        assert_eq!(
            peer_state.consecutive_send_failures, 1,
            "client_pool miss should record send_fail"
        );
    }
}
