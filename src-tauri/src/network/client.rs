use std::{sync::Arc, time::Duration};

use anyhow::Context;

use super::protocol::{
    ApprovalDecisionReq, ApprovalDismissReq, ApprovalForwardReq, ClipboardReq, DeleteHistoryReq,
    FileReq, GroupActionReq, HandshakeReq, HandshakeResp, PeerPublic, TrustReq,
};
use crate::{crypto, peer::Peer, state::AppState};

fn build_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(3))
        .build()
        .context("初始化 HTTP 客户端失败")
}

pub fn normalize_addr(raw: &str) -> String {
    raw.trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/')
        .trim()
        .to_string()
}

/// 握手结果：对方 Peer 信息 + gossip 列表 + 协商好的 AES 密钥
pub struct HandshakeResult {
    pub peer: Peer,
    pub gossip_peers: Vec<PeerPublic>,
    pub aes_key: [u8; 32],
}

pub async fn handshake(
    target_addr: &str,
    my_device_id: &str,
    my_device_name: &str,
    my_listen_port: u16,
) -> anyhow::Result<HandshakeResult> {
    let target = normalize_addr(target_addr);
    if !target.contains(':') {
        anyhow::bail!("加入目标格式不对，应该是 ip:port，例如 192.168.1.10:5858");
    }
    let url = format!("http://{}/handshake", target);

    // 临时 X25519 密钥对
    let (my_secret, my_public) = crypto::new_ephemeral();
    let my_pubkey_b64 = crypto::pubkey_to_b64(&my_public);

    let req = HandshakeReq {
        device_id: my_device_id.to_string(),
        device_name: my_device_name.to_string(),
        listen_port: my_listen_port,
        pubkey: my_pubkey_b64,
    };
    let client = build_client()?;
    let resp = client
        .post(&url)
        .json(&req)
        .timeout(Duration::from_secs(35))
        .send()
        .await
        .with_context(|| format!("连接 {} 失败", target))?;
    if !resp.status().is_success() {
        let status = resp.status();
        match status.as_u16() {
            400 => anyhow::bail!("握手请求被对方视为无效（协议版本不匹配？）"),
            403 => anyhow::bail!("对方拒绝了你的加入请求"),
            408 => anyhow::bail!("对方没有在 30 秒内确认，请让对方点「同意」后重试"),
            409 => anyhow::bail!("冲突：device_id 与对方相同（可能是同一份配置复制到两台机器）"),
            code => {
                let body = resp.text().await.unwrap_or_default();
                anyhow::bail!("握手失败：HTTP {} {}", code, body);
            }
        }
    }
    let body: HandshakeResp = resp.json().await.context("解析握手响应 JSON 失败")?;

    // 对方公钥 → 派生共享 AES 密钥
    let their_pub = crypto::pubkey_from_b64(&body.pubkey).context("对方返回的公钥无效")?;
    let aes_key = crypto::derive_aes_key(my_secret, &their_pub);

    Ok(HandshakeResult {
        peer: Peer {
            device_id: body.device_id,
            device_name: body.device_name,
            addr: target,
        },
        gossip_peers: body.peers,
        aes_key,
    })
}

