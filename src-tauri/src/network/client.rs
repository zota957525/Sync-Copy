use std::{sync::Arc, time::Duration};

use anyhow::Context;

use super::protocol::{ClipboardReq, HandshakeReq, HandshakeResp, PeerPublic};
use crate::{peer::Peer, state::AppState};

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

/// 一次握手的完整结果：对方的 Peer 信息 + 它告诉我的其它 peer（用于 gossip）
pub struct HandshakeResult {
    pub peer: Peer,
    pub gossip_peers: Vec<PeerPublic>,
}

/// 向目标 addr 握手
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
    let req = HandshakeReq {
        device_id: my_device_id.to_string(),
        device_name: my_device_name.to_string(),
        listen_port: my_listen_port,
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
    Ok(HandshakeResult {
        peer: Peer {
            device_id: body.device_id,
            device_name: body.device_name,
            addr: target,
        },
        gossip_peers: body.peers,
    })
}

/// 把文本广播给所有已知 peer
pub async fn broadcast_clipboard(state: Arc<AppState>, text: String) {
    let (device_id, device_name, seq) = {
        let cfg = state.config.read();
        let seq = state.next_seq();
        (cfg.device_id.clone(), cfg.device_name.clone(), seq)
    };
    let peers = state.peers.snapshot();
    if peers.is_empty() {
        return;
    }
    let body = ClipboardReq {
        origin_device_id: device_id,
        origin_device_name: device_name,
        seq,
        text,
    };
    let client = match build_client() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "cannot build reqwest client for broadcast");
            return;
        }
    };
    for peer in peers {
        let url = format!("http://{}/clipboard", peer.addr);
        let body = body.clone();
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
