//! Crypto trait 定义 + AAD 拼装 + 常量
//! see specs/e2e-encryption.md, decisions/ADR-011-crypto-traits.md
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-1 AAD 绑值, MUST-2 zeroize)
//!
//! FUTURE: 若 v3 引入 PSK / HMAC challenge，会在此处新增
//! `pub trait Verifier { fn verify_origin(&self, claim: &[u8]) -> Result<(), VerifyError>; }`
//! 触发条件：ADR-008 第 3.7 节 PSK 否决决议被 supersede。
//! 引入路径：新 ADR-N supersede 本 ADR 第 3.1 节，handler 增 1 行 verifier.verify_origin。

pub mod aes_gcm;
pub mod x25519;

pub use aes_gcm::AesGcmSealer;
pub use x25519::X25519KeyExchange;

// ---------------------------------------------------------------------------
// 常量（唯一定义点）
// HKDF_SALT / HKDF_INFO 在 crypto/x25519.rs 顶部定义（ADR-011 第 3.4 节实施提示 #2）
// ---------------------------------------------------------------------------

/// AAD magic 前缀（唯一定义点）。
/// code-reviewer PR 阶段：grep "sync-copy-v2" 字面量个数 = 3（salt + info + magic 各一处）。
/// future v3 bump：同步修改 HKDF_SALT / HKDF_INFO（ADR-011 第 3.4 节"两常量 bump 一致"不变式）。
pub const AAD_MAGIC: &[u8] = b"sync-copy-v2";

// ---------------------------------------------------------------------------
// KeyExchange trait（ADR-011 第 3.1 节）
// ---------------------------------------------------------------------------

/// 密钥协商 trait。
/// 默认实现：X25519KeyExchange（crypto/x25519.rs）。
/// 设计上是无状态 unit struct；future 若需注入 PRNG 配置则 supersede 本节。
pub trait KeyExchange {
    /// 实现层的临时秘钥类型（x25519-dalek::EphemeralSecret）。
    type Secret;
    /// 实现层的公钥类型（x25519-dalek::PublicKey）。
    type PublicKey;

    /// 生成一对临时密钥（Secret 消费语义；由 x25519-dalek API 强约束）。
    fn new_ephemeral() -> (Self::Secret, Self::PublicKey);

    /// 把公钥编码为 base64 字符串（URL-safe 无 padding 或 standard 均可，实现层统一即可）。
    fn pubkey_to_b64(pk: &Self::PublicKey) -> String;

    /// 从 base64 字符串解码公钥；格式错误返 KeyExchangeError。
    fn pubkey_from_b64(s: &str) -> Result<Self::PublicKey, KeyExchangeError>;

    /// 消费 secret + 对端公钥 → ECDH → HKDF-SHA256 → 32 字节 AES key。
    /// HKDF salt/info 由 impl 决定（见 x25519.rs HKDF_SALT / HKDF_INFO 常量）；调用方不传。
    fn derive_aes_key(
        secret: Self::Secret,
        their: &Self::PublicKey,
    ) -> Result<[u8; 32], KeyExchangeError>;
}

// ---------------------------------------------------------------------------
// Sealer trait（ADR-011 第 3.1 节）
// ---------------------------------------------------------------------------

