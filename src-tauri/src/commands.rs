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

/// 前端可编辑的本机配置（不含 peer_hint —— 那是加入对象不是本机属性）
#[derive(Debug, Deserialize)]
pub struct ConfigUpdate {
    pub port: u16,
    pub password: String,
    pub device_name: String,
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
    app: AppHandle,
    update: ConfigUpdate,
) -> Result<ConfigView, String> {
    let new_cfg: Config = {
        let mut cfg = state.config.write();
        cfg.port = update.port;
        cfg.password = update.password;
        cfg.device_name = update.device_name;
        cfg.clone()
    };
    new_cfg.save().map_err(|e| e.to_string())?;

    // 如果保存后密码已配置并且服务端还没跑，自动起服务端
    let should_auto_listen =
        !new_cfg.password.is_empty() && state.server_shutdown.lock().is_none();
    if should_auto_listen {
        let state_c: Arc<AppState> = Arc::clone(state.inner());
        let app_c = app.clone();
        tauri::async_runtime::spawn(async move {
            auto_listen_on_startup(state_c, app_c).await;
        });
    }

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

/// 如果本机 HTTP server 未启动，就启动它。幂等。
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
    // 给 socket 一点时间绑定
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
}

/// 应用启动时调用：配了密码就自动上线（起 server，状态 → Listening）
pub async fn auto_listen_on_startup(state: Arc<AppState>, app: AppHandle) {
    if state.config.read().password.is_empty() {
        tracing::info!("password not set, skipping auto-listen");
        return;
    }
    start_server_if_needed(Arc::clone(&state), app.clone()).await;
    *state.status.write() = ConnectionStatus::Listening;
    let _ = app.emit("status-updated", ());
    tracing::info!("auto-listen started on port {}", state.config.read().port);
}

/// 启动 HTTP 服务 + 可选地向 target 握手。
///
/// - target 为空：仅上线（等别人来连我）
/// - target 非空：上线 + 主动握手对方
#[tauri::command]
pub async fn join_group(app: AppHandle, target: String) -> Result<(), String> {
    let state: Arc<AppState> = Arc::clone(app.state::<Arc<AppState>>().inner());
    let (password, device_id, device_name, port) = {
        let cfg = state.config.read();
        (
            cfg.password.clone(),
            cfg.device_id.clone(),
            cfg.device_name.clone(),
            cfg.port,
        )
    };
    if password.is_empty() {
        return Err("请先在设置里填写密码".into());
    }
    let target = target.trim().to_string();

    start_server_if_needed(Arc::clone(&state), app.clone()).await;

    // 无 target：只上线等别人连
    if target.is_empty() {
        *state.status.write() = ConnectionStatus::Listening;
        let _ = app.emit("status-updated", ());
        return Ok(());
    }

    *state.status.write() = ConnectionStatus::Connecting;
    let _ = app.emit("status-updated", ());

    match network::client::handshake(&target, &password, &device_id, &device_name, port).await {
        Ok(peer) => {
            let normalized = peer.addr.clone();
            state.peers.upsert(peer);
            crate::state::update_status_connected(&state);
            {
                let mut cfg = state.config.write();
                cfg.peer_hint = Some(normalized);
                let _ = cfg.save();
            }
            let _ = app.emit("status-updated", ());
            Ok(())
        }
        Err(e) => {
            let msg = format!("{:#}", e);
            tracing::warn!(error = %msg, "handshake failed");
            // 握手失败时，服务端仍然是 Listening 状态（已经起来了）
            *state.status.write() = ConnectionStatus::Listening;
            let _ = app.emit("status-updated", ());
            Err(msg)
        }
    }
}

/// 响应握手审批：前端弹框里点「同意/拒绝」后调用
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

/// 返回本机在局域网中对外表现的 IPv4。
///
/// 策略：枚举所有网卡，过滤虚拟/回环/链路本地/benchmark 段，按优先级排序：
///   1. 192.168/16  (家用/办公 WiFi / 小型路由器 NAT)
///   2. 10/8        (大型企业网 / 某些运营商内网)
///   3. 172.16/12   (Docker/WSL/Hyper-V 同段；真实 LAN 较少用)
///   4. 其它非公网
#[tauri::command]
pub fn get_local_ip() -> Option<String> {
    use std::net::IpAddr;

    let ifs = if_addrs::get_if_addrs().ok()?;
    let mut best: Option<(u8, String)> = None;

    for iface in ifs {
        if iface.is_loopback() {
            continue;
        }
        // 过滤常见虚拟网卡名（大小写不敏感）
        let name_lc = iface.name.to_lowercase();
        let looks_virtual = name_lc.contains("vethernet")
            || name_lc.contains("wsl")
            || name_lc.contains("virtualbox")
            || name_lc.contains("vmware")
            || name_lc.contains("hyper-v")
            || name_lc.contains("docker")
            || name_lc.starts_with("utun")    // macOS VPN 隧道
            || name_lc.starts_with("awdl")    // Apple Wireless Direct Link
            || name_lc.starts_with("llw")     // macOS link-local wireless
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

        // 细化优先级
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
