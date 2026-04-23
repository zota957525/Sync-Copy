use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::oneshot;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

use crate::{
    clipboard::ClipboardCmd,
    config::Config,
    history::{sha256_hex, HistoryItem, HistoryPayload, Source},
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
    let Some(removed) = state.history.remove(&id) else {
        return;
    };
    let _ = app.emit("history-updated", ());
    // 有 content_hash 才广播（文件在发送失败等情况可能没 hash）
    if let Some(hash) = removed.content_hash {
        let state_c: Arc<AppState> = Arc::clone(state.inner());
        tauri::async_runtime::spawn(async move {
            network::client::broadcast_delete(state_c, hash).await;
        });
    }
}

#[tauri::command]
pub fn clear_history(state: State<'_, Arc<AppState>>, app: AppHandle) {
    let peer_count = state.peers.count();
    tracing::info!(peer_count, "clear_history invoked, will broadcast to peers");
    state.history.clear();
    let _ = app.emit("history-updated", ());
    let state_c: Arc<AppState> = Arc::clone(state.inner());
    tauri::async_runtime::spawn(async move {
        network::client::broadcast_clear_history(state_c).await;
        tracing::info!("broadcast_clear_history finished");
    });
}

/// 把某条历史条目重新写回系统剪切板（文本或图片都支持）
#[tauri::command]
pub fn recopy_history_item(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    let items = state.history.snapshot();
    let item = items
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| "历史条目不存在".to_string())?;
    let tx = state
        .clipboard_tx
        .lock()
        .clone()
        .ok_or_else(|| "剪切板子线程未就绪".to_string())?;
    match item.payload {
        HistoryPayload::Text { text } => {
            let _ = tx.send(ClipboardCmd::SetTextSuppress(text));
        }
        HistoryPayload::Image {
            width,
            height,
            data_url,
        } => {
            let b64 = data_url
                .split_once(',')
                .map(|(_, b)| b)
                .ok_or_else(|| "data_url 格式异常".to_string())?;
            let png = B64.decode(b64).map_err(|e| e.to_string())?;
            let _ = tx.send(ClipboardCmd::SetImageSuppress { png, width, height });
        }
        HistoryPayload::File { .. } => {
            // 文件不走剪切板，点击应调 reveal_file 而不是 recopy
            return Err("文件条目不支持复制到剪切板".into());
        }
    }
    Ok(())
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
pub async fn leave_group(app: AppHandle) -> Result<(), String> {
    let state: Arc<AppState> = Arc::clone(app.state::<Arc<AppState>>().inner());
    // 先给 peer 发离线通知（best-effort，最多等 1.5 秒）
    let state_c = Arc::clone(&state);
    let _ = tokio::time::timeout(
        std::time::Duration::from_millis(1500),
        network::client::broadcast_leave(state_c),
    )
    .await;
    state.peers.clear();
    state.peer_keys.clear();
    state.approved_device_ids.write().clear();
    state.banned_device_ids.write().clear();
    state.forwarded_approvals.lock().clear();
    if let Some(tx) = state.server_shutdown.lock().take() {
        let _ = tx.send(());
    }
    *state.status.write() = ConnectionStatus::Idle;
    let _ = app.emit("status-updated", ());
    Ok(())
}

