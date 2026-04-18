use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::oneshot;

use crate::{
    config::Config,
    history::HistoryItem,
    network,
    peer::Peer,
    state::{AppState, ConnectionStatus},
};

#[derive(Debug, Serialize)]
pub struct ConfigView {
    pub port: u16,
    pub password: String,
    pub device_name: String,
    pub peer_hint: Option<String>,
    pub device_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ConfigUpdate {
    pub port: u16,
    pub password: String,
    pub device_name: String,
    pub peer_hint: Option<String>,
}

#[tauri::command]
pub fn get_config(state: State<'_, Arc<AppState>>) -> ConfigView {
    let cfg = state.config.read();
    ConfigView {
        port: cfg.port,
        password: cfg.password.clone(),
        device_name: cfg.device_name.clone(),
        peer_hint: cfg.peer_hint.clone(),
        device_id: cfg.device_id.clone(),
    }
}

#[tauri::command]
pub fn set_config(
    state: State<'_, Arc<AppState>>,
    update: ConfigUpdate,
) -> Result<ConfigView, String> {
    let new_cfg: Config = {
        let mut cfg = state.config.write();
        cfg.port = update.port;
        cfg.password = update.password;
        cfg.device_name = update.device_name;
        cfg.peer_hint = update.peer_hint;
        cfg.clone()
    };
    new_cfg.save().map_err(|e| e.to_string())?;
    Ok(ConfigView {
        port: new_cfg.port,
        password: new_cfg.password,
        device_name: new_cfg.device_name,
        peer_hint: new_cfg.peer_hint,
        device_id: new_cfg.device_id,
    })
}

#[tauri::command]
pub fn get_status(state: State<'_, Arc<AppState>>) -> ConnectionStatus {
    state.status.read().clone()
}

#[tauri::command]
pub fn get_peers(state: State<'_, Arc<AppState>>) -> Vec<Peer> {
    state.peers.snapshot()
}

#[tauri::command]
pub fn get_history(state: State<'_, Arc<AppState>>) -> Vec<HistoryItem> {
    state.history.snapshot()
}

#[tauri::command]
pub fn delete_history_item(
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
    id: String,
) {
    if state.history.remove(&id) {
        let _ = app.emit("history-updated", ());
    }
}

#[tauri::command]
pub fn clear_history(state: State<'_, Arc<AppState>>, app: AppHandle) {
    state.history.clear();
    let _ = app.emit("history-updated", ());
}

/// 启动 HTTP 服务 + 可选地向 peer_hint 握手
#[tauri::command]
pub async fn join_group(app: AppHandle) -> Result<(), String> {
    let state: Arc<AppState> = Arc::clone(app.state::<Arc<AppState>>().inner());
    let (port, password, peer_hint, device_id, device_name) = {
        let cfg = state.config.read();
        (
            cfg.port,
            cfg.password.clone(),
            cfg.peer_hint.clone(),
            cfg.device_id.clone(),
            cfg.device_name.clone(),
        )
    };
    if password.is_empty() {
        return Err("请先在设置里填写密码".into());
    }

    // 启动 server（如果还没启动）
    let already_running = state.server_shutdown.lock().is_some();
    if !already_running {
        let (tx, rx) = oneshot::channel::<()>();
        *state.server_shutdown.lock() = Some(tx);
        let state_srv = Arc::clone(&state);
        let app_srv = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = network::server::run(state_srv.clone(), app_srv, port, rx).await {
                tracing::error!(error = %e, "server exited with error");
                *state_srv.status.write() = ConnectionStatus::Error {
                    message: format!("服务端启动失败: {}", e),
                };
            }
            // server 退出后清空 shutdown 句柄
            *state_srv.server_shutdown.lock() = None;
        });
        // 给 server 一点点时间绑定 socket
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }

    *state.status.write() = ConnectionStatus::Listening;
    let _ = app.emit("status-updated", ());

    // 握手到 peer_hint
    if let Some(target) = peer_hint.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        *state.status.write() = ConnectionStatus::Connecting;
        let _ = app.emit("status-updated", ());
        match network::client::handshake(target, &password, &device_id, &device_name, port).await {
            Ok(mut peer) => {
                // 对方返回的 Peer 使用用户填的 addr 作为回连地址
                peer.addr = target.to_string();
                state.peers.upsert(peer);
                crate::state::update_status_connected(&state);
                let _ = app.emit("status-updated", ());
            }
            Err(e) => {
                let msg = e.to_string();
                *state.status.write() = ConnectionStatus::Error { message: msg.clone() };
                let _ = app.emit("status-updated", ());
                return Err(msg);
            }
        }
    }

    Ok(())
}

#[tauri::command]
pub fn leave_group(
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    state.peers.clear();
    if let Some(tx) = state.server_shutdown.lock().take() {
        let _ = tx.send(());
    }
    *state.status.write() = ConnectionStatus::Idle;
    let _ = app.emit("status-updated", ());
    Ok(())
}
