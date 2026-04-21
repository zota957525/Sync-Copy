use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    extract::{ConnectInfo, DefaultBodyLimit, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde_json::json;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

use super::protocol::{ClipboardReq, FileReq, HandshakeReq, HandshakeResp, PeerPublic};
use crate::{
    clipboard::ClipboardCmd,
    crypto,
    history::Source,
    peer::Peer,
    state::{AppState, PendingFileSave},
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
        .route("/file", post(handle_file))
        // 放宽 body 上限：5MB 文件 + base64 膨胀 + JSON 开销，留到 8MB
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
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

    let source = Source::Remote {
        device_name: req.origin_device_name.clone(),
    };

    match req.kind.as_str() {
        "image_png" => {
            let width = req.image_width.unwrap_or(0);
            let height = req.image_height.unwrap_or(0);
            if width == 0 || height == 0 {
                return Err(StatusCode::BAD_REQUEST);
            }
            let data_url = format!("data:image/png;base64,{}", B64.encode(&plaintext_bytes));
            if ctx
                .state
                .history
                .push_image(width, height, data_url, source)
                .is_some()
            {
                let _ = ctx.app.emit("history-updated", ());
            }
            if let Some(tx) = ctx.state.clipboard_tx.lock().as_ref() {
                let _ = tx.send(ClipboardCmd::SetImageSuppress {
                    png: plaintext_bytes,
                    width,
                    height,
                });
            }
        }
        _ => {
            // 默认文本
            let text = match String::from_utf8(plaintext_bytes) {
                Ok(s) => s,
                Err(_) => return Err(StatusCode::BAD_REQUEST),
            };
            if ctx
                .state
                .history
                .push_text(text.clone(), source)
                .is_some()
            {
                let _ = ctx.app.emit("history-updated", ());
            }
            if let Some(tx) = ctx.state.clipboard_tx.lock().as_ref() {
                let _ = tx.send(ClipboardCmd::SetTextSuppress(text));
            }
        }
    }

    Ok(StatusCode::OK)
}

const MAX_FILE_SIZE: u64 = 5 * 1024 * 1024;

async fn handle_file(
    State(ctx): State<ServerCtx>,
    Json(req): Json<FileReq>,
) -> Result<StatusCode, StatusCode> {
    let key = match ctx.state.peer_keys.get(&req.origin_device_id) {
        Some(k) => k,
        None => return Err(StatusCode::FORBIDDEN),
    };
    if req.size > MAX_FILE_SIZE {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let plaintext = match crypto::decrypt(&key, &req.nonce, &req.ciphertext) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "file decrypt failed");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };
    if plaintext.len() as u64 != req.size {
        return Err(StatusCode::BAD_REQUEST);
    }

    // 剥离路径，防止对方发 "../../etc/passwd" 写到奇怪位置
    let safe_name = sanitize_filename(&req.filename);

    let request_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel::<bool>();
    {
        let mut pending = ctx.state.pending_file_saves.lock();
        pending.insert(
            request_id.clone(),
            PendingFileSave {
                filename: safe_name.clone(),
                size: req.size,
                origin_device_name: req.origin_device_name.clone(),
                tx,
            },
        );
    }
    let _ = ctx.app.emit(
        "file-pending",
        serde_json::json!({
            "request_id": request_id,
            "filename": safe_name,
            "size": req.size,
            "origin_device_name": req.origin_device_name,
        }),
    );
    tracing::info!(peer = %req.origin_device_name, file = %safe_name, size = req.size, "file pending user approval");

    let approved = match tokio::time::timeout(APPROVAL_TIMEOUT, rx).await {
        Ok(Ok(ok)) => ok,
        _ => {
            ctx.state.pending_file_saves.lock().remove(&request_id);
            return Err(StatusCode::REQUEST_TIMEOUT);
        }
    };
    if !approved {
        return Err(StatusCode::FORBIDDEN);
    }

    // 写到 Downloads（如果不存在用临时目录兜底）
    let target_dir = directories::UserDirs::new()
        .and_then(|u| u.download_dir().map(std::path::PathBuf::from))
        .unwrap_or_else(std::env::temp_dir);
    let dest = unique_path(&target_dir, &safe_name);
    if let Err(e) = tokio::fs::write(&dest, &plaintext).await {
        tracing::error!(error = %e, "write file failed");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    tracing::info!(dest = %dest.display(), "file saved");
    let _ = ctx.app.emit(
        "file-saved",
        serde_json::json!({
            "path": dest.to_string_lossy(),
            "filename": safe_name,
        }),
    );

    Ok(StatusCode::OK)
}

fn sanitize_filename(name: &str) -> String {
    // 取 basename；去掉斜杠和反斜杠；限制长度
    let base = std::path::Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let cleaned: String = base
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | '\0'))
        .collect();
    if cleaned.is_empty() {
        "file".to_string()
    } else if cleaned.len() > 200 {
        cleaned.chars().take(200).collect()
    } else {
        cleaned
    }
}

/// 如果目标文件已存在，追加 _1, _2 ...
fn unique_path(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let full = dir.join(name);
    if !full.exists() {
        return full;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{}", e)),
        None => (name.to_string(), String::new()),
    };
    for i in 1..1000 {
        let candidate = dir.join(format!("{}_{}{}", stem, i, ext));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{}_{}{}", stem, uuid::Uuid::new_v4(), ext))
}
