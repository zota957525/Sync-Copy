//! 端到端加密：X25519 ECDH + HKDF-SHA256 → AES-256-GCM
//!
//! 每一对 peer 在握手时各自生成临时（ephemeral）X25519 密钥对，
//! 交换公钥后 Diffie-Hellman 算出 32 字节共享秘密，HKDF 派生出 AES 密钥，
//! 只存内存，进程重启即丢失 → 下次握手重新协商。
//!
//! 用户完全感知不到密钥的存在。
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Key, Nonce,
};
use anyhow::{anyhow, Context};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::Sha256;
use x25519_dalek::{EphemeralSecret, PublicKey};

const HKDF_SALT: &[u8] = b"sync-copy-v1-salt";
const HKDF_INFO: &[u8] = b"sync-copy-v1:aes-256-gcm";

/// 生成一把临时 X25519 密钥对
pub fn new_ephemeral() -> (EphemeralSecret, PublicKey) {
    let secret = EphemeralSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);
    (secret, public)
}

pub fn pubkey_to_b64(pk: &PublicKey) -> String {
    B64.encode(pk.as_bytes())
}

pub fn pubkey_from_b64(s: &str) -> anyhow::Result<PublicKey> {
    let bytes = B64.decode(s).context("pubkey base64 解码失败")?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow!("pubkey 必须是 32 字节"))?;
    Ok(PublicKey::from(arr))
}

/// Diffie-Hellman + HKDF 派生 AES-256 密钥
pub fn derive_aes_key(my_secret: EphemeralSecret, their_pub: &PublicKey) -> [u8; 32] {
    let shared = my_secret.diffie_hellman(their_pub);
    let hk = Hkdf::<Sha256>::new(Some(HKDF_SALT), shared.as_bytes());
    let mut aes_key = [0u8; 32];
    hk.expand(HKDF_INFO, &mut aes_key)
        .expect("HKDF expand failed"); // 只会在 output 太长时 fail，32 字节不会
    aes_key
}

/// 加密：输入明文 + 32 字节密钥，返回 (nonce_b64, ciphertext_b64)
pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> anyhow::Result<(String, String)> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, Payload { msg: plaintext, aad: &[] })
        .map_err(|e| anyhow!("加密失败：{:?}", e))?;
    Ok((B64.encode(nonce_bytes), B64.encode(ct)))
}

pub fn decrypt(key: &[u8; 32], nonce_b64: &str, ct_b64: &str) -> anyhow::Result<Vec<u8>> {
    let nonce_bytes = B64.decode(nonce_b64).context("nonce base64 解码失败")?;
    if nonce_bytes.len() != 12 {
        anyhow::bail!("nonce 必须是 12 字节");
    }
    let ct_bytes = B64.decode(ct_b64).context("ciphertext base64 解码失败")?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .decrypt(nonce, Payload { msg: &ct_bytes, aad: &[] })
        .map_err(|_| anyhow!("解密失败：密钥不一致或消息被篡改"))
}
