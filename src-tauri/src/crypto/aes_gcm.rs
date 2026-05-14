//! AES-256-GCM 加密实现（Sealer trait 的默认实现）
//! see specs/e2e-encryption.md, decisions/ADR-011-crypto-traits.md
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-1 AAD 绑值)
//!
//! 设计不变式（ADR-011 第 7.2 节 第 4 条 / ADR-008 安全审阅）：
//! - nonce 由 impl 内部 OsRng 生成；caller 不可注入 nonce（Sealer::encrypt 签名无 nonce 入参）
//! - 任何 decrypt 失败统一返 SealError::DecryptFailed（不区分具体原因，防错误信息泄露）

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

use super::{SealError, Sealer};

// ---------------------------------------------------------------------------
// AesGcmSealer unit struct
// ---------------------------------------------------------------------------

/// AES-256-GCM 加密器（无状态 unit struct）。
/// 线程安全靠"无内部可变状态"——不需要 Arc/Mutex 包装。
/// future 若需注入 PRNG 配置则 supersede ADR-011 第 3.2 节。
pub struct AesGcmSealer;

impl Sealer for AesGcmSealer {
    /// 加密 plaintext。
    ///
    /// 内部流程：
    /// 1. OsRng 生成 12 字节 nonce（caller 不可注入，密码学不变式）
    /// 2. Aes256Gcm::new(key) 构造 cipher（aes-gcm 0.10 自带 ZeroizeOnDrop）
    /// 3. cipher.encrypt(nonce, Payload { msg: plaintext, aad })
    /// 4. 返回 (nonce_b64, ciphertext_b64)；nonce 和 ciphertext 各自 base64
    fn encrypt(
        &self,
        key: &[u8; 32],
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<(String, String), SealError> {
        let cipher = Aes256Gcm::new(key.into());
        // OsRng 生成 12 字节随机 nonce；每次调用独立生成（防止 nonce 复用）
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| SealError::EncryptFailed)?;

        let nonce_b64 = B64.encode(nonce.as_slice());
        let ct_b64 = B64.encode(&ciphertext);
        Ok((nonce_b64, ct_b64))
    }

    /// 解密。
    ///
    /// 内部流程：
    /// 1. base64 解码 nonce_b64 → 12 字节
    /// 2. base64 解码 ct_b64 → 密文（含 16 字节 GCM tag）
    /// 3. cipher.decrypt(nonce, Payload { msg: ciphertext, aad })
    /// 4. 任何失败统一返 SealError::DecryptFailed（key mismatch / aad mismatch / 被篡改不可区分）
    fn decrypt(
        &self,
        key: &[u8; 32],
        nonce_b64: &str,
        ct_b64: &str,
        aad: &[u8],
    ) -> Result<Vec<u8>, SealError> {
        let nonce_bytes = B64.decode(nonce_b64).map_err(|_| SealError::Base64)?;
        if nonce_bytes.len() != 12 {
            return Err(SealError::NonceLength);
        }
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = B64.decode(ct_b64).map_err(|_| SealError::Base64)?;

        let cipher = Aes256Gcm::new(key.into());
        let plaintext = cipher
            .decrypt(
                nonce,
                Payload {
                    msg: &ciphertext,
                    aad,
                },
            )
            // 统一映射为 DecryptFailed；不区分原因（ADR-008 第 4 节信息边界）
            .map_err(|_| SealError::DecryptFailed)?;

        Ok(plaintext)
    }
}

