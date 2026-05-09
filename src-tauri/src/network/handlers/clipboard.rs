//! POST /clipboard handler
//! see specs/clipboard-text-sync.md (第 3 节 + 第 4 节 AC)
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-3 / MUST-8)
//! see decisions/ADR-011-crypto-traits.md (第 3.3 节 build_aad 调用契约)
//! see decisions/ADR-009-peer-registry.md (第 3.2 节 invariant 5 seen_seq_and_update 第一行)
//!
//! PR-5 业务逻辑：
//! - is_known + !is_banned 双重鉴权（MUST-3 / ADR-008 5.3 节）
//! - seen_seq_and_update → 重放 200 静默丢（必须在 handler 第一行，ADR-009 invariant 5）
//! - build_aad(kind, origin, seq) → AesGcmSealer::decrypt（ADR-011 第 3.3 节调用契约表）
//! - 解密成功 → 通过 AppState.clipboard_apply_tx Option<Sender> 发到待应用 channel
//!   (TODO PR-6：真接 arboard 线程；本 PR 占位 None + tracing::info)
//! - 解密失败 → 422（NetworkError::DecryptFailed → "unprocessable"）

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

use crate::app::state::AppState;
use crate::crypto::{build_aad, AadKind, AesGcmSealer, Sealer};
use crate::network::error::NetworkError;
use crate::network::protocol::ClipboardReq;
use crate::peer::sanitize::sanitize_log_field;