/// 对称加密 / 解密 trait（AES-256-GCM）。
/// aad 由 caller 用 build_aad() 拼装（ADR-008 MUST-1 / ADR-011 第 3.3 节）。
/// 加密失败返 SealError；调用方映射到 401/UNAUTHORIZED（不是 500）。
pub trait Sealer {
    /// 加密 plaintext。
    ///
    /// nonce 由 impl 内部 OsRng 生成（caller 不可注入 nonce，ADR-011 第 7.2 节 第 4 条）。
    /// 返回值：nonce_b64（12 字节 base64）+ ciphertext_b64（含 16 字节 GCM tag 的 base64）。
    fn encrypt(
        &self,
        key: &[u8; 32],
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<(String, String), SealError>;

    /// 解密。
    /// aad 必须与加密时相同（AAD 绑值防线；ADR-008 MUST-1）。
    /// 任何错误（key mismatch / aad mismatch / tampered）统一返 SealError::DecryptFailed。
    fn decrypt(
        &self,
        key: &[u8; 32],
        nonce_b64: &str,
        ct_b64: &str,
        aad: &[u8],
    ) -> Result<Vec<u8>, SealError>;
}

// ---------------------------------------------------------------------------
// 错误类型（ADR-011 第 3.1 节；boundary enum，按 ADR-003 第 3.6 节）
// ---------------------------------------------------------------------------

/// 密钥协商错误。
#[derive(Debug)]
pub enum KeyExchangeError {
    /// pubkey base64 decode 失败。
    Base64,
    /// pubkey 必须是 32 字节。
    Length,
    /// hkdf expand 失败（输出长度请求超出 HKDF 上限，实际不会发生）。
    Hkdf,
}

impl std::fmt::Display for KeyExchangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyExchangeError::Base64 => write!(f, "pubkey base64 decode failed"),
            KeyExchangeError::Length => write!(f, "pubkey must be 32 bytes"),
            KeyExchangeError::Hkdf => write!(f, "hkdf expand failed"),
        }
    }
}

impl std::error::Error for KeyExchangeError {}

/// 加密 / 解密错误。
#[derive(Debug)]
pub enum SealError {
    /// nonce 或 ciphertext base64 decode 失败。
    Base64,
    /// nonce 必须是 12 字节。
    NonceLength,
    /// AEAD 加密失败（通常不发生；OsRng / aes-gcm 内部错）。
    EncryptFailed,
    /// AEAD 解密失败（key mismatch / aad mismatch / 报文被篡改）。
    /// 触发 NetworkError → 401；body 不区分具体原因（ADR-008 第 4 节信息边界）。
    DecryptFailed,
}

impl std::fmt::Display for SealError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SealError::Base64 => write!(f, "nonce/ciphertext base64 decode failed"),
            SealError::NonceLength => write!(f, "nonce must be 12 bytes"),
            SealError::EncryptFailed => write!(f, "aead encrypt failed"),
            SealError::DecryptFailed => {
                write!(
                    f,
                    "aead decrypt failed (key mismatch / aad mismatch / tampered)"
                )
            }
        }
    }
}

impl std::error::Error for SealError {}

// ---------------------------------------------------------------------------
// AadKind enum（ADR-011 第 3.3 节；9 个值与 ADR-009 last_seen_seq_by_kind 对应）
// ---------------------------------------------------------------------------

/// AAD 种类枚举。
/// 9 个值与 ADR-009 第 3.1 节 last_seen_seq_by_kind 的 kind 字面量一一对应。
/// v2 实质使用 3 个（Text / ImagePng / File）；其余保留供 future 加密 gossip 报文 + dedupe。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AadKind {
    /// 文本剪切板（clipboard-text-sync）
    Text,
    /// PNG 图片剪切板（clipboard-image-sync）
    ImagePng,
    /// 文件传输（file-transfer-drag）
    File,
    /// 信任传播（group-trust-gossip）
    Trust,
    /// 封禁传播（group-trust-gossip）
    Ban,
    /// 离线广播（group-leave-notify）
    Leave,
    /// 跨机删除历史（history-sync-delete）
    DeleteHistory,
    /// 跨机清空历史（history-sync-delete）
    ClearHistory,
    /// 审批转发回流报文（group-approval）
    Approval,
}

impl AadKind {
    /// 返回 kind 对应的字节字面量。
    /// 字面量表见 ADR-011 第 3.3 节表；全小写 + snake_case。
    pub fn as_bytes(self) -> &'static [u8] {
        match self {
            AadKind::Text => b"text",
            AadKind::ImagePng => b"image_png",
            AadKind::File => b"file",
            AadKind::Trust => b"trust",
            AadKind::Ban => b"ban",
            AadKind::Leave => b"leave",
            AadKind::DeleteHistory => b"delete_history",
            AadKind::ClearHistory => b"clear_history",
            AadKind::Approval => b"approval",
        }
    }
}