// ---------------------------------------------------------------------------
// 测试辅助函数
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{build_aad, AadKind};

    /// 生成确定性测试用 key（32 字节全 0x42）。
    fn test_key() -> [u8; 32] {
        [0x42u8; 32]
    }

    /// 生成另一把与 test_key 不同的 key（32 字节全 0x13）。
    fn other_key() -> [u8; 32] {
        [0x13u8; 32]
    }

    // -----------------------------------------------------------------------
    // 测试 #1（ADR-011 第 3.6 节）：encrypt → decrypt round-trip 还原 plaintext
    // -----------------------------------------------------------------------

    /// encrypt 后用相同 key + 相同 aad decrypt，返回原 plaintext 字节相等。
    #[test]
    fn roundtrip_text_decrypts_to_plaintext() {
        let sealer = AesGcmSealer;
        let key = test_key();
        let plaintext = b"hello, sync-copy v2";
        let aad = build_aad(AadKind::Text, "device-A", 1);

        let (nonce_b64, ct_b64) = sealer
            .encrypt(&key, plaintext, &aad)
            .expect("encrypt 不应失败");

        let recovered = sealer
            .decrypt(&key, &nonce_b64, &ct_b64, &aad)
            .expect("decrypt 不应失败（正确 key + 正确 aad）");

        assert_eq!(
            recovered.as_slice(),
            plaintext,
            "round-trip 还原的明文与原始明文不一致"
        );
    }

    // -----------------------------------------------------------------------
    // 测试 #2（ADR-011 第 3.6 节）：改 ciphertext 任一字节后 decrypt 失败
    // -----------------------------------------------------------------------

    /// 篡改 ciphertext 后 decrypt 返 SealError::DecryptFailed（GCM tag 验证失败）。
    #[test]
    fn tampered_ciphertext_decrypt_fails() {
        let sealer = AesGcmSealer;
        let key = test_key();
        let plaintext = b"tamper me";
        let aad = build_aad(AadKind::Text, "device-A", 1);

        let (nonce_b64, ct_b64) = sealer
            .encrypt(&key, plaintext, &aad)
            .expect("encrypt 不应失败");

        // 翻转 ciphertext base64 解码后第一字节
        let mut ct_bytes = B64.decode(&ct_b64).expect("base64 decode");
        ct_bytes[0] ^= 0xFF;
        let bad_ct_b64 = B64.encode(&ct_bytes);

        let result = sealer.decrypt(&key, &nonce_b64, &bad_ct_b64, &aad);
        assert!(
            matches!(result, Err(SealError::DecryptFailed)),
            "篡改 ciphertext 后 decrypt 应返 DecryptFailed，实际: {:?}",
            result.err()
        );
    }

    // -----------------------------------------------------------------------
    // 测试 #3（ADR-011 第 3.6 节 / ADR-008 MUST-1 单测要求）：改 aad 任一字节后 decrypt 失败
    // -----------------------------------------------------------------------

    /// 改 aad 的 magic 段首字节后 decrypt 失败（AAD 绑值 magic 段防线）。
    #[test]
    fn aad_byte_flip_magic_decrypt_fails() {
        let sealer = AesGcmSealer;
        let key = test_key();
        let plaintext = b"aad test";
        let aad = build_aad(AadKind::Text, "device-A", 1);

        let (nonce_b64, ct_b64) = sealer
            .encrypt(&key, plaintext, &aad)
            .expect("encrypt 不应失败");

        // 翻转 aad 第 0 字节（magic 段 'b'[0] = 's'）
        let mut bad_aad = aad.clone();
        bad_aad[0] ^= 0x01;

        let result = sealer.decrypt(&key, &nonce_b64, &ct_b64, &bad_aad);
        assert!(
            matches!(result, Err(SealError::DecryptFailed)),
            "改 aad magic 段首字节后 decrypt 应返 DecryptFailed"
        );
    }

    /// 改 aad 的 kind 段字节后 decrypt 失败（AAD 绑值 kind 段防线）。
    #[test]
    fn aad_byte_flip_kind_decrypt_fails() {
        let sealer = AesGcmSealer;
        let key = test_key();
        let plaintext = b"aad kind test";
        // magic(12B) 之后就是 kind；b"text" 第 0 字节 = 't' = 0x74
        let aad = build_aad(AadKind::Text, "device-A", 1);

        let (nonce_b64, ct_b64) = sealer
            .encrypt(&key, plaintext, &aad)
            .expect("encrypt 不应失败");

        // 翻转 aad[12]（kind 段第 0 字节）
        let mut bad_aad = aad.clone();
        bad_aad[12] ^= 0x01;

        let result = sealer.decrypt(&key, &nonce_b64, &ct_b64, &bad_aad);
        assert!(
            matches!(result, Err(SealError::DecryptFailed)),
            "改 aad kind 段字节后 decrypt 应返 DecryptFailed"
        );
    }

    /// 改 aad 的 seq 段最后字节后 decrypt 失败（AAD 绑值 seq 段防线）。
    #[test]
    fn aad_byte_flip_seq_decrypt_fails() {
        let sealer = AesGcmSealer;
        let key = test_key();
        let plaintext = b"aad seq test";
        let aad = build_aad(AadKind::File, "device-B", 99);

        let (nonce_b64, ct_b64) = sealer
            .encrypt(&key, plaintext, &aad)
            .expect("encrypt 不应失败");

        // 翻转 aad 最后一字节（seq 的 LSB）
        let mut bad_aad = aad.clone();
        let last = bad_aad.len() - 1;
        bad_aad[last] ^= 0x01;

        let result = sealer.decrypt(&key, &nonce_b64, &ct_b64, &bad_aad);
        assert!(
            matches!(result, Err(SealError::DecryptFailed)),
            "改 aad seq 段字节后 decrypt 应返 DecryptFailed"
        );
    }

    // -----------------------------------------------------------------------
    // 测试 #4（ADR-011 第 3.6 节）：跨 origin_device_id AAD 拒绝
    // A.encrypt(aad_origin_A) → B.decrypt(aad_origin_B) 失败
    // -----------------------------------------------------------------------

    /// 加密时用 origin=device-A，解密时用 origin=device-B → DecryptFailed。
    #[test]
    fn cross_origin_aad_rejected() {
        let sealer = AesGcmSealer;
        let key = test_key();
        let plaintext = b"cross origin test";

        let aad_a = build_aad(AadKind::Text, "device-A", 1);
        let aad_b = build_aad(AadKind::Text, "device-B", 1);

        let (nonce_b64, ct_b64) = sealer
            .encrypt(&key, plaintext, &aad_a)
            .expect("encrypt 不应失败");

        // 用 device-B 的 aad 解密 device-A 加密的密文
        let result = sealer.decrypt(&key, &nonce_b64, &ct_b64, &aad_b);
        assert!(
            matches!(result, Err(SealError::DecryptFailed)),
            "跨 origin_device_id 的 aad 不应解密成功"
        );
    }

    // -----------------------------------------------------------------------
    // 测试 #5（ADR-011 第 3.6 节）：跨 seq AAD 拒绝
    // 同 origin 不同 seq 互不能解
    // -----------------------------------------------------------------------

    /// 加密时用 seq=1，解密时用 seq=2 → DecryptFailed。
    #[test]
    fn cross_seq_aad_rejected() {
        let sealer = AesGcmSealer;
        let key = test_key();
        let plaintext = b"cross seq test";

        let aad_seq1 = build_aad(AadKind::Text, "device-A", 1);
        let aad_seq2 = build_aad(AadKind::Text, "device-A", 2);

        let (nonce_b64, ct_b64) = sealer
            .encrypt(&key, plaintext, &aad_seq1)
            .expect("encrypt 不应失败");

        // 用 seq=2 的 aad 解密 seq=1 加密的密文
        let result = sealer.decrypt(&key, &nonce_b64, &ct_b64, &aad_seq2);
        assert!(
            matches!(result, Err(SealError::DecryptFailed)),
            "跨 seq 的 aad 不应解密成功"
        );
    }

    // -----------------------------------------------------------------------
    // 测试 #6（ADR-011 第 3.6 节）：nonce 唯一性（100 次 encrypt 同输入，nonce 全不同）
    // -----------------------------------------------------------------------

    /// 连续 100 次 encrypt 同一 plaintext + 同一 aad + 同一 key，nonce_b64 全不同。
    /// OsRng 12B 碰撞概率 ≈ 0（生日界: 2^48 ≈ 2.8×10^14 次才期望碰撞）。
    #[test]
    fn nonce_uniqueness_under_repeated_encrypt() {
        let sealer = AesGcmSealer;
        let key = test_key();
        let plaintext = b"nonce test payload";
        let aad = build_aad(AadKind::ImagePng, "device-A", 42);

        let nonces: std::collections::HashSet<String> = (0..100)
            .map(|_| {
                let (nonce_b64, _) = sealer
                    .encrypt(&key, plaintext, &aad)
                    .expect("encrypt 不应失败");
                nonce_b64
            })
            .collect();

        assert_eq!(
            nonces.len(),
            100,
            "100 次 encrypt 的 nonce 应全部不同（发现碰撞）"
        );
    }

    // -----------------------------------------------------------------------
    // 额外测试：错误密钥 decrypt 失败（e2e-encryption AC #3）
    // -----------------------------------------------------------------------

    /// 用错误 key 解密应返 DecryptFailed（不是 panic 或其他错误）。
    #[test]
    fn wrong_key_decrypt_fails() {
        let sealer = AesGcmSealer;
        let key = test_key();
        let wrong_key = other_key();
        let plaintext = b"wrong key test";
        let aad = build_aad(AadKind::Text, "device-A", 1);

        let (nonce_b64, ct_b64) = sealer
            .encrypt(&key, plaintext, &aad)
            .expect("encrypt 不应失败");

        let result = sealer.decrypt(&wrong_key, &nonce_b64, &ct_b64, &aad);
        assert!(
            matches!(result, Err(SealError::DecryptFailed)),
            "错误 key 解密应返 DecryptFailed"
        );
    }

    // -----------------------------------------------------------------------
    // 额外测试：空明文 round-trip 也应成功
    // -----------------------------------------------------------------------

    /// 空 plaintext 也能正确 round-trip（边界场景）。
    #[test]
    fn empty_plaintext_roundtrip() {
        let sealer = AesGcmSealer;
        let key = test_key();
        let plaintext = b"";
        let aad = build_aad(AadKind::Text, "device-A", 0);

        let (nonce_b64, ct_b64) = sealer
            .encrypt(&key, plaintext, &aad)
            .expect("encrypt 不应失败（空明文）");

        let recovered = sealer
            .decrypt(&key, &nonce_b64, &ct_b64, &aad)
            .expect("decrypt 不应失败（空明文 round-trip）");

        assert_eq!(recovered.as_slice(), plaintext);
    }
}
