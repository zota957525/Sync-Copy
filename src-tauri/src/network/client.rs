use std::{sync::Arc, time::Duration};

use super::protocol::{ClipboardReq, HandshakeReq, HandshakeResp};
use crate::{peer::Peer, state::AppState};

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(3))
        .build()
        .expect("reqwest client build")
}

/// 向目标 addr 做握手，返回对方的 Peer 信息
pub async fn handshake(
    target_addr: &str,
    my_password: &str,
    my_device_id: &str,
    my_device_name: &str,
    my_listen_port: u16,
) -> anyhow::Result<Peer> {
    let url = format!("http://{}/handshake", target_addr);
    let req = HandshakeReq {
        password: my_password.to_string(),
        device_id: my_device_id.to_string(),
        device_name: my_device_name.to_string(),
        // listen_addr 的 ip 对方通过 socket 能看到；这里只给端口，服务器会用 conn info 或信任此字段
        // 简化起见直接发 "0.0.0.0:port"，对端收到后应以 connection peer ip 为准。
        // M3 简版：我们把 LAN IP 识别放在前端（用户输入对端 ip:port），对端拿到后回连时使用"握手发起方 IP + port"
        listen_addr: format!("0.0.0.0:{}", my_listen_port),
    };
    let resp = client().post(&url).json(&req).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "握手失败：{} {}",
            resp.status().as_u16(),
            resp.status().canonical_reason().unwrap_or("")
        );
    }
    let body: HandshakeResp = resp.json().await?;
    Ok(Peer {
        device_id: body.device_id,
        device_name: body.device_name,
        addr: target_addr.to_string(),
    })
}

/// 把文本广播给所有已知 peer
pub async fn broadcast_clipboard(state: Arc<AppState>, text: String) {
    let (password, device_id, device_name, seq) = {
        let cfg = state.config.read();
        let seq = state.next_seq();
        (cfg.password.clone(), cfg.device_id.clone(), cfg.device_name.clone(), seq)
    };
    let peers = state.peers.snapshot();
    if peers.is_empty() {
        return;
    }
    let body = ClipboardReq {
        password,
        origin_device_id: device_id,
        origin_device_name: device_name,
        seq,
        text,
    };
    let client = client();
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
