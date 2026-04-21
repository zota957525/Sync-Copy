use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    extract::{ConnectInfo, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde_json::json;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

use super::protocol::{ClipboardReq, HandshakeReq, HandshakeResp, PeerPublic};
use crate::{
    clipboard::ClipboardCmd,
    crypto,
    history::Source,
    peer::Peer,
    state::AppState,
};

const APPROVAL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct ServerCtx {
    state: Arc<AppState>,
    app: AppHandle,
}

pub async fn run(
    state: Arc<AppState>,
    app: AppHandle,
    port: u16,
    shutdown: oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    let ctx = ServerCtx { state, app };
    let router: Router = Router::new()
        .route("/handshake", post(handle_handshake))
        .route("/clipboard", post(handle_clipboard))
        .with_state(ctx);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "HTTP server listening");
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        let _ = shutdown.await;
        tracing::info!("HTTP server shutting down");
    })
    .await?;
    Ok(())
}

fn peer_list_excluding(state: &AppState, exclude_id: &str) -> Vec<PeerPublic> {
    state
        .peers
        .snapshot()
        .into_iter()
        .filter(|p| p.device_id != exclude_id)
        .map(|p| PeerPublic {
            device_id: p.device_id,
            device_name: p.device_name,
            addr: p.addr,
        })
        .collect()
}

async fn handle_handshake(
    State(ctx): State<ServerCtx>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    Json(req): Json<HandshakeReq>,
) -> Result<Json<HandshakeResp>, StatusCode> {
    let peer_addr = SocketAddr::new(remote.ip(), req.listen_port).to_string();

    // 解析对方公钥
    let their_pub = match crypto::pubkey_from_b64(&req.pubkey) {
        Ok(pk) => pk,
        Err(e) => {
            tracing::warn!(error = %e, "bad pubkey in handshake");
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    // 我自己的临时密钥对
    let (my_secret, my_public) = crypto::new_ephemeral();
    let my_pubkey_b64 = crypto::pubkey_to_b64(&my_public);

    let (my_id, my_name) = {
        let cfg = ctx.state.config.read();
        (cfg.device_id.clone(), cfg.device_name.clone())
    };
    if req.device_id == my_id {
        return Err(StatusCode::CONFLICT);
    }

    // 已知 peer：直接更新，重新协商密钥
    let known = ctx
        .state
        .peers
        .snapshot()
        .iter()
        .any(|p| p.device_id == req.device_id);
    if known {
        let aes_key = crypto::derive_aes_key(my_secret, &their_pub);
        ctx.state.peer_keys.set(req.device_id.clone(), aes_key);
        ctx.state.peers.upsert(Peer {
            device_id: req.device_id.clone(),
            device_name: req.device_name.clone(),
            addr: peer_addr.clone(),
        });
        tracing::info!(peer = %req.device_name, addr = %peer_addr, "re-handshake with known peer, key refreshed");
        return Ok(Json(HandshakeResp {
            device_id: my_id,
            device_name: my_name,
            peers: peer_list_excluding(&ctx.state, &req.device_id),
            pubkey: my_pubkey_b64,
        }));
    }

    // 新设备：走审批流程
    let request_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel::<bool>();
    {
        let mut pending = ctx.state.pending_approvals.lock();
        pending.insert(request_id.clone(), tx);
    }
    let _ = ctx.app.emit(
        "handshake-pending",
        json!({
            "request_id": request_id,
            "device_id": req.device_id,
            "device_name": req.device_name,
        }),
    );
    tracing::info!(peer = %req.device_name, addr = %peer_addr, "handshake pending user approval");

    let approved = match tokio::time::timeout(APPROVAL_TIMEOUT, rx).await {
        Ok(Ok(ok)) => ok,
        _ => {
            ctx.state.pending_approvals.lock().remove(&request_id);
            tracing::warn!(peer = %req.device_name, "handshake approval timed out");
            return Err(StatusCode::REQUEST_TIMEOUT);
        }
    };

    if !approved {
        tracing::info!(peer = %req.device_name, "handshake rejected by user");
        return Err(StatusCode::FORBIDDEN);
    }

    // 用户同意 → ECDH 派生共享 AES 密钥并持久化
    let aes_key = crypto::derive_aes_key(my_secret, &their_pub);
    ctx.state.peer_keys.set(req.device_id.clone(), aes_key);
    ctx.state.peers.upsert(Peer {
        device_id: req.device_id.clone(),
        device_name: req.device_name.clone(),
        addr: peer_addr.clone(),
    });
    crate::state::update_status_connected(&ctx.state);
    let _ = ctx.app.emit("status-updated", ());
    tracing::info!(peer = %req.device_name, addr = %peer_addr, "peer approved, key exchanged");

    Ok(Json(HandshakeResp {
        device_id: my_id,
        device_name: my_name,
        peers: peer_list_excluding(&ctx.state, &req.device_id),
        pubkey: my_pubkey_b64,
    }))
}

async fn handle_clipboard(
    State(ctx): State<ServerCtx>,
    Json(req): Json<ClipboardReq>,
) -> Result<StatusCode, StatusCode> {
    // 必须是已协商密钥的 peer
    let key = match ctx.state.peer_keys.get(&req.origin_device_id) {
        Some(k) => k,
        None => {
            tracing::warn!(origin = %req.origin_device_id, "clipboard from unknown peer (no key)");
            return Err(StatusCode::FORBIDDEN);
        }
    };

    let my_id = ctx.state.config.read().device_id.clone();
    if req.origin_device_id == my_id {
        return Ok(StatusCode::OK);
    }
    if !ctx.state.seen_seq_and_update(&req.origin_device_id, req.seq) {
        return Ok(StatusCode::OK);
    }

    // 解密
    let plaintext_bytes = match crypto::decrypt(&key, &req.nonce, &req.ciphertext) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, origin = %req.origin_device_name, "clipboard decrypt failed");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };
    let text = match String::from_utf8(plaintext_bytes) {
        Ok(s) => s,
        Err(_) => {
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    if ctx
        .state
        .history
        .push(
            text.clone(),
            Source::Remote {
                device_name: req.origin_device_name.clone(),
            },
        )
        .is_some()
    {
        let _ = ctx.app.emit("history-updated", ());
    }

    if let Some(tx) = ctx.state.clipboard_tx.lock().as_ref() {
        let _ = tx.send(ClipboardCmd::SetSuppress(text));
    }

    Ok(StatusCode::OK)
}