/// POST /clipboard
///
/// 入口检查顺序（ADR-009 第 3.2 节 invariant 5：seen_seq_and_update 必须第一行）：
/// 1. is_known 校验 → NotInPeers 403（MUST-3）
/// 2. is_banned 校验 → Banned 403（ADR-008 5.3 节双重防线）
/// 3. seen_seq_and_update → 重放 200 静默丢（replay 短路，ADR-009 第 3.2 节 invariant 5）
/// 4. 取 peer aes_key（clone，不持锁过 await）
/// 5. build_aad(kind, origin_device_id, seq)（ADR-011 第 3.3 节）
/// 6. AesGcmSealer::decrypt(key, nonce_b64, ct_b64, aad) → 失败 422
/// 7. 发到 clipboard_apply_tx（TODO PR-6 接 arboard；当前 None 占位 + tracing::info）
/// 8. 返 200 OK
pub async fn handle_clipboard(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ClipboardReq>,
) -> Result<StatusCode, NetworkError> {
    // --- 步骤 1：来源鉴权 is_known（ADR-008 MUST-3）---
    if !state.peers.is_known(&req.origin_device_id) {
        let err = NetworkError::NotInPeers;
        err.log();
        return Err(err);
    }

    // --- 步骤 2：banned 双重防线（ADR-008 5.3 节 — 防 zombie peer）---
    if state.peers.is_banned(&req.origin_device_id) {
        let err = NetworkError::Banned;
        err.log();
        return Err(err);
    }

    // --- 步骤 3：seq 去重（ADR-009 第 3.2 节 invariant 5 — 必须在解密之前）---
    let kind = if req.kind == "image_png" {
        AadKind::ImagePng
    } else {
        AadKind::Text
    };
    if !state
        .peers
        .seen_seq_and_update(&req.origin_device_id, kind, req.seq)
    {
        // 重放：静默 200（不暴露"已见过"信息）
        tracing::debug!(
            target: "network::clipboard",
            origin = %req.origin_device_id,
            seq = req.seq,
            "clipboard seq replay, silently dropped"
        );
        return Ok(StatusCode::OK);
    }

    // --- 步骤 4：取 aes_key（clone + 立即释放读锁，不持锁过 await）---
    // SECURITY（ADR-009 第 3.2 节 P1 注释）：
    //   克隆的 aes_key 字节不进 tracing fields / 不落盘。
    let aes_key = {
        let peer_state = match state.peers.get(&req.origin_device_id) {
            Some(p) => p,
            None => {
                // is_known 通过后 peer 不在 inner 的罕见 race（ban 在 is_known 之后）
                let err = NetworkError::NotInPeers;
                err.log();
                return Err(err);
            }
        };
        // 拷贝 32 字节，立即离开作用域（读锁已在 .get() 返回时释放）
        *peer_state.aes_key
    };

    // --- 步骤 5：build_aad（ADR-011 第 3.3 节调用契约）---
    // 接收方用 req.origin_device_id 重建 aad（与发送方一致）
    let aad = build_aad(kind, &req.origin_device_id, req.seq);

    // --- 步骤 6：AesGcmSealer::decrypt（ADR-011 第 3.1 节 Sealer trait）---
    let sealer = AesGcmSealer;
    let plaintext = sealer
        .decrypt(&aes_key, &req.nonce_b64, &req.ciphertext_b64, &aad)
        .map_err(|e| {
            // SECURITY：decrypt 失败统一返 422（不区分 key/aad/tamper，ADR-008 MUST-3）
            tracing::warn!(
                target: "network::clipboard",
                origin = %req.origin_device_id,
                seq = req.seq,
                kind = %req.kind,
                error = %e,
                "clipboard decrypt failed → 422"
            );
            NetworkError::DecryptFailed
        })?;

    // --- 步骤 7：派发到待应用 channel（TODO PR-6 真接 arboard 线程）---
    // ADR-005（future）clipboard_apply_tx：Option<mpsc::Sender<ApplyClipboardEvt>>
    // 本 PR 仅 tracing::info 占位；PR-6 起 arboard 线程时填入真实 tx
    //
    // SECURITY（ADR-011 第 3.5 节）：
    //   plaintext 是剪切板明文，敏感性等同于 OS 剪切板内容；不进 tracing fields。
    //   在本作用域结束时 drop（Rust ownership，ADR-011 第 3.5 节"caller 路径短即 drop"）
    let plaintext_len = plaintext.len();

    // TODO PR-6：
    //   if let Some(tx) = &state.clipboard_apply_tx {
    //       let _ = tx.send(ApplyClipboardEvt { kind, data: plaintext }).await;
    //   }
    //
    // 当前 AppState 有 clipboard_apply_tx: Option<mpsc::Sender<ApplyClipboardEvt>> 字段（None 占位）
    // PR-6 起 arboard 线程后填入真实 Sender；此处仅 log 通知已就绪
    tracing::info!(
        target: "network::clipboard",
        origin = %sanitize_log_field(&req.origin_device_id),
        seq = req.seq,
        kind = %req.kind,
        plaintext_len,
        "clipboard decrypted ok (TODO PR-6: send to arboard apply channel)"
    );

    // 明文在此作用域结束时 drop（Rust ownership 自动清理）
    drop(plaintext);

    Ok(StatusCode::OK)
}

