//! HTTP 协议 DTO（Data Transfer Objects）
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节 HTTP 协议骨架)
//! see decisions/ADR-008-security-review-of-adr003.md (第 7.2 节 MUST-3/6/7/8)
//!
//! 所有 DTO 均为 serde::Deserialize（从请求 body 解析）。
//! 通用 header（ADR-003 第 3.2 节 选项 B）：
//!   X-SC-Device-Id  — origin device id（header fast-fail；body 仍是权威）
//!   X-SC-Seq        — monotonic seq（header dedupe 快路径）
//!   X-SC-Auth       — 占位（ADR-003 决议 v2 暂不验证）

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// /handshake POST
// ---------------------------------------------------------------------------

/// 握手请求（POST /handshake）。
///
/// handler 在接收后首先调 sanitize_device_name(device_name)（ADR-008 MUST-8）。
#[derive(Debug, Deserialize)]
pub struct HandshakeReq {
    /// 发起方 device_id（UUID）
    pub device_id: String,
    /// 发起方展示名称（需 sanitize，ADR-008 第 4.4 节）
    pub device_name: String,
    /// 发起方 X25519 公钥（base64）
    pub pubkey_b64: String,
    /// 发起方监听端口（供本机回调）
    pub listen_port: u16,
}

/// 握手响应。
#[derive(Debug, Serialize)]
pub struct HandshakeResp {
    /// 本机 device_id
    pub device_id: String,
    /// 本机 X25519 公钥（base64）
    pub pubkey_b64: String,
}

// ---------------------------------------------------------------------------
// /clipboard POST
// ---------------------------------------------------------------------------

/// 剪切板推送请求（POST /clipboard）。
///
/// ADR-003 第 3.2 节：text / image_png 两种 kind；snapshot 复用此 DTO。
#[derive(Debug, Deserialize)]
pub struct ClipboardReq {
    pub origin_device_id: String,
    pub seq: u64,
    /// "text" | "image_png"
    pub kind: String,
    /// 加密后 nonce（base64）
    pub nonce_b64: String,
    /// 加密后密文（base64，含 GCM tag）
    pub ciphertext_b64: String,
    /// 是否为快照（clipboard-snapshot-sync；复用 /clipboard 端点）
    #[serde(default)]
    pub is_snapshot: bool,
}

// ---------------------------------------------------------------------------
// /file POST
// ---------------------------------------------------------------------------

/// 文件传输请求（POST /file）。
///
/// ADR-008 MUST-6：handler 入口需做 size 双校验 + seq dedupe。
#[derive(Debug, Deserialize)]
pub struct FileReq {
    pub origin_device_id: String,
    pub seq: u64,
    /// 文件名（需 sanitize，ADR-008 MUST-8 第 4.5 节）
    pub filename: String,
    /// 发送方声明文件大小（字节，声明值；不可信）
    pub size: u64,
    /// 加密后 nonce（base64）
    pub nonce_b64: String,
    /// 加密后密文（base64，含 GCM tag）
    pub ciphertext_b64: String,
}

// ---------------------------------------------------------------------------
// /heartbeat POST（v2 心跳，保留占位）
// ---------------------------------------------------------------------------

/// 心跳请求（POST /heartbeat）。
#[derive(Debug, Deserialize)]
pub struct HeartbeatReq {
    pub origin_device_id: String,
    pub seq: u64,
}

// ---------------------------------------------------------------------------
// /leave POST
// ---------------------------------------------------------------------------

/// 离线广播（POST /peers/leave）。
#[derive(Debug, Deserialize)]
pub struct LeaveReq {
    pub origin_device_id: String,
    pub seq: u64,
}

// ---------------------------------------------------------------------------
// /peers/trust  /peers/ban POST（trust gossip）
// ---------------------------------------------------------------------------

/// 信任 / 封禁传播（POST /peers/trust  /  POST /peers/ban）。
#[derive(Debug, Deserialize)]
pub struct TrustReq {
    pub origin_device_id: String,
    pub seq: u64,
    /// 被信任 / 封禁的目标 device_id
    pub subject_device_id: String,
}

// ---------------------------------------------------------------------------
// /peers/approval/{forward,decide,dismiss} POST
// ---------------------------------------------------------------------------

/// 审批转发（POST /peers/approval/forward）。
#[derive(Debug, Deserialize)]
pub struct ApprovalForwardReq {
    pub origin_device_id: String,
    pub seq: u64,
    /// 需审批的新 peer device_id
    pub newcomer_device_id: String,
    /// 新 peer 展示名称（需 sanitize）
    pub newcomer_name: String,
    /// 新 peer 公钥（base64）
    pub newcomer_pubkey_b64: String,
}

/// 审批决策（POST /peers/approval/decide）。
#[derive(Debug, Deserialize)]
pub struct ApprovalDecideReq {
    pub origin_device_id: String,
    pub seq: u64,
    pub newcomer_device_id: String,
    /// true = 批准；false = 拒绝
    pub approved: bool,
}

/// 审批取消（POST /peers/approval/dismiss）。
#[derive(Debug, Deserialize)]
pub struct ApprovalDismissReq {
    pub origin_device_id: String,
    pub seq: u64,
    pub newcomer_device_id: String,
}

// ---------------------------------------------------------------------------
// /history/clear DELETE / /delete_history POST
// ---------------------------------------------------------------------------

/// 跨机清空历史（DELETE /history 或 POST /history/clear）。
#[derive(Debug, Deserialize)]
pub struct ClearHistoryReq {
    pub origin_device_id: String,
    pub seq: u64,
}

/// 跨机删除单条历史（POST /delete_history）。
#[derive(Debug, Deserialize)]
pub struct DeleteHistoryReq {
    pub origin_device_id: String,
    pub seq: u64,
    /// 要删除的历史条目 content_hash（ADR-008 第 4.7 节：v2 暂用 SHA-256(plaintext)）
    pub content_hash: String,
}
