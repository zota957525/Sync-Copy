use std::{sync::Arc, time::Duration};

use axum::{
    extract::State,
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde_json::json;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

use super::protocol::{ClipboardReq, HandshakeReq, HandshakeResp};
use crate::{
    clipboard::ClipboardCmd,
    history::Source,
    peer::Peer,
    state::AppState,
};

/// 前端审批超时：服务端收到握手后最多等这么久
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
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = shutdown.await;
            tracing::info!("HTTP server shutting down");
        })
        .await?;
    Ok(())
}

async fn handle_handshake(
    State(ctx): State<ServerCtx>,
    Json(req): Json<HandshakeReq>,
) -> Result<Json<HandshakeResp>, StatusCode> {
    // 1. 密码校验（快速，不打扰用户）
    let (my_id, my_name, my_pwd) = {
        let cfg = ctx.state.config.read();
        (
            cfg.device_id.clone(),
            cfg.device_name.clone(),
            cfg.password.clone(),
        )
    };
    if req.password != my_pwd {
        tracing::warn!(remote = %req.device_name, "handshake password mismatch");
        return Err(StatusCode::UNAUTHORIZED);
    }
    // 自己不能加入自己
    if req.device_id == my_id {
        return Err(StatusCode::CONFLICT);
    }
    // 已经是已知 peer 就直接通过（幂等）
    if ctx
        .state
        .peers
        .snapshot()
        .iter()
        .any(|p| p.device_id == req.device_id)
    {
        return Ok(Json(HandshakeResp {
            device_id: my_id,
            device_name: my_name,
        }));
    }

    // 2. 生成 request_id，挂 oneshot，发事件给前端
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
    tracing::info!(peer = %req.device_name, "handshake pending user approval");

    // 3. 最多等 30 秒
    let approved = match tokio::time::timeout(APPROVAL_TIMEOUT, rx).await {
        Ok(Ok(ok)) => ok,
        _ => {
            // 超时或 sender drop：清理挂起项，返回 408
            ctx.state.pending_approvals.lock().remove(&request_id);
            tracing::warn!(peer = %req.device_name, "handshake approval timed out");
            return Err(StatusCode::REQUEST_TIMEOUT);
        }
    };

    if !approved {
        tracing::info!(peer = %req.device_name, "handshake rejected by user");
        return Err(StatusCode::FORBIDDEN);
    }

    // 4. 通过：登记 peer
    ctx.state.peers.upsert(Peer {
        device_id: req.device_id.clone(),
        device_name: req.device_name.clone(),
        addr: req.listen_addr.clone(),
    });
    crate::state::update_status_connected(&ctx.state);
    let _ = ctx.app.emit("status-updated", ());
    tracing::info!(peer = %req.device_name, "peer approved and joined");

    Ok(Json(HandshakeResp {
        device_id: my_id,
        device_name: my_name,
    }))
}

async fn handle_clipboard(
    State(ctx): State<ServerCtx>,
    Json(req): Json<ClipboardReq>,
) -> Result<StatusCode, StatusCode> {
    let my_pwd = ctx.state.config.read().password.clone();
    if req.password != my_pwd {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if req.origin_device_id == ctx.state.config.read().device_id {
        return Ok(StatusCode::OK);
    }
    if !ctx.state.seen_seq_and_update(&req.origin_device_id, req.seq) {
        return Ok(StatusCode::OK);
    }

    if ctx
        .state
        .history
        .push(
            req.text.clone(),
            Source::Remote {
                device_name: req.origin_device_name.clone(),
            },
        )
        .is_some()
    {
        let _ = ctx.app.emit("history-updated", ());
    }

    if let Some(tx) = ctx.state.clipboard_tx.lock().as_ref() {
        let _ = tx.send(ClipboardCmd::SetSuppress(req.text));
    }

    Ok(StatusCode::OK)
}