// ---------------------------------------------------------------------------
// 单元测试（clipboard 解密路径）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::crypto::{build_aad, AadKind, AesGcmSealer, Sealer};

    /// clipboard_decrypt_roundtrip：加密后解密应还原 plaintext
    #[test]
    fn clipboard_decrypt_roundtrip() {
        let sealer = AesGcmSealer;
        let key = [0x42u8; 32];
        let plaintext = b"clipboard test content";
        let aad = build_aad(AadKind::Text, "device-src", 42);

        let (nonce_b64, ct_b64) = sealer.encrypt(&key, plaintext, &aad).expect("encrypt");

        let recovered = sealer
            .decrypt(&key, &nonce_b64, &ct_b64, &aad)
            .expect("decrypt");

        assert_eq!(
            recovered.as_slice(),
            plaintext,
            "roundtrip must recover original plaintext"
        );
    }

    /// clipboard_decrypt_aad_mismatch_fails：AAD 不匹配（不同 origin）→ DecryptFailed
    #[test]
    fn clipboard_decrypt_aad_mismatch_fails() {
        use crate::crypto::SealError;

        let sealer = AesGcmSealer;
        let key = [0x42u8; 32];
        let plaintext = b"secret data";

        // 加密用 device-A 的 aad
        let aad_a = build_aad(AadKind::Text, "device-A", 1);
        let (nonce_b64, ct_b64) = sealer.encrypt(&key, plaintext, &aad_a).expect("encrypt");

        // 解密用 device-B 的 aad → 应失败（防止跨设备重放）
        let aad_b = build_aad(AadKind::Text, "device-B", 1);
        let result = sealer.decrypt(&key, &nonce_b64, &ct_b64, &aad_b);

        assert!(
            matches!(result, Err(SealError::DecryptFailed)),
            "AAD mismatch (different origin) must cause DecryptFailed"
        );
    }

    /// clipboard_rejects_unknown_peer：is_known 失败路径验证（逻辑单元测试）
    #[test]
    fn clipboard_rejects_unknown_peer_logic() {
        // 验证 PeerRegistry.is_known 对未注册 peer 返 false
        use crate::peer::PeerRegistry;
        let registry = PeerRegistry::new_for_test();
        assert!(
            !registry.is_known("unknown-peer"),
            "is_known must return false for unregistered peer"
        );
    }

    /// clipboard_rejects_banned_peer：is_banned 失败路径验证
    #[test]
    fn clipboard_rejects_banned_peer_logic() {
        use crate::peer::{PeerRegistry, PeerState, TrustState};
        use std::collections::HashMap;
        use std::net::SocketAddr;
        use zeroize::Zeroizing;

        let registry = PeerRegistry::new_for_test();
        let peer_id = "banned-peer-001";

        // 先 insert，再 ban
        let peer = PeerState {
            device_id: peer_id.to_string(),
            device_name: "Bad Actor".to_string(),
            addr: "127.0.0.1:9998".parse::<SocketAddr>().expect("addr parse"),
            pubkey_b64: "test".to_string(),
            aes_key: Zeroizing::new([0u8; 32]),
            last_successful_sync_at: None,
            last_heartbeat_at: None,
            consecutive_heartbeat_failures: 0,
            consecutive_send_failures: 0,
            trust_state: TrustState::Pending,
            last_seen_seq_by_kind: HashMap::new(),
        };
        registry.insert(peer);
        registry.ban(peer_id);

        // ban 后 is_known 应为 false（ban 踢出 inner）
        assert!(
            !registry.is_known(peer_id),
            "banned peer must be removed from inner"
        );
        assert!(
            registry.is_banned(peer_id),
            "banned peer must be in banned set"
        );
    }

    /// clipboard_seq_dedupe：相同 seq 第二次应返 false（replay 丢弃）
    #[test]
    fn clipboard_seq_dedupe() {
        use crate::crypto::AadKind;
        use crate::peer::{PeerRegistry, PeerState, TrustState};
        use std::collections::HashMap;
        use std::net::SocketAddr;
        use zeroize::Zeroizing;

        let registry = PeerRegistry::new_for_test();
        let peer_id = "seq-test-peer";

        let peer = PeerState {
            device_id: peer_id.to_string(),
            device_name: "SeqPeer".to_string(),
            addr: "127.0.0.1:9997".parse::<SocketAddr>().expect("addr parse"),
            pubkey_b64: "test".to_string(),
            aes_key: Zeroizing::new([0u8; 32]),
            last_successful_sync_at: None,
            last_heartbeat_at: None,
            consecutive_heartbeat_failures: 0,
            consecutive_send_failures: 0,
            trust_state: TrustState::Approved,
            last_seen_seq_by_kind: HashMap::new(),
        };
        registry.insert(peer);

        // 第一次 seq=7 → true（新）
        assert!(registry.seen_seq_and_update(peer_id, AadKind::Text, 7));
        // 第二次 seq=7 → false（重放）
        assert!(!registry.seen_seq_and_update(peer_id, AadKind::Text, 7));
        // seq=8 → true（递增）
        assert!(registry.seen_seq_and_update(peer_id, AadKind::Text, 8));
    }
}
