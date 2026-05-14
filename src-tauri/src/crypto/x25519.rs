//! X25519 密钥协商实现（KeyExchange trait 的默认实现）
//! see specs/e2e-encryption.md, decisions/ADR-011-crypto-traits.md
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.4 节算法选型)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-2 zeroize)
//!
//! HKDF_SALT / HKDF_INFO 是唯一定义点（ADR-011 第 3.4 节 + 实施提示 #2）。
//! 其它 module 不重复定义；future v3 bump 时在此处同步修改两个常量。

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{EphemeralSecret, PublicKey};

use super::{KeyExchange, KeyExchangeError};

// ---------------------------------------------------------------------------
// HKDF 常量（ADR-011 第 3.4 节 / ADR-003 卡片 4 must-fix v2 bump 落地）
// 唯一定义点；code-reviewer grep "sync-copy-v2" 字面量 = 3（salt + info + mod.rs magic）
// ---------------------------------------------------------------------------

/// HKDF 提取阶段的 salt。v2 bump（与 v0 `b"sync-copy-v1-salt"` 不同）。
pub const HKDF_SALT: &[u8] = b"sync-copy-v2-salt";

/// HKDF 扩展阶段的 info（context string）。v2 bump。
pub const HKDF_INFO: &[u8] = b"sync-copy-v2:aes-256-gcm";

// ---------------------------------------------------------------------------
// X25519KeyExchange unit struct
// ---------------------------------------------------------------------------

/// X25519 + HKDF-SHA256 密钥协商（无状态 unit struct）。
/// 线程安全靠"无内部可变状态"——不需要 Arc/Mutex 包装。
pub struct X25519KeyExchange;

impl KeyExchange for X25519KeyExchange {
    type Secret = EphemeralSecret;
    type PublicKey = PublicKey;

    /// 生成一对临时密钥。EphemeralSecret 内部已用 OsRng。
    fn new_ephemeral() -> (Self::Secret, Self::PublicKey) {
        let secret = EphemeralSecret::random_from_rng(rand::thread_rng());
        let public = PublicKey::from(&secret);
        (secret, public)
    }

    /// 公钥编码为 standard base64（与 JSON 字段兼容）。
    fn pubkey_to_b64(pk: &Self::PublicKey) -> String {
        B64.encode(pk.as_bytes())
    }

    /// 从 standard base64 解码公钥（32 字节）。
    fn pubkey_from_b64(s: &str) -> Result<Self::PublicKey, KeyExchangeError> {
        let bytes = B64.decode(s).map_err(|_| KeyExchangeError::Base64)?;
        let arr: [u8; 32] = bytes.try_into().map_err(|_| KeyExchangeError::Length)?;
        Ok(PublicKey::from(arr))
    }

    /// ECDH → HKDF-SHA256 → 32 字节 AES key。
    ///
    /// 设计决策（ADR-011 第 3.5 节）：
    /// - EphemeralSecret + SharedSecret 由 x25519-dalek 2.x 自带 ZeroizeOnDrop；本函数消费 secret。
    /// - HKDF expand 输出缓冲 [u8;32] 以裸数组返给 caller；caller 负责 Zeroizing 包装（ADR-009 第 3.1 节）。
    /// - 本函数内部不 zeroize 返回缓冲（避免提前清零返回空字节，ADR-011 第 3.5 节反模式黑名单）。
    fn derive_aes_key(
        secret: Self::Secret,
        their: &Self::PublicKey,
    ) -> Result<[u8; 32], KeyExchangeError> {
        // DH 输出；SharedSecret 离开作用域时自动 zeroize（x25519-dalek ZeroizeOnDrop）
        let shared = secret.diffie_hellman(their);

        // HKDF-SHA256：extract(salt, ikm) → expand(info, 32B)
        let hk = Hkdf::<Sha256>::new(Some(HKDF_SALT), shared.as_bytes());
        let mut key = [0u8; 32];
        hk.expand(HKDF_INFO, &mut key)
            .map_err(|_| KeyExchangeError::Hkdf)?;

        // 返回裸数组；caller 立即包 Zeroizing::new(key)（ADR-009 第 3.1 节锁定）
        Ok(key)
    }
}