/// 把明文字节 + 元数据广播给所有 peer，内部使用
async fn broadcast_payload(
    state: Arc<AppState>,
    plaintext: Vec<u8>,
    kind: &'static str,
    image_width: Option<u32>,
    image_height: Option<u32>,
) {
    let (device_id, device_name, seq) = {
        let cfg = state.config.read();
        let seq = state.next_seq();
        (cfg.device_id.clone(), cfg.device_name.clone(), seq)
    };
    let peers = state.peers.snapshot();
    if peers.is_empty() {
        return;
    }
    let client = match build_client() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "cannot build reqwest client for broadcast");
            return;
        }
    };
    for peer in peers {
        let key = match state.peer_keys.get(&peer.device_id) {
            Some(k) => k,
            None => {
                tracing::warn!(peer = %peer.device_name, "no AES key for peer, skip broadcast");
                continue;
            }
        };
        let (nonce_b64, ct_b64) = match crypto::encrypt(&key, &plaintext) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(error = %e, peer = %peer.device_name, "encrypt failed");
                continue;
            }
        };
        let body = ClipboardReq {
            origin_device_id: device_id.clone(),
            origin_device_name: device_name.clone(),
            seq,
            nonce: nonce_b64,
            ciphertext: ct_b64,
            kind: kind.to_string(),
            image_width,
            image_height,
        };
        let url = format!("http://{}/clipboard", peer.addr);
        let client = client.clone();
        tauri::async_runtime::spawn(async move {
            match client.post(&url).json(&body).send().await {
                Ok(r) if r.status().is_success() => {}
                Ok(r) => tracing::warn!(peer = %peer.device_name, status = %r.status(), "clipboard broadcast non-2xx"),
                Err(e) => tracing::warn!(peer = %peer.device_name, error = %e, "clipboard broadcast failed"),
            }
        });
    }
}

/// 广播文本
pub async fn broadcast_text(state: Arc<AppState>, text: String) {
    broadcast_payload(state, text.into_bytes(), "text", None, None).await
}

/// 广播图片（PNG 字节流）
pub async fn broadcast_image(state: Arc<AppState>, png: Vec<u8>, width: u32, height: u32) {
    broadcast_payload(state, png, "image_png", Some(width), Some(height)).await
}

/// 广播信任/封禁决定。path 应该是 "/peers/trust" 或 "/peers/ban"。
/// await 全部 peer 完成（或失败）后返回，便于上游用 tokio::time::timeout 控总时长
async fn broadcast_trust_decision(
    state: Arc<AppState>,
    path: &'static str,
    subject_device_id: String,
    subject_device_name: String,
) {
    let (origin_id, seq) = {
        let cfg = state.config.read();
        (cfg.device_id.clone(), state.next_seq())
    };
    let peers = state.peers.snapshot();
    if peers.is_empty() {
        return;
    }
    let body = TrustReq {
        origin_device_id: origin_id,
        seq,
        subject_device_id: subject_device_id.clone(),
        subject_device_name,
    };
    let client = match build_client() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "cannot build reqwest client for trust gossip");
            return;
        }
    };
    let mut handles = Vec::with_capacity(peers.len());
    for peer in peers {
        // 不把这个决定发给当事人自己
        if peer.device_id == subject_device_id {
            continue;
        }
        let url = format!("http://{}{}", peer.addr, path);
        let body = body.clone();
        let client = client.clone();
        let peer_name = peer.device_name.clone();
        handles.push(tauri::async_runtime::spawn(async move {
            match client.post(&url).json(&body).send().await {
                Ok(r) if r.status().is_success() => {}
                Ok(r) => tracing::warn!(peer = %peer_name, status = %r.status(), path, "trust gossip non-2xx"),
                Err(e) => tracing::warn!(peer = %peer_name, error = %e, path, "trust gossip failed"),
            }
        }));
    }
    for h in handles {
        let _ = h.await;
    }
}

pub async fn broadcast_trust(
    state: Arc<AppState>,
    subject_device_id: String,
    subject_device_name: String,
) {
    broadcast_trust_decision(state, "/peers/trust", subject_device_id, subject_device_name).await
}

pub async fn broadcast_ban(
    state: Arc<AppState>,
    subject_device_id: String,
    subject_device_name: String,
) {
    broadcast_trust_decision(state, "/peers/ban", subject_device_id, subject_device_name).await
}

