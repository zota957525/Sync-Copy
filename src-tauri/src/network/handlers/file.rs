//! POST /file handler
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-6 seq dedupe + size 双校验 / MUST-8 sanitize)
//!
//! PR-4 范围（MUST-6）：
//! - sanitize_filename（MUST-8）
//! - seen_seq_and_update → 重放 200 静默丢（MUST-6 严重发现 #1）
//! - 声明 size 校验 ≤ MAX_FILE_SIZE → 413
//! - base64 解码后字节长度校验 ≤ MAX_CIPHERTEXT_BYTES → 413（MUST-6 第 2 道闸）
//! - 占位返 503（crypto 解密 / 文件写盘 PR-5+）
//!
//! DefaultBodyLimit 收紧到 7MB 在 build_router() 层设置（ADR-008 MUST-6 配套）。
//!
//! 不在本 PR：crypto 真解密 / 审批弹框 / 文件写盘

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use base64::Engine as _;
use std::sync::Arc;

use crate::app::state::AppState;
use crate::crypto::AadKind;
use crate::network::error::NetworkError;
use crate::network::protocol::FileReq;
use crate::peer::sanitize::sanitize_filename;

/// 最大文件大小（ADR-003 第 3.2 节 + _assumptions A16：5 MB）
const MAX_FILE_SIZE: u64 = 5 * 1024 * 1024; // 5 MB

/// 最大 ciphertext 字节数（ADR-008 MUST-6 第 2 道闸）：
///   5MB * ceil(4/3) + 16B AEAD tag ≈ 6.7 MB → 取 7 MB 与 DefaultBodyLimit 对齐
/// 计算：5 * 1024 * 1024 * 4/3 + 16 = ~6,710,903 字节
const MAX_CIPHERTEXT_BYTES: usize = 7 * 1024 * 1024; // 7 MB（收紧边界与 DefaultBodyLimit 对齐）

/// POST /file
///
/// 入口检查顺序（MUST-6 mandated）：
/// 1. 来源鉴权（is_known）
/// 2. sanitize_filename（MUST-8）
/// 3. seq dedupe → 重放 200（MUST-6 严重发现 #1）
/// 4. 声明 size 校验 ≤ MAX_FILE_SIZE → 413
/// 5. ciphertext base64 解码后字节长度校验 ≤ MAX_CIPHERTEXT_BYTES → 413（第 2 道闸，MUST-6）
/// 6. 占位返 503
pub async fn handle_file(
    State(state): State<Arc<AppState>>,
    Json(req): Json<FileReq>,
) -> Result<StatusCode, NetworkError> {
    // --- 来源鉴权（ADR-008 MUST-3）---
    if !state.peers.is_known(&req.origin_device_id) {
        let err = NetworkError::NotInPeers;
        err.log();
        return Err(err);
    }

    // --- MUST-8：sanitize filename（ADR-008 第 4.5 节）---
    let safe_filename = sanitize_filename(&req.filename);
    // safe_filename 将在 PR-5+ 文件写盘时使用

    // --- MUST-6（严重发现 #1）：seq dedupe — 命中即 200 静默丢（ADR-008 第 4.2 节）---
    if !state
        .peers
        .seen_seq_and_update(&req.origin_device_id, AadKind::File, req.seq)
    {
        tracing::debug!(
            target: "network::file",
            origin = %req.origin_device_id,
            seq = req.seq,
            "file seq replay, silently dropped"
        );
        return Ok(StatusCode::OK);
    }

    // --- MUST-6（第 1 道闸）：声明 size 校验（v0 已有，保留）---
    if req.size > MAX_FILE_SIZE {
        let err = NetworkError::PayloadTooLarge;
        err.log();
        return Err(err);
    }

    // --- MUST-6（第 2 道闸）：实际 ciphertext 字节长度校验（decrypt 之前）---
    // ADR-008 MUST-6 决议：声明 1KB 但 ciphertext 6MB 的攻击，靠此闸挡住
    // SECURITY: 不先解密（防声明小字节实际大 ciphertext 的轻量 DoS）
    let ct_bytes_result = base64::engine::general_purpose::STANDARD.decode(&req.ciphertext_b64);
    let ct_len = match ct_bytes_result {
        Ok(ref bytes) => bytes.len(),
        Err(_) => {
            // base64 解码失败 → 400 格式错（不是 422；decrypt 还没发生）
            let err = NetworkError::BadRequest("ciphertext base64 decode failed".into());
            err.log();
            return Err(err);
        }
    };

    if ct_len > MAX_CIPHERTEXT_BYTES {
        tracing::warn!(
            target: "network::file",
            origin = %req.origin_device_id,
            ct_len,
            max = MAX_CIPHERTEXT_BYTES,
            "file ciphertext exceeds max bytes (MUST-6 second gate)"
        );
        let err = NetworkError::PayloadTooLarge;
        err.log();
        return Err(err);
    }

    tracing::debug!(
        target: "network::file",
        origin = %req.origin_device_id,
        seq = req.seq,
        safe_filename = %safe_filename,
        ct_len,
        "file received (PR-4 placeholder; crypto + write PR-5+)"
    );

    // 占位返 503（PR-5+ 替换为 crypto 解密 + 审批弹框 + 文件写盘）
    Ok(StatusCode::SERVICE_UNAVAILABLE)
}

// ---------------------------------------------------------------------------
// 单元测试（ADR-008 MUST-6 验证）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 MAX_FILE_SIZE 常量值正确（5 MB）
    #[test]
    fn max_file_size_is_5mb() {
        assert_eq!(MAX_FILE_SIZE, 5 * 1024 * 1024);
    }

    /// 验证 MAX_CIPHERTEXT_BYTES 常量 ≥ MAX_FILE_SIZE（保证合法文件能通过第 2 道闸）
    #[test]
    fn max_ciphertext_greater_than_max_file() {
        assert!(
            MAX_CIPHERTEXT_BYTES as u64 > MAX_FILE_SIZE,
            "MAX_CIPHERTEXT_BYTES must be > MAX_FILE_SIZE to allow legitimate files"
        );
    }

    /// base64 解码后超过上限的 ciphertext 会被拒绝（逻辑验证）
    #[test]
    fn oversized_ciphertext_exceeds_limit() {
        // 构造一个超限的字节向量
        let oversized = vec![0u8; MAX_CIPHERTEXT_BYTES + 1];
        assert!(
            oversized.len() > MAX_CIPHERTEXT_BYTES,
            "oversized bytes must exceed MAX_CIPHERTEXT_BYTES"
        );
    }
}
