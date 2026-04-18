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

/// 启动 HTTP 服务 + 握手到对方机器。target 必填，由前端 Join 对话框传入
#[tauri::command]
pub async fn join_group(app: AppHandle, target: String) -> Result<(), String> {
    let state: Arc<AppState> = Arc::clone(app.state::<Arc<AppState>>().inner());
    let (port, password, device_id, device_name) = {
        let cfg = state.config.read();
        (
            cfg.port,
            cfg.password.clone(),
            cfg.device_id.clone(),
            cfg.device_name.clone(),
        )
    };
    if password.is_empty() {
        return Err("请先在设置里填写密码".into());
    }
    let target = target.trim().to_string();
    if target.is_empty() {
        return Err("请输入对方机器地址（ip:port）".into());
    }

    // 启动本机 server（如果还没启动）
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
            *state_srv.server_shutdown.lock() = None;
        });
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }

    *state.status.write() = ConnectionStatus::Connecting;
    let _ = app.emit("status-updated", ());

    match network::client::handshake(&target, &password, &device_id, &device_name, port).await {
        Ok(peer) => {
            let normalized = peer.addr.clone();
            state.peers.upsert(peer);
            crate::state::update_status_connected(&state);
            // 握手成功，把 target 存为 peer_hint（下次加入对话框默认填这个）
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
            *state.status.write() = ConnectionStatus::Error { message: msg.clone() };
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
/// 之前用 `UdpSocket::connect("8.8.8.8:80")` 让 OS 选路的套路，在有 Clash/VPN
/// 劫持了 DNS 或 198.18.0.0/15 路由时会返回 fake-IP（比如 198.18.0.1），
/// 所以改为枚举所有网卡，过滤回环/链路本地/benchmark 段，优先返回 RFC1918 私网地址。
#[tauri::command]
pub fn get_local_ip() -> Option<String> {
    use std::net::IpAddr;

    let ifs = if_addrs::get_if_addrs().ok()?;
    let mut best: Option<(u8, String)> = None;

    for iface in ifs {
        if iface.is_loopback() {
            continue;
        }
        let IpAddr::V4(v4) = iface.ip() else { continue };
        let [a, b, _, _] = v4.octets();

        // 169.254/16 自动专用（APIPA）—— 未拿到 DHCP 时才有
        if a == 169 && b == 254 {
            continue;
        }
        // 198.18/15 benchmarking 段 —— Clash fake-IP / 网卡测试常用
        if a == 198 && (b == 18 || b == 19) {
            continue;
        }

        // 优先级：RFC1918 私网 > 其它
        let is_private = (a == 192 && b == 168)
            || a == 10
            || (a == 172 && (16..=31).contains(&b));
        let priority: u8 = if is_private { 0 } else { 1 };
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