/// 通知所有 peer：我要下线了，把我从 peers 表移除
pub async fn broadcast_leave(state: Arc<AppState>) {
    let (origin_id, seq) = {
        let cfg = state.config.read();
        (cfg.device_id.clone(), state.next_seq())
    };
    let peers = state.peers.snapshot();
    if peers.is_empty() {
        return;
    }
    let body = GroupActionReq {
        origin_device_id: origin_id,
        seq,
    };
    let client = match build_client() {
        Ok(c) => c,
        Err(_) => return,
    };
    let mut handles = Vec::with_capacity(peers.len());
    for peer in peers {
        let url = format!("http://{}/peers/leave", peer.addr);
        let body = body.clone();
        let client = client.clone();
        handles.push(tauri::async_runtime::spawn(async move {
            let _ = client.post(&url).json(&body).send().await;
        }));
    }
    for h in handles {
        let _ = h.await;
    }
}

/// 通知所有 peer：把全部历史清空
pub async fn broadcast_clear_history(state: Arc<AppState>) {
    let (origin_id, seq) = {
        let cfg = state.config.read();
        (cfg.device_id.clone(), state.next_seq())
    };
    let peers = state.peers.snapshot();
    if peers.is_empty() {
        tracing::info!("broadcast_clear_history: no peers");
        return;
    }
    let body = GroupActionReq {
        origin_device_id: origin_id,
        seq,
    };
    let client = match build_client() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "broadcast_clear_history: build client failed");
            return;
        }
    };
    for peer in peers {
        let url = format!("http://{}/history/clear", peer.addr);
        let body = body.clone();
        let client = client.clone();
        let peer_name = peer.device_name.clone();
        tauri::async_runtime::spawn(async move {
            match client.post(&url).json(&body).send().await {
                Ok(r) if r.status().is_success() => {
                    tracing::info!(peer = %peer_name, "clear-history broadcast ok");
                }
                Ok(r) => tracing::warn!(peer = %peer_name, status = %r.status(), "clear-history non-2xx"),
                Err(e) => tracing::warn!(peer = %peer_name, error = %e, "clear-history failed"),
            }
        });
    }
}

/// 把审批请求转发给所有其它 peer，让它们也弹框
pub async fn broadcast_approval_forward(
    state: Arc<AppState>,
    request_id: String,
    subject_device_id: String,
    subject_device_name: String,
) {
    let (origin_id, seq) = {
        let cfg = state.config.read();
        (cfg.device_id.clone(), state.next_seq())
    };
    let peers = state.peers.snapshot();
    if peers.is_empty() {
        return;
    }
    let body = ApprovalForwardReq {
        origin_device_id: origin_id,
        seq,
        request_id,
        subject_device_id: subject_device_id.clone(),
        subject_device_name,
    };
    let client = match build_client() {
        Ok(c) => c,
        Err(_) => return,
    };
    for peer in peers {
        // 不要转发给 subject 自己（其实 subject 这时不在 peers 表里，防御性检查）
        if peer.device_id == subject_device_id {
            continue;
        }
        let url = format!("http://{}/peers/approval/forward", peer.addr);
        let body = body.clone();
        let client = client.clone();
        tauri::async_runtime::spawn(async move {
            let _ = client.post(&url).json(&body).send().await;
        });
    }
}

/// B 把决定回传给 A，单播到指定 addr
pub async fn send_approval_decision(
    state: Arc<AppState>,
    origin_addr: String,
    request_id: String,
    accept: bool,
) {
    let (my_id, seq) = {
        let cfg = state.config.read();
        (cfg.device_id.clone(), state.next_seq())
    };
    let body = ApprovalDecisionReq {
        origin_device_id: my_id,
        seq,
        request_id,
        accept,
    };
    let client = match build_client() {
        Ok(c) => c,
        Err(_) => return,
    };
    let url = format!("http://{}/peers/approval/decide", origin_addr);
    let _ = client.post(&url).json(&body).send().await;
}