/// 完全退出 app（会先发离线通知 + 清理状态）
#[tauri::command]
pub async fn quit_app(app: AppHandle) {
    let state: Arc<AppState> = Arc::clone(app.state::<Arc<AppState>>().inner());
    // 广播离线（给组内其它 peer 即时更新计数）
    let state_c = Arc::clone(&state);
    let _ = tokio::time::timeout(
        std::time::Duration::from_millis(1500),
        network::client::broadcast_leave(state_c),
    )
    .await;
    state.peers.clear();
    state.peer_keys.clear();
    state.approved_device_ids.write().clear();
    state.banned_device_ids.write().clear();
    state.forwarded_approvals.lock().clear();
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
    // 情形 A：这是本机发起的审批（握手直接进到我），tx 在本机
    if let Some(tx) = state.pending_approvals.lock().remove(&request_id) {
        let _ = tx.send(accept);
        return;
    }
    // 情形 B：这是其它 peer 转发给我的审批，我要把决定送回给发起方
    let fwd = state.forwarded_approvals.lock().remove(&request_id);
    if let Some(info) = fwd {
        // 在 peers 里找 origin 的 addr
        let origin_addr = state
            .peers
            .snapshot()
            .into_iter()
            .find(|p| p.device_id == info.origin_device_id)
            .map(|p| p.addr);
        let Some(addr) = origin_addr else {
            tracing::warn!(origin = %info.origin_device_id, "forwarded approval decision: origin peer unknown");
            return;
        };
        let state_c: Arc<AppState> = Arc::clone(state.inner());
        tauri::async_runtime::spawn(async move {
            network::client::send_approval_decision(state_c, addr, request_id, accept).await;
        });
    }
}

#[tauri::command]
pub fn respond_file_save(
    state: State<'_, Arc<AppState>>,
    request_id: String,
    accept: bool,
) {
    let mut pending = state.pending_file_saves.lock();
    if let Some(meta) = pending.remove(&request_id) {
        let _ = meta.tx.send(accept);
    }
}

const MAX_SEND_SIZE: u64 = 5 * 1024 * 1024;

/// 前端拖文件到浮窗后调用，把每个文件发给所有 peer
#[tauri::command]
pub async fn send_files(app: AppHandle, paths: Vec<String>) -> Result<String, String> {
    let state: Arc<AppState> = Arc::clone(app.state::<Arc<AppState>>().inner());
    if state.peers.count() == 0 {
        return Err("还没有连接的设备".into());
    }

    let mut reports = Vec::new();
    for raw in paths {
        let path = std::path::PathBuf::from(&raw);
        if !path.is_file() {
            reports.push(format!("{}: 不是文件或不存在，跳过", raw));
            continue;
        }
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        let meta = match tokio::fs::metadata(&path).await {
            Ok(m) => m,
            Err(e) => {
                reports.push(format!("{}: 读取元信息失败 ({})", filename, e));
                continue;
            }
        };
        if meta.len() > MAX_SEND_SIZE {
            reports.push(format!("{}: 超过 5MB 上限 ({} 字节)", filename, meta.len()));
            continue;
        }
        let bytes = match tokio::fs::read(&path).await {
            Ok(b) => b,
            Err(e) => {
                reports.push(format!("{}: 读取失败 ({})", filename, e));
                continue;
            }
        };
        let content_hash = sha256_hex(&bytes);
        let size = bytes.len() as u64;
        let (ok, total) =
            network::client::broadcast_file(Arc::clone(&state), filename.clone(), bytes).await;
        reports.push(format!("{}: 已送达 {}/{} 台", filename, ok, total));
        // 发送端也把文件写进历史（便于删除同步和查看）
        let _ = state.history.push_file(
            filename.clone(),
            size,
            Some(path.to_string_lossy().to_string()),
            "sent",
            None,
            Some(content_hash),
            Source::Local,
        );
        let _ = app.emit("history-updated", ());
    }
    Ok(reports.join("\n"))
}

/// 隐藏浮窗（通过托盘图标可重新显示）
#[tauri::command]
pub fn hide_window(app: AppHandle) {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("main") {
        // 防御：如果窗口当前已经滑到屏幕外（吸附隐藏状态），先拉回屏幕，
        // 这样下次 show 不会直接显示在屏幕外看不到
        crate::ensure_on_screen(&w);
        let _ = w.hide();
    }
}

/// 在系统文件管理器里定位文件（已保存的文件点条目时调用）
#[tauri::command]
pub fn reveal_file(path: String) -> Result<(), String> {
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("文件不存在: {}", path));
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&p)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&p)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = p;
        return Err("当前平台不支持".into());
    }
    Ok(())
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