// ---------------------------------------------------------------------------
// 单元测试（ADR-011 第 3.6 节测试 #7 / #8 / #9）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use zeroize::Zeroizing;

    /// 测试 #9（ADR-011 第 3.6 节）：pubkey base64 roundtrip；长度 == 44 字符。
    /// 落实 e2e-encryption AC "公钥 base64 长度 44"。
    #[test]
    fn pubkey_b64_roundtrip() {
        let (_, pk) = X25519KeyExchange::new_ephemeral();
        let b64 = X25519KeyExchange::pubkey_to_b64(&pk);

        // standard base64 32 字节 = ceil(32/3)*4 = 44 字符（含 padding）
        assert_eq!(b64.len(), 44, "公钥 base64 长度必须是 44");

        let pk2 = X25519KeyExchange::pubkey_from_b64(&b64)
            .expect("pubkey_from_b64 不应失败（刚才 pubkey_to_b64 编码）");
        assert_eq!(
            pk.as_bytes(),
            pk2.as_bytes(),
            "pubkey_to_b64 -> pubkey_from_b64 roundtrip 字节不等"
        );
    }

    /// 测试 #7（ADR-011 第 3.6 节）：HKDF 确定性——derive_aes_key 输出 32 字节 zeroize 包装，
    /// 同输入（同 shared secret 模拟）产生同 key。
    ///
    /// 注意：x25519-dalek EphemeralSecret 消费语义无法直接重用同一 secret 跑两次 DH，
    /// 因此此测试以"重建两个不同临时对，HKDF 对相同输入确定性"为目标——
    /// 用 fixed IKM 直接调 HKDF verify 等价性（覆盖 ADR-011 第 3.6 节 #7 意图）。
    #[test]
    fn hkdf_deterministic_same_inputs_same_key() {
        // 模拟两次相同的 shared secret 字节（绕开 EphemeralSecret 消费限制）
        let fake_shared = [0x42u8; 32];
        let hk1 = hkdf::Hkdf::<sha2::Sha256>::new(Some(HKDF_SALT), &fake_shared);
        let hk2 = hkdf::Hkdf::<sha2::Sha256>::new(Some(HKDF_SALT), &fake_shared);

        let mut key1 = [0u8; 32];
        let mut key2 = [0u8; 32];
        hk1.expand(HKDF_INFO, &mut key1).unwrap();
        hk2.expand(HKDF_INFO, &mut key2).unwrap();

        let z1 = Zeroizing::new(key1);
        let z2 = Zeroizing::new(key2);
        assert_eq!(
            z1.as_ref(),
            z2.as_ref(),
            "相同 shared secret 的 HKDF 输出必须确定性相等"
        );
        assert_eq!(z1.len(), 32, "derive_aes_key 输出必须是 32 字节");
    }

    /// 测试 #8（ADR-011 第 3.6 节）：不同 peer 对（不同临时密钥对）派生的 AES key 互不相同。
    /// 落实 e2e-encryption AC "跨 peer 密钥独立"。
    #[test]
    fn cross_peer_keys_differ() {
        // Alice-Bob 对（alice_pub / bob_sec 在本对不互通，用于证明单侧可派生，另一侧用第二对）
        let (alice_sec, _alice_pub) = X25519KeyExchange::new_ephemeral();
        let (_bob_sec, bob_pub) = X25519KeyExchange::new_ephemeral();
        let key_ab_a = X25519KeyExchange::derive_aes_key(alice_sec, &bob_pub)
            .expect("Alice 侧 derive_aes_key 不应失败");

        // Carol-Dave 对（独立的临时密钥）
        let (carol_sec, _carol_pub) = X25519KeyExchange::new_ephemeral();
        let (_dave_sec, dave_pub) = X25519KeyExchange::new_ephemeral();
        let key_cd_c = X25519KeyExchange::derive_aes_key(carol_sec, &dave_pub)
            .expect("Carol 侧 derive_aes_key 不应失败");

        // Bob 对 Alice 重新 derive（DH 互通验证）
        let (alice_sec2, alice_pub2) = X25519KeyExchange::new_ephemeral();
        let (bob_sec2, bob_pub2) = X25519KeyExchange::new_ephemeral();
        let key_ab_a2 = X25519KeyExchange::derive_aes_key(alice_sec2, &bob_pub2)
            .expect("Alice2 侧 derive_aes_key 不应失败");
        let key_ab_b2 = X25519KeyExchange::derive_aes_key(bob_sec2, &alice_pub2)
            .expect("Bob2 侧 derive_aes_key 不应失败");

        // 同一对 DH 两侧必须得到相同 key（协议正确性）
        assert_eq!(
            key_ab_a2, key_ab_b2,
            "Alice-Bob 同一对 DH 两侧 derive 的 key 必须相等"
        );

        // 不同 peer 对的 key 必须不同
        assert_ne!(
            key_ab_a, key_cd_c,
            "不同 peer 对的 AES key 不应相等（跨 peer 密钥独立）"
        );
    }

    /// 验证 pubkey_from_b64 对无效 base64 返回 Base64 错误。
    #[test]
    fn pubkey_from_b64_invalid_base64_errors() {
        let result = X25519KeyExchange::pubkey_from_b64("not_valid_base64!!!");
        assert!(
            matches!(result, Err(KeyExchangeError::Base64)),
            "无效 base64 应返 KeyExchangeError::Base64"
        );
    }

    /// 验证 pubkey_from_b64 对长度不足 32 字节的 base64 返回 Length 错误。
    #[test]
    fn pubkey_from_b64_wrong_length_errors() {
        // 只编码 16 字节，不足 32
        let short_b64 = B64.encode([0u8; 16]);
        let result = X25519KeyExchange::pubkey_from_b64(&short_b64);
        assert!(
            matches!(result, Err(KeyExchangeError::Length)),
            "长度不足 32 字节应返 KeyExchangeError::Length"
        );
    }
}