/// 通知所有 peer：这个审批已有结果了，把对应弹框关掉
pub async fn broadcast_approval_dismiss(state: Arc<AppState>, request_id: String) {
    let (origin_id, seq) = {
        let cfg = state.config.read();
        (cfg.device_id.clone(), state.next_seq())
    };
    let peers = state.peers.snapshot();
    if peers.is_empty() {
        return;
    }
    let body = ApprovalDismissReq {
        origin_device_id: origin_id,
        seq,
        request_id,
    };
    let client = match build_client() {
        Ok(c) => c,
        Err(_) => return,
    };
    for peer in peers {
        let url = format!("http://{}/peers/approval/dismiss", peer.addr);
        let body = body.clone();
        let client = client.clone();
        tauri::async_runtime::spawn(async move {
            let _ = client.post(&url).json(&body).send().await;
        });
    }
}

/// 广播删除历史条目（按 content_hash 通知所有 peer 删除同一内容）
pub async fn broadcast_delete(state: Arc<AppState>, content_hash: String) {
    let (device_id, seq) = {
        let cfg = state.config.read();
        let seq = state.next_seq();
        (cfg.device_id.clone(), seq)
    };
    let peers = state.peers.snapshot();
    if peers.is_empty() {
        return;
    }
    let body = DeleteHistoryReq {
        origin_device_id: device_id,
        seq,
        content_hash,
    };
    let client = match build_client() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "cannot build reqwest client for delete");
            return;
        }
    };
    for peer in peers {
        let url = format!("http://{}/delete_history", peer.addr);
        let body = body.clone();
        let client = client.clone();
        tauri::async_runtime::spawn(async move {
            match client.post(&url).json(&body).send().await {
                Ok(r) if r.status().is_success() => {}
                Ok(r) => tracing::warn!(peer = %peer.device_name, status = %r.status(), "delete broadcast non-2xx"),
                Err(e) => tracing::warn!(peer = %peer.device_name, error = %e, "delete broadcast failed"),
            }
        });
    }
}

/// 广播文件：返回 (成功数, 总 peer 数)
pub async fn broadcast_file(
    state: Arc<AppState>,
    filename: String,
    bytes: Vec<u8>,
) -> (usize, usize) {
    let (device_id, device_name, seq) = {
        let cfg = state.config.read();
        let seq = state.next_seq();
        (cfg.device_id.clone(), cfg.device_name.clone(), seq)
    };
    let peers = state.peers.snapshot();
    let total = peers.len();
    if total == 0 {
        return (0, 0);
    }
    let size = bytes.len() as u64;
    // 文件上传可能慢些（5MB 在慢 WiFi 上可能几秒）+ 对方要点「保存」
    let client = match reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(60))
        .connect_timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "cannot build reqwest client for file");
            return (0, total);
        }
    };

    let mut handles = Vec::with_capacity(total);
    for peer in peers {
        let Some(key) = state.peer_keys.get(&peer.device_id) else {
            tracing::warn!(peer = %peer.device_name, "no AES key for peer, skip file");
            continue;
        };
        let Ok((nonce_b64, ct_b64)) = crypto::encrypt(&key, &bytes) else {
            tracing::error!(peer = %peer.device_name, "encrypt file failed");
            continue;
        };
        let body = FileReq {
            origin_device_id: device_id.clone(),
            origin_device_name: device_name.clone(),
            seq,
            filename: filename.clone(),
            size,
            nonce: nonce_b64,
            ciphertext: ct_b64,
        };
        let url = format!("http://{}/file", peer.addr);
        let client = client.clone();
        let peer_name = peer.device_name.clone();
        handles.push(tauri::async_runtime::spawn(async move {
            match client.post(&url).json(&body).send().await {
                Ok(r) if r.status().is_success() => true,
                Ok(r) => {
                    tracing::warn!(peer = %peer_name, status = %r.status(), "file send non-2xx");
                    false
                }
                Err(e) => {
                    tracing::warn!(peer = %peer_name, error = %e, "file send failed");
                    false
                }
            }
        }));
    }

    let mut ok_count = 0usize;
    for h in handles {
        if let Ok(true) = h.await {
            ok_count += 1;
        }
    }
    (ok_count, total)
}
