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
    pub device_name: String,
    pub peer_hint: Option<String>,
    pub device_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ConfigUpdate {
    pub port: u16,
    pub device_name: String,
}

#[tauri::command]
pub fn get_config(state: State<'_, Arc<AppState>>) -> ConfigView {
    let cfg = state.config.read();
    ConfigView {
        port: cfg.port,
        device_name: cfg.device_name.clone(),
        peer_hint: cfg.peer_hint.clone(),
        device_id: cfg.device_id.clone(),
    }
}

#[tauri::command]
pub fn set_config(
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
    update: ConfigUpdate,
) -> Result<ConfigView, String> {
    let new_cfg: Config = {
        let mut cfg = state.config.write();
        cfg.port = update.port;
        cfg.device_name = update.device_name;
        cfg.clone()
    };
    new_cfg.save().map_err(|e| e.to_string())?;

    // 首次保存后如果服务端还没跑，自动起
    let should_auto_listen = state.server_shutdown.lock().is_none();
    if should_auto_listen {
        let state_c: Arc<AppState> = Arc::clone(state.inner());
        let app_c = app.clone();
        tauri::async_runtime::spawn(async move {
            auto_listen_on_startup(state_c, app_c).await;
        });
    }

    Ok(ConfigView {
        port: new_cfg.port,
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

/// 启动 HTTP server（幂等）
pub async fn start_server_if_needed(state: Arc<AppState>, app: AppHandle) {
    if state.server_shutdown.lock().is_some() {
        return;
    }
    let port = state.config.read().port;
    let (tx, rx) = oneshot::channel::<()>();
    *state.server_shutdown.lock() = Some(tx);
    let state_srv = Arc::clone(&state);
    let app_srv = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = network::server::run(state_srv.clone(), app_srv.clone(), port, rx).await {
            tracing::error!(error = %e, "server exited with error");
            *state_srv.status.write() = ConnectionStatus::Error {
                message: format!("服务端启动失败: {}", e),
            };
            let _ = app_srv.emit("status-updated", ());
        }
        *state_srv.server_shutdown.lock() = None;
    });
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
}

pub async fn auto_listen_on_startup(state: Arc<AppState>, app: AppHandle) {
    start_server_if_needed(Arc::clone(&state), app.clone()).await;
    *state.status.write() = ConnectionStatus::Listening;
    let _ = app.emit("status-updated", ());
    tracing::info!("auto-listen started on port {}", state.config.read().port);
}

/// 尝试和 gossip 列表里的每个 peer 握手（已知的跳过）。并发、异步发起，不阻塞主 join。
fn spawn_gossip_handshakes(
    state: Arc<AppState>,
    app: AppHandle,
    gossip: Vec<network::protocol::PeerPublic>,
    device_id: String,
    device_name: String,
    port: u16,
) {
    for p in gossip {
        // 跳过自己
        if p.device_id == device_id {
            continue;
        }
        // 跳过已知的 peer
        let already_known = state
            .peers
            .snapshot()
            .iter()
            .any(|kp| kp.device_id == p.device_id);
        if already_known {
            continue;
        }

        let state_c = Arc::clone(&state);
        let app_c = app.clone();
        let dev_id = device_id.clone();
        let dev_name = device_name.clone();
        let addr = p.addr.clone();
        let peer_name = p.device_name.clone();
        tauri::async_runtime::spawn(async move {
            tracing::info!(addr = %addr, name = %peer_name, "gossip: attempting handshake");
            match network::client::handshake(&addr, &dev_id, &dev_name, port).await {
                Ok(result) => {
                    let peer_id = result.peer.device_id.clone();
                    state_c.peers.upsert(result.peer);
                    state_c.peer_keys.set(peer_id, result.aes_key);
                    crate::state::update_status_connected(&state_c);
                    let _ = app_c.emit("status-updated", ());
                    // 不递归进一步 gossip —— 简单起见停在一跳
                }
                Err(e) => {
                    tracing::warn!(addr = %addr, error = %e, "gossip handshake failed");
                }
            }
        });
    }
}

/// 启动 HTTP 服务 + 可选地向 target 握手 + M5 gossip 扩展
#[tauri::command]
pub async fn join_group(app: AppHandle, target: String) -> Result<(), String> {
    let state: Arc<AppState> = Arc::clone(app.state::<Arc<AppState>>().inner());
    let (device_id, device_name, port) = {
        let cfg = state.config.read();
        (
            cfg.device_id.clone(),
            cfg.device_name.clone(),
            cfg.port,
        )
    };
    let target = target.trim().to_string();

    start_server_if_needed(Arc::clone(&state), app.clone()).await;

    if target.is_empty() {
        *state.status.write() = ConnectionStatus::Listening;
        let _ = app.emit("status-updated", ());
        return Ok(());
    }

    *state.status.write() = ConnectionStatus::Connecting;
    let _ = app.emit("status-updated", ());

    match network::client::handshake(&target, &device_id, &device_name, port).await {
        Ok(result) => {
            let normalized = result.peer.addr.clone();
            let peer_id = result.peer.device_id.clone();
            state.peers.upsert(result.peer);
            state.peer_keys.set(peer_id, result.aes_key);
            crate::state::update_status_connected(&state);
            {
                let mut cfg = state.config.write();
                cfg.peer_hint = Some(normalized);
                let _ = cfg.save();
            }
            let _ = app.emit("status-updated", ());

            // M5: 自动握手对方告诉我的其它 peer
            spawn_gossip_handshakes(
                Arc::clone(&state),
                app.clone(),
                result.gossip_peers,
                device_id,
                device_name,
                port,
            );
            Ok(())
        }
        Err(e) => {
            let msg = format!("{:#}", e);
            tracing::warn!(error = %msg, "handshake failed");
            *state.status.write() = ConnectionStatus::Listening;
            let _ = app.emit("status-updated", ());
            Err(msg)
        }
    }
}

#[tauri::command]
pub fn leave_group(
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    state.peers.clear();
    state.peer_keys.clear();
    if let Some(tx) = state.server_shutdown.lock().take() {
        let _ = tx.send(());
    }
    *state.status.write() = ConnectionStatus::Idle;
    let _ = app.emit("status-updated", ());
    Ok(())
}

/// 完全退出 app（会先下线）
#[tauri::command]
pub async fn quit_app(app: AppHandle) {
    let state: Arc<AppState> = Arc::clone(app.state::<Arc<AppState>>().inner());
    state.peers.clear();
    state.peer_keys.clear();
    if let Some(tx) = state.server_shutdown.lock().take() {
        let _ = tx.send(());
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    app.exit(0);
}

#[tauri::command]
pub fn respond_handshake(
    state: State<'_, Arc<AppState>>,
    request_id: String,
    accept: bool,
) {
    let mut pending = state.pending_approvals.lock();
    if let Some(tx) = pending.remove(&request_id) {
        let _ = tx.send(accept);
    }
}

#[tauri::command]
pub fn get_local_ip() -> Option<String> {
    use std::net::IpAddr;

    let ifs = if_addrs::get_if_addrs().ok()?;
    let mut best: Option<(u8, String)> = None;

    for iface in ifs {
        if iface.is_loopback() {
            continue;
        }
        let name_lc = iface.name.to_lowercase();
        let looks_virtual = name_lc.contains("vethernet")
            || name_lc.contains("wsl")
            || name_lc.contains("virtualbox")
            || name_lc.contains("vmware")
            || name_lc.contains("hyper-v")
            || name_lc.contains("docker")
            || name_lc.starts_with("utun")
            || name_lc.starts_with("awdl")
            || name_lc.starts_with("llw")
            || name_lc.contains("virtual")
            || name_lc.contains("loopback");
        if looks_virtual {
            continue;
        }

        let IpAddr::V4(v4) = iface.ip() else { continue };
        let [a, b, _, _] = v4.octets();
        if a == 169 && b == 254 {
            continue;
        }
        if a == 198 && (b == 18 || b == 19) {
            continue;
        }

        let priority: u8 = if a == 192 && b == 168 {
            0
        } else if a == 10 {
            1
        } else if a == 172 && (16..=31).contains(&b) {
            2
        } else {
            3
        };

        let ip_str = v4.to_string();
        match &best {
            None => best = Some((priority, ip_str)),
            Some((p, _)) if priority < *p => best = Some((priority, ip_str)),
            _ => {}
        }
    }
    best.map(|(_, s)| s)
}