// ---------------------------------------------------------------------------
// build_aad（ADR-008 第 7.2 节 MUST-1 / ADR-011 第 3.3 节）
// ---------------------------------------------------------------------------

/// ADR-008 第 7.2 节 MUST-1 落地：AAD = magic || kind || origin_device_id || seq (BE 8 bytes)。
///
/// 字节顺序（锁死，见 ADR-011 第 3.3 节）：
///   `b"sync-copy-v2"` (12B) || `kind.as_bytes()` (变长 ASCII) || `origin_device_id.as_bytes()` (变长 UTF-8) || `seq.to_be_bytes()` (8B big-endian)
///
/// 所有 broadcast handler 在 encrypt / decrypt 前调此函数；禁止散点拼装、禁止跳过直传 `&[]`。
pub fn build_aad(kind: AadKind, origin_device_id: &str, seq: u64) -> Vec<u8> {
    let kind_bytes = kind.as_bytes();
    let id_bytes = origin_device_id.as_bytes();
    let mut buf = Vec::with_capacity(AAD_MAGIC.len() + kind_bytes.len() + id_bytes.len() + 8);
    buf.extend_from_slice(AAD_MAGIC);
    buf.extend_from_slice(kind_bytes);
    buf.extend_from_slice(id_bytes);
    buf.extend_from_slice(&seq.to_be_bytes());
    buf
}

// ---------------------------------------------------------------------------
// 单元测试（ADR-011 第 3.6 节 build_aad 字节顺序锁死）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// ADR-011 第 3.6 节 build_aad 单测：断言固定输入产生固定字节序列。
    /// 防止 future 重构改顺序失误。
    #[test]
    fn aad_layout_byte_exact() {
        let origin = "device-A";
        let seq: u64 = 42;
        let aad = build_aad(AadKind::Text, origin, seq);

        // 手算期望值：
        //   magic     = b"sync-copy-v2"  (12 字节)
        //   kind      = b"text"          (4 字节)
        //   origin    = b"device-A"      (8 字节)
        //   seq BE8   = 42u64.to_be_bytes()
        let mut expected = Vec::new();
        expected.extend_from_slice(b"sync-copy-v2");
        expected.extend_from_slice(b"text");
        expected.extend_from_slice(b"device-A");
        expected.extend_from_slice(&42u64.to_be_bytes());

        assert_eq!(aad, expected, "build_aad 字节顺序与 ADR-011 第 3.3 节不符");
    }

    /// 验证 seq 使用 big-endian 编码（网络字节序）。
    #[test]
    fn aad_seq_is_big_endian() {
        let aad1 = build_aad(AadKind::File, "dev", 1u64);
        let aad2 = build_aad(AadKind::File, "dev", 256u64);

        // seq=1 的最后 8 字节应为 [0,0,0,0,0,0,0,1]
        // seq=256 的最后 8 字节应为 [0,0,0,0,0,0,1,0]
        let tail1 = &aad1[aad1.len() - 8..];
        let tail2 = &aad2[aad2.len() - 8..];
        assert_eq!(tail1, &1u64.to_be_bytes());
        assert_eq!(tail2, &256u64.to_be_bytes());
    }

    /// 验证不同 AadKind 产生不同 aad（enum 覆盖 9 个值全部不同）。
    #[test]
    fn aad_kind_bytes_all_distinct() {
        let kinds = [
            AadKind::Text,
            AadKind::ImagePng,
            AadKind::File,
            AadKind::Trust,
            AadKind::Ban,
            AadKind::Leave,
            AadKind::DeleteHistory,
            AadKind::ClearHistory,
            AadKind::Approval,
        ];
        let aads: Vec<Vec<u8>> = kinds.iter().map(|k| build_aad(*k, "origin", 0)).collect();

        // 检查两两不同
        for i in 0..aads.len() {
            for j in (i + 1)..aads.len() {
                assert_ne!(
                    aads[i], aads[j],
                    "AadKind {:?} 和 {:?} 产生相同 aad",
                    kinds[i], kinds[j]
                );
            }
        }
    }
}
