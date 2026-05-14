//! NetworkError — 统一 HTTP 错误映射层
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节 7 状态码统一表)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-3 第 4.1 节 403 不可区分)
//!
//! ADR-003 第 3.2 节状态码语义（7 + 1 = 8 项）：
//!   200 OK
//!   400 Bad Request       — JSON 解析失败 / 字段缺失 / size 校验不通过
//!   403 Forbidden         — 鉴权失败（原因不外泄！所有 403 返同一 body）
//!   408 Request Timeout   — 审批超时 30s
//!   409 Device ID Conflict — ❌ v2 改返 403（ADR-008 MUST-3，防 device_id 枚举）
//!   413 Payload Too Large  — size > MAX_FILE_SIZE = 5 MB
//!   422 Unprocessable Entity — 解密失败 / plaintext.len != size
//!   429 Too Many Requests  — handshake DoS 限流（ADR-008 MUST-7，新增第 8 项）
//!   500 Internal Server Error — 不可恢复内部错
//!
//! MUST-3（ADR-008 第 7.2 节）：
//! - 409 DeviceIdConflict → 403 + body = "forbidden"（防 device_id 枚举）
//! - 所有 403 返同一 body 串 "forbidden"（ban / 不在 peers / 用户拒绝 三路径不可区分）
//! - 422 统一 body 串 "unprocessable"
//! - 429 统一 body 串 "too many requests"

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

// ---------------------------------------------------------------------------
// NetworkError enum
// ---------------------------------------------------------------------------

/// 所有 HTTP handler 可返回的业务错误。
///
/// IntoResponse 实现遵循 ADR-003 状态码表 + ADR-008 MUST-3 信息边界约束。
#[derive(Debug)]
pub enum NetworkError {
    // 400
    /// JSON 解析失败 / 字段缺失 / 格式错误
    BadRequest(String),

    // 403 — 全部对外返同一 body（ADR-008 MUST-3）
    /// 不在 peers 表（origin 未握手）
    NotInPeers,
    /// 在 banned 列表
    Banned,
    /// 用户拒绝审批
    UserRejected,
    /// device_id 与本机相同（ADR-008 MUST-3：改返 403 防枚举）
    DeviceIdConflict,

    // 408
    /// 审批超时 30s 无人决定
    ApprovalTimeout,

    // 413
    /// 请求体 / 文件内容超过 MAX_FILE_SIZE
    PayloadTooLarge,

    // 422
    /// 解密失败 / 实际 plaintext.len != size（ADR-008 MUST-3：统一 body）
    DecryptFailed,
    /// plaintext.len != 声明 size（同 422）
    SizeMismatch,

    // 429
    /// handshake DoS 限流（ADR-008 MUST-7；body 不区分 per-pair vs 全局）
    RateLimited,

    // 500
    /// 不可恢复内部错误（写盘失败等）
    Internal(String),
}

// ---------------------------------------------------------------------------
// IntoResponse 实现（ADR-008 MUST-3 信息边界）
// ---------------------------------------------------------------------------

impl IntoResponse for NetworkError {
    fn into_response(self) -> Response {
        // SECURITY (ADR-008 MUST-3):
        // - 所有 403 对外返同一 body "forbidden"，不暴露内部区分（ban / not_in_peers / user_rejected / device_id_conflict）
        // - 422 统一 body "unprocessable"（不区分 decrypt_failed vs size_mismatch）
        // - 429 统一 body "too many requests"（不区分 per-pair vs 全局）
        // - 400 / 500 body 为通用字面串，不含 internal Rust 细节（路径 / panic message 等）
        let (status, body) = match self {
            // 400
            NetworkError::BadRequest(_reason) => {
                // reason 仅进日志，不返回 body（ADR-003 第 3.6 节信息边界）
                (StatusCode::BAD_REQUEST, "invalid request")
            }

            // 403 — 全部统一 body（MUST-3）
            NetworkError::NotInPeers
            | NetworkError::Banned
            | NetworkError::UserRejected
            | NetworkError::DeviceIdConflict => {
                // ADR-008 MUST-3：DeviceIdConflict 改返 403（原 409）防 device_id 枚举
                (StatusCode::FORBIDDEN, "forbidden")
            }

            // 408
            NetworkError::ApprovalTimeout => (StatusCode::REQUEST_TIMEOUT, "approval timeout"),

            // 413
            NetworkError::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "payload too large"),

            // 422 — 统一 body（MUST-3）
            NetworkError::DecryptFailed | NetworkError::SizeMismatch => {
                (StatusCode::UNPROCESSABLE_ENTITY, "unprocessable")
            }

            // 429
            NetworkError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "too many requests"),

            // 500
            NetworkError::Internal(_reason) => {
                // reason 仅进日志
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error")
            }
        };

        (status, body).into_response()
    }
}

