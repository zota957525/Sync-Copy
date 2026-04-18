use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    routing::post,
    Json, Router,
};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

use super::protocol::{ClipboardReq, HandshakeReq, HandshakeResp};
use crate::{
    clipboard::ClipboardCmd,
    history::Source,
    peer::Peer,
    state::AppState,
};

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
    // 密码校验
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

    // 把对方加入本机 peer 表
    ctx.state.peers.upsert(Peer {
        device_id: req.device_id.clone(),
        device_name: req.device_name.clone(),
        addr: req.listen_addr.clone(),
    });
    crate::state::update_status_connected(&ctx.state);
    let _ = ctx.app.emit("status-updated", ());
    tracing::info!(peer = %req.device_name, addr = %req.listen_addr, "peer joined");

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
    // 忽略由自己发出的（不应该，但保险）
    if req.origin_device_id == ctx.state.config.read().device_id {
        return Ok(StatusCode::OK);
    }
    // 去重：seq 校验
    if !ctx.state.seen_seq_and_update(&req.origin_device_id, req.seq) {
        return Ok(StatusCode::OK);
    }

    // 入历史
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

    // 通过 channel 发给剪切板线程：写入系统剪切板 + 抑制下次本地轮询回传
    if let Some(tx) = ctx.state.clipboard_tx.lock().as_ref() {
        let _ = tx.send(ClipboardCmd::SetSuppress(req.text));
    }

    Ok(StatusCode::OK)
}