impl NetworkError {
    /// 记录内部原因到日志（body 不外泄，但实现层内部需要 debug 信息）。
    ///
    /// 调用方式：
    ///   `NetworkError::BadRequest(reason).log_and_return()`
    ///   等价于 `tracing::warn!(...); return Err(NetworkError::BadRequest(reason));`
    ///   但集中在 error.rs 保证日志格式一致。
    ///
    /// 注意：`reason` 不得含运行时 aes_key / plaintext 等敏感字段（ADR-008 MUST-5）。
    pub fn log(&self) {
        match self {
            NetworkError::BadRequest(r) => {
                tracing::warn!(target: "network::error", kind = "bad_request", reason = %r)
            }
            NetworkError::NotInPeers => {
                tracing::warn!(target: "network::error", kind = "not_in_peers")
            }
            NetworkError::Banned => tracing::warn!(target: "network::error", kind = "banned"),
            NetworkError::UserRejected => {
                tracing::warn!(target: "network::error", kind = "user_rejected")
            }
            NetworkError::DeviceIdConflict => {
                // SECURITY: 日志记录冲突事件（对内可观测），但 HTTP 返 403 不暴露（ADR-008 MUST-3）
                tracing::warn!(target: "network::error", kind = "device_id_conflict")
            }
            NetworkError::ApprovalTimeout => {
                tracing::warn!(target: "network::error", kind = "approval_timeout")
            }
            NetworkError::PayloadTooLarge => {
                tracing::warn!(target: "network::error", kind = "payload_too_large")
            }
            NetworkError::DecryptFailed => {
                // SECURITY: 统一日志字段（不区分 decrypt_failed vs size_mismatch 对外可见）
                tracing::warn!(target: "network::error", kind = "decrypt_or_size_mismatch")
            }
            NetworkError::SizeMismatch => {
                tracing::warn!(target: "network::error", kind = "decrypt_or_size_mismatch")
            }
            NetworkError::RateLimited => {
                tracing::warn!(target: "network::error", kind = "rate_limited")
            }
            NetworkError::Internal(r) => {
                tracing::error!(target: "network::error", kind = "internal", reason = %r)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 单元测试（ADR-008 MUST-3 验证）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::response::IntoResponse;

    async fn body_string(resp: Response) -> String {
        let bytes = to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body read");
        String::from_utf8(bytes.to_vec()).expect("utf8")
    }

    /// MUST-3：DeviceIdConflict → 403 + body = "forbidden"（不可枚举本机 device_id）
    #[tokio::test]
    async fn device_id_conflict_returns_403_forbidden() {
        let resp = NetworkError::DeviceIdConflict.into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = body_string(resp).await;
        assert_eq!(
            body, "forbidden",
            "DeviceIdConflict must return 'forbidden' body"
        );
    }

    /// MUST-3：Banned → 403 + body = "forbidden"（与 DeviceIdConflict 不可区分）
    #[tokio::test]
    async fn banned_returns_same_403_as_conflict() {
        let resp = NetworkError::Banned.into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = body_string(resp).await;
        assert_eq!(
            body, "forbidden",
            "Banned must return same 'forbidden' body as DeviceIdConflict"
        );
    }

    /// MUST-3：NotInPeers → 403 + body = "forbidden"（三路径不可区分）
    #[tokio::test]
    async fn not_in_peers_returns_403_forbidden() {
        let resp = NetworkError::NotInPeers.into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = body_string(resp).await;
        assert_eq!(body, "forbidden");
    }

    /// MUST-3：422 统一 body "unprocessable"（decrypt_failed vs size_mismatch 不可区分）
    #[tokio::test]
    async fn decrypt_failed_and_size_mismatch_same_422_body() {
        let r1 = NetworkError::DecryptFailed.into_response();
        let r2 = NetworkError::SizeMismatch.into_response();
        assert_eq!(r1.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(r2.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let b1 = body_string(r1).await;
        let b2 = body_string(r2).await;
        assert_eq!(b1, "unprocessable");
        assert_eq!(b2, "unprocessable");
        assert_eq!(
            b1, b2,
            "DecryptFailed and SizeMismatch must have identical body"
        );
    }

    /// 429 统一 body "too many requests"
    #[tokio::test]
    async fn rate_limited_returns_429() {
        let resp = NetworkError::RateLimited.into_response();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        let body = body_string(resp).await;
        assert_eq!(body, "too many requests");
    }

    /// 500 不暴露内部原因字符串
    #[tokio::test]
    async fn internal_error_body_does_not_expose_reason() {
        let resp = NetworkError::Internal("secret db path /home/user".to_string()).into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = body_string(resp).await;
        // body 不含 reason 内容
        assert_eq!(body, "internal error");
        assert!(
            !body.contains("secret"),
            "500 body must not expose internal reason"
        );
    }
}
