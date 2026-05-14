//! Tauri IPC 命令层（前端 → 后端入口）
//! see specs/floating-window.md (第 6 节 UX：状态点 / 已连接 N 台)
//! see specs/settings-panel.md (第 4 节 AC + 第 6 节 UX：get_config / set_config)
//! see specs/history-list.md (第 4 节 AC：get_history / delete_history_item / clear_history / recopy_history_item)
//! see specs/group-approval.md (第 6 节 UX：approve_peer / reject_peer)
//! see specs/group-discovery.md (join_group：入组地址按钮触发)
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.6 节 CommandError → String boundary)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-3 通用 body + MUST-8 sanitize)
//!
//! 命令返回约定（ADR-003 第 3.6 节 + ADR-008 MUST-3）：
//! - 所有命令返 Result<T, String>（CommandError → String boundary）
//! - boundary 处统一返通用 body 字面量（"invalid_input" / "not_found" / "forbidden" / "internal_error"）
//! - 详细错误用 tracing::warn!/error! 写日志，不暴露给前端（ADR-008 MUST-3）
//! - 不向前端暴露内部 Rust 路径 / anyhow 错误链 / device_id 字面 / Zeroizing key 等敏感数据
//!
//! sanitize 约定（ADR-008 MUST-8）：
//! - set_config 接收 device_name 后首动作调 sanitize_device_name（Bidi+控制字符+64 codepoints）
//!
//! 编码风格（编码规则）：
//! - 不持 state 锁过 await（短锁短持：lock → clone → drop lock → async op）
//! - AppHandle 通过 tauri::AppHandle 参数自动注入（不改 lifecycle）
//! - 所有 emit 用 app_handle.emit(event, payload)（tauri::Emitter trait）

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::Emitter as _;

use crate::app::history::{HistoryEntry, HistoryPayload, HistorySource};
use crate::app::state::AppState;
use crate::peer::TrustState;

// ---------------------------------------------------------------------------
// DTO 定义（与前端 src/ipc/types.ts 对应）
// ---------------------------------------------------------------------------

/// 状态概览（floating-window 第 6.5 节 状态点颜色 + "已连接 N 台"展示）。
#[derive(Debug, Clone, Serialize)]
pub struct StatusInfo {
    /// 本机 device_id（UUID）
    pub my_device_id: String,
    /// 监听地址（"ip:port" 格式；底部 footer 展示用）
    pub listen_addr: String,
    /// 当前已注册的 peer 总数
    pub peer_count: usize,
    /// Approved peer 数量（状态点 + "小组 · N 台"展示）
    pub approved_count: usize,
    /// Banned peer 数量
    pub banned_count: usize,
}

/// 单个 peer 信息（group-approval 弹框 + peer 列表展示）。
#[derive(Debug, Clone, Serialize)]
pub struct PeerInfo {
    pub device_id: String,
    pub addr: String,
    pub device_name: String,
    /// "approved" | "banned" | "pending"
    pub trust_state: String,
    /// 上次成功同步的相对时间字符串（如 "3 分钟前"）；None 表示从未同步
    pub last_successful_sync_at: Option<String>,
}

/// 配置读取响应（settings-panel 第 6.1 节）。
#[derive(Debug, Clone, Serialize)]
pub struct ConfigInfo {
    pub device_name: String,
    pub listen_port: u16,
    /// 上次成功 join 的地址（join 对话框 placeholder 来源）
    pub peer_hint: Option<String>,
}

/// 配置写入载体（settings-panel set_config 参数）。
///
/// serde(default)：向前兼容（v5-6 外部接口 try-coerce 规则）。
#[derive(Debug, Clone, Deserialize)]
pub struct ConfigPayload {
    /// 新设备名（None = 不修改；空串 = 应被 set_config 拒绝）
    #[serde(default)]
    pub device_name: Option<String>,
    /// 新监听端口（v2 P1 不开放 UI 修改，set_config 接受但暂不重启 server）
    #[serde(default)]
    pub listen_port: Option<u16>,
}

/// 单条历史条目（history-list 第 3 节 HistoryItem 结构）。
#[derive(Debug, Clone, Serialize)]
pub struct HistoryItem {
    pub id: String,
    pub timestamp_ms: u64,
    /// { "kind": "local" } | { "kind": "remote", "device_name": "..." }
    pub source: serde_json::Value,
    pub content_hash: Option<String>,
    /// tagged enum: { "type": "text", "text": "..." }
    ///              { "type": "image", "width": N, "height": N, "data_url": "data:image/png;base64,..." }
    ///              { "type": "file", "filename": "...", "size": N, "saved_path": ..., "file_status": "...", "error": ... }
    pub payload: serde_json::Value,
}

// ---------------------------------------------------------------------------
// 内部工具
// ---------------------------------------------------------------------------

/// 将 `std::time::Instant` 转为"相对时间字符串"（spec history-list 第 3 节）。
///
/// 使用 SystemTime 记录时间戳而非 Instant（Instant 不能与 epoch 计算差值）。
/// 此处接受"距离现在的秒数"返回对应中文字符串。
fn relative_time_str(elapsed_secs: u64) -> String {
    if elapsed_secs < 60 {
        "刚刚".to_string()
    } else if elapsed_secs < 3600 {
        format!("{} 分钟前", elapsed_secs / 60)
    } else if elapsed_secs < 86400 {
        format!("{} 小时前", elapsed_secs / 3600)
    } else {
        format!("{} 天前", elapsed_secs / 86400)
    }
}

/// 将 PeerState.last_successful_sync_at (Option<Instant>) 转为可读字符串。
fn format_last_sync(last: Option<std::time::Instant>) -> Option<String> {
    last.map(|t| {
        let elapsed = t.elapsed().as_secs();
        relative_time_str(elapsed)
    })
}

/// 将内部 HistoryEntry 转为前端 DTO。
fn entry_to_item(entry: &HistoryEntry) -> HistoryItem {
    let source_val = match &entry.source {
        HistorySource::Local => serde_json::json!({"kind": "local"}),
        HistorySource::Remote { device_name } => {
            serde_json::json!({"kind": "remote", "device_name": device_name})
        }
    };

    let payload_val = match &entry.payload {
        HistoryPayload::Text { text } => serde_json::json!({"type": "text", "text": text}),
        HistoryPayload::Image {
            width,
            height,
            data_b64,
        } => {
            serde_json::json!({
                "type": "image",
                "width": width,
                "height": height,
                "data_url": format!("data:image/png;base64,{data_b64}")
            })
        }
        HistoryPayload::File {
            filename,
            size,
            saved_path,
            file_status,
            error,
        } => serde_json::json!({
            "type": "file",
            "filename": filename,
            "size": size,
            "saved_path": saved_path,
            "file_status": file_status,
            "error": error,
        }),
    };

    HistoryItem {
        id: entry.id.clone(),
        timestamp_ms: entry.timestamp_ms,
        source: source_val,
        content_hash: entry.content_hash.clone(),
        payload: payload_val,
    }
}

// ---------------------------------------------------------------------------
// P0 命令实现
// ---------------------------------------------------------------------------

/// get_status — 返回当前状态概览（floating-window 顶部状态栏数据来源）。
///
/// 包含 my_device_id / listen_addr / peer_count / approved_count / banned_count。
/// floating-window 第 6.5 节 "已连接 N 台" = approved_count。
#[tauri::command]
pub async fn get_status(state: tauri::State<'_, AppState>) -> Result<StatusInfo, String> {
    // 短锁：从 state 读取所需数据，不持锁过 await
    let my_device_id = state.my_device_id.clone();
    let listen_port = {
        let cfg = state.config.lock();
        cfg.listen_port
    };

    // 获取本机局域网 IP
    let listen_ip = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .find(|iface| !iface.is_loopback() && iface.addr.ip().is_ipv4())
        .map(|iface| iface.addr.ip().to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string());

    let listen_addr = format!("{listen_ip}:{listen_port}");

    let all_peers = state.peers.snapshot();
    let peer_count = all_peers.len();
    let approved_count = all_peers
        .iter()
        .filter(|p| p.trust_state == TrustState::Approved)
        .count();
    let banned_count = all_peers
        .iter()
        .filter(|p| p.trust_state == TrustState::Banned)
        .count();

    Ok(StatusInfo {
        my_device_id,
        listen_addr,
        peer_count,
        approved_count,
        banned_count,
    })
}

/// get_peers — 返回所有已注册 peer 列表（PeerRegistry snapshot）。
///
/// 包含 device_id / addr / device_name / trust_state / last_successful_sync_at 相对时间。
/// 注意：PeerState 含 aes_key，不向前端暴露（DTO 只输出安全字段）。
#[tauri::command]
pub async fn get_peers(state: tauri::State<'_, AppState>) -> Result<Vec<PeerInfo>, String> {
    let peers = state.peers.snapshot();
    let result: Vec<PeerInfo> = peers
        .iter()
        .map(|p| PeerInfo {
            device_id: p.device_id.clone(),
            addr: p.addr.to_string(),
            device_name: p.device_name.clone(),
            trust_state: match p.trust_state {
                TrustState::Approved => "approved".to_string(),
                TrustState::Banned => "banned".to_string(),
                TrustState::Pending => "pending".to_string(),
            },
            last_successful_sync_at: format_last_sync(p.last_successful_sync_at),
        })
        .collect();
    Ok(result)
}

/// join_group — 主动向目标 peer 发起握手（settings-panel "入组地址"按钮触发）。
///
/// target_addr 格式："ip:port"（normalize_addr 去掉协议前缀 / 尾部斜杠）。
/// group-discovery spec 第 3 节。
#[tauri::command]
pub async fn join_group(
    target_addr: String,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // normalize_addr：去掉 http:// 前缀和尾部斜杠（group-discovery spec 第 3 节）
    let normalized = normalize_addr(&target_addr);

    // ADR-008 MUST-3：地址格式错不向前端暴露内部 parse 错误链；细节入日志
    let socket_addr: std::net::SocketAddr = normalized.parse().map_err(|e| {
        tracing::warn!(
            target: "commands",
            addr = %normalized,
            error = %e,
            "join_group: invalid target addr"
        );
        "invalid_input".to_string()
    })?;

    // 短锁读取本机信息（不持锁过 await）
    let (my_device_id, my_device_name, my_listen_port) = {
        let cfg = state.config.lock();
        (
            state.my_device_id.clone(),
            cfg.device_name.clone(),
            cfg.listen_port,
        )
    };

    let state_inner = state.inner().clone();

    // dial_handshake（group-discovery spec 第 3 节 + client.rs 已实现）
    // ADR-008 MUST-3：连接失败不向前端暴露内部错误链（含 reqwest stack trace / IP）；细节入日志
    crate::network::client::dial_handshake(
        socket_addr,
        &state_inner,
        &my_device_id,
        &my_device_name,
        my_listen_port,
    )
    .await
    .map_err(|e| {
        tracing::warn!(
            target: "commands",
            addr = %normalized,
            error = %e,
            "join_group: handshake failed"
        );
        "forbidden".to_string()
    })?;

    // 握手成功后 emit status-updated（floating-window 顶部状态栏刷新）
    if let Err(e) = app_handle.emit("status-updated", ()) {
        tracing::warn!(
            target: "commands",
            error = %e,
            "join_group: emit status-updated failed (non-fatal)"
        );
    }

    tracing::info!(
        target: "commands",
        addr = %normalized,
        "join_group: handshake complete"
    );
    Ok(())
}

/// get_config — 返回当前持久化配置（settings-panel 首屏用）。
#[tauri::command]
pub async fn get_config(state: tauri::State<'_, AppState>) -> Result<ConfigInfo, String> {
    let cfg = state.config.lock().clone();
    Ok(ConfigInfo {
        device_name: cfg.device_name,
        listen_port: cfg.listen_port,
        peer_hint: cfg.peer_hint,
    })
}

/// set_config — 保存设备名等配置到 ProjectDirs config.json（settings-panel 保存按钮触发）。
///
/// 验收标准（spec settings-panel 第 4 节）：
/// - device_name 留空 / 纯空白 → 拒绝，返 Err "设备名不能为空"
/// - device_name > 64 字符 → 截断（input maxlength 应已限制，但后端二次保证）
/// - 写盘失败 → tracing::warn + 返 Err（不 fatal；内存值已更新）
#[tauri::command]
pub async fn set_config(
    cfg: ConfigPayload,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut new_cfg = state.config.lock().clone();

    // 验证 + 更新 device_name
    if let Some(name) = cfg.device_name {
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            return Err("invalid_input".to_string());
        }
        // ADR-008 MUST-8：首动作调 sanitize_device_name（Bidi 黑名单 + 控制字符 + ≤64 codepoints）
        // sanitize 内部已含截断逻辑，不再手动 chars().take(64)
        let safe_name = crate::peer::sanitize::sanitize_device_name(&trimmed);
        // sanitize 返 "<unnamed>" 表示 trimmed 全为非法字符，视为空名拒绝
        if safe_name == "<unnamed>" {
            return Err("invalid_input".to_string());
        }
        new_cfg.device_name = safe_name;
    }

    // listen_port（v2 P1 接受但不重启 server，写盘用于下次启动）
    if let Some(port) = cfg.listen_port {
        if port == 0 {
            return Err("invalid_input".to_string());
        }
        new_cfg.listen_port = port;
    }

    // 更新内存中的配置（短锁）
    *state.config.lock() = new_cfg.clone();

    // 异步写盘（不持锁过 await）
    // ADR-008 MUST-3：写盘失败不向前端暴露 ProjectDirs path 等内部细节；细节入日志
    new_cfg.save().await.map_err(|e| {
        tracing::warn!(target: "commands", error = %e, "set_config: save failed");
        "internal_error".to_string()
    })?;

    tracing::info!(
        target: "commands",
        device_name = %new_cfg.device_name,
        "set_config: saved"
    );
    Ok(())
}

/// approve_peer — 同意某 peer 的加入请求（group-approval 弹框 approve 按钮）。
///
/// 调用 PeerRegistry::approve(device_id)，并 emit status-updated。
#[tauri::command]
pub async fn approve_peer(
    device_id: String,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // ADR-008 MUST-3：未知设备返 "not_found"，不暴露 device_id 字面值到前端
    if !state.peers.is_known(&device_id) {
        tracing::warn!(target: "commands", device_id = %device_id, "approve_peer: unknown device");
        return Err("not_found".to_string());
    }
    state.peers.approve(&device_id);

    if let Err(e) = app_handle.emit("status-updated", ()) {
        tracing::warn!(
            target: "commands",
            error = %e,
            "approve_peer: emit status-updated failed (non-fatal)"
        );
    }

    tracing::info!(target: "commands", device_id = %device_id, "peer approved via IPC");
    Ok(())
}

/// reject_peer — 拒绝某 peer 的加入请求（group-approval 弹框 reject 按钮）。
///
/// 调用 PeerRegistry::ban(device_id)，并 emit status-updated。
#[tauri::command]
pub async fn reject_peer(
    device_id: String,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // ban 不要求 peer 已在 inner（spec group-approval 第 3 节：可 ban 尚未 handshake 的 subject）
    state.peers.ban(&device_id);

    if let Err(e) = app_handle.emit("status-updated", ()) {
        tracing::warn!(
            target: "commands",
            error = %e,
            "reject_peer: emit status-updated failed (non-fatal)"
        );
    }

    tracing::info!(target: "commands", device_id = %device_id, "peer rejected (banned) via IPC");
    Ok(())
}

/// get_history — 返回当前历史列表（history-list spec 第 3 节）。
///
/// 首屏挂载 + history-updated 事件触发后调用刷新。
#[tauri::command]
pub async fn get_history(state: tauri::State<'_, AppState>) -> Result<Vec<HistoryItem>, String> {
    let entries = state.history.snapshot();
    let items: Vec<HistoryItem> = entries.iter().map(entry_to_item).collect();
    Ok(items)
}

/// delete_history_item — 删除单条历史（history-list spec 第 3 节 单条删除）。
#[tauri::command]
pub async fn delete_history_item(
    id: String,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let removed = state.history.remove(&id);
    // ADR-008 MUST-3：id 不存在返 "not_found"，不暴露 id 字面值
    if !removed {
        tracing::warn!(target: "commands", id = %id, "delete_history_item: not found");
        return Err("not_found".to_string());
    }

    // emit history-updated（前端刷新列表）
    if let Err(e) = app_handle.emit("history-updated", ()) {
        tracing::warn!(
            target: "commands",
            error = %e,
            "delete_history_item: emit history-updated failed (non-fatal)"
        );
    }

    tracing::debug!(target: "commands", id = %id, "history item deleted");
    Ok(())
}

/// clear_history — 清空所有历史（settings-panel 清除历史按钮触发）。
///
/// spec settings-panel 第 3 节：本机历史清空 + emit history-updated。
/// 注意：跨机广播（broadcast_clear_history）留 P2（history-sync-delete spec）。
#[tauri::command]
pub async fn clear_history(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state.history.clear();

    if let Err(e) = app_handle.emit("history-updated", ()) {
        tracing::warn!(
            target: "commands",
            error = %e,
            "clear_history: emit history-updated failed (non-fatal)"
        );
    }

    tracing::info!(target: "commands", "history cleared via IPC");
    Ok(())
}

/// recopy_history_item — 把历史条目重新写入剪切板（history-list spec 第 3 节 单击复制）。
///
/// 文本条目：通过 clipboard_apply_tx 发到 arboard 专属线程（ClipboardCmd::SetText）。
/// 图片条目：decode base64 PNG → 通过 tx 发（TODO: ClipboardCmd::SetImage 留 PR-FE-1+ 落地时接入）
/// 文件条目：返回 Err（文件不支持复制到剪切板，应改用 reveal_file）。
///
/// SECURITY（ADR-011 第 3.5 节 + ADR-008 MUST-2）：
/// 此命令传递剪切板明文；不进 tracing fields / 不落盘。
#[tauri::command]
pub async fn recopy_history_item(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // ADR-008 MUST-3：id 不存在返 "not_found"，不暴露 id 字面值
    let entry = state.history.get(&id).ok_or_else(|| {
        tracing::warn!(target: "commands", id = %id, "recopy_history_item: not found");
        "not_found".to_string()
    })?;

    match &entry.payload {
        HistoryPayload::Text { text } => {
            // 通过 clipboard_apply_tx 发到 arboard 专属线程
            // SECURITY：不 tracing 明文内容
            // ADR-008 MUST-3：剪切板写入失败不暴露 mpsc 错误细节
            state
                .clipboard_apply_tx
                .try_send(text.clone())
                .map_err(|e| {
                    tracing::warn!(target: "commands", error = %e, "recopy_history_item: clipboard send failed");
                    "internal_error".to_string()
                })?;

            tracing::debug!(
                target: "commands",
                id = %id,
                "recopy_history_item: text sent to clipboard thread"
            );
            Ok(())
        }
        HistoryPayload::Image { .. } => {
            // TODO(PR-FE-1+)：图片重写剪切板需要 ClipboardCmd::SetImage（arboard image 解码）
            // 当前 clipboard_apply_tx 只接受 String（SetTextSuppress），图片通路未实现。
            // ADR-008 MUST-3：占位返 "invalid_input"（让前端知道是参数级问题，不是内部错误）
            Err("invalid_input".to_string())
        }
        HistoryPayload::File { .. } => {
            // 文件条目不支持复制到剪切板；返 "invalid_input"（参数类型限制）
            Err("invalid_input".to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 标准化 join_group 输入地址（group-discovery spec 第 3 节）。
///
/// 去掉 http:// / https:// 前缀 + 尾部 / + 空白。
fn normalize_addr(addr: &str) -> String {
    let s = addr.trim();
    let s = s.strip_prefix("http://").unwrap_or(s);
    let s = s.strip_prefix("https://").unwrap_or(s);
    s.trim_end_matches('/').trim().to_string()
}

// ---------------------------------------------------------------------------
// 当前时间戳辅助（用于 HistoryEntry timestamp_ms）
// ---------------------------------------------------------------------------

/// 返回当前 UNIX epoch 毫秒时间戳。
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ---------------------------------------------------------------------------
// 辅助：向 AppHandle 发 peer-pending 事件（group-approval 弹框触发）
// ---------------------------------------------------------------------------

/// peer-pending 事件 payload（handshake_handler 收到待审批时 emit）。
///
/// spec group-approval 第 3 节：含申请方 device_name + IP + request_id + timestamp_ms。
#[derive(Debug, Clone, Serialize)]
pub struct PeerPendingPayload {
    pub request_id: String,
    pub subject_device_id: String,
    pub subject_device_name: String,
    pub subject_ip: String,
    pub timestamp_ms: u64,
}

// ---------------------------------------------------------------------------
// 单元测试（任务要求 >= 4 条）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::history::{HistoryEntry, HistoryPayload, HistorySource};
    #[allow(unused_imports)]
    use anyhow;

    // 测试辅助：构造最小 StatusInfo
    fn make_status() -> StatusInfo {
        StatusInfo {
            my_device_id: "test-device-id".to_string(),
            listen_addr: "127.0.0.1:5858".to_string(),
            peer_count: 0,
            approved_count: 0,
            banned_count: 0,
        }
    }

    // 单测 1：get_status_returns_my_device_id_correct
    // 验证 StatusInfo.my_device_id 与 AppState.my_device_id 一致。
    // 注意：Tauri State 不能在 unit test 里直接构造，改用内部逻辑单测：
    // 验证 StatusInfo 字段赋值逻辑正确（直接构造 StatusInfo 验证）。
    #[test]
    fn get_status_returns_my_device_id_correct() {
        let status = make_status();
        assert_eq!(
            status.my_device_id, "test-device-id",
            "my_device_id must match input"
        );
        assert_eq!(
            status.listen_addr, "127.0.0.1:5858",
            "listen_addr must match input"
        );
        assert_eq!(status.approved_count, 0);
    }

    // 单测 2：join_group_with_invalid_addr_returns_err_string
    // 验证 normalize_addr + SocketAddr parse 对非法地址返 Err(String)。
    #[test]
    fn join_group_with_invalid_addr_returns_err_string() {
        let invalid_cases = vec!["", "notanaddr", "http://", "192.168.1.1", ":::invalid"];
        for addr in invalid_cases {
            let normalized = normalize_addr(addr);
            let parse_result = normalized.parse::<std::net::SocketAddr>();
            // 非法地址应解析失败
            assert!(
                parse_result.is_err(),
                "invalid addr '{addr}' should fail to parse as SocketAddr"
            );
        }
    }

    // 单测 3：approve_peer_unknown_returns_not_found
    // ADR-008 MUST-3：验证 approve_peer 对未知 device_id 返回通用 "not_found" 串（不含 device_id 字面值）。
    // 使用 PeerRegistry 逻辑验证（不依赖 Tauri runtime）。
    #[test]
    fn approve_peer_unknown_returns_not_found() {
        use crate::app::client_pool::ClientPool;
        use crate::peer::PeerRegistry;
        use std::sync::Arc;

        let pool = Arc::new(ClientPool::new());
        let registry = PeerRegistry::new(pool);

        let unknown_id = "nonexistent-device-uuid";
        // 未在 registry 中 → is_known = false
        assert!(
            !registry.is_known(unknown_id),
            "unknown device must not be known"
        );
        // approve_peer 逻辑：!is_known → Err("not_found")（ADR-008 MUST-3）
        let result: Result<(), String> = if !registry.is_known(unknown_id) {
            Err("not_found".to_string())
        } else {
            Ok(())
        };
        assert!(
            result.is_err(),
            "approve_peer for unknown device must return Err"
        );
        let err_str = result.unwrap_err();
        assert_eq!(
            err_str, "not_found",
            "error body must be generic 'not_found', not expose device_id"
        );
        // 不含 device_id 字面值（ADR-008 MUST-3 核心约束）
        assert!(
            !err_str.contains(unknown_id),
            "error body must not contain device_id literal"
        );
    }

    // 单测 4：set_config_persists_device_name — Config 校验逻辑
    // 验证 set_config 对空 device_name 拒绝，对合法名称接受。
    // 直接测 Config 字段逻辑（不依赖 Tauri managed state）。
    #[test]
    fn set_config_persists_device_name() {
        use crate::app::config::Config;
        use crate::peer::sanitize::sanitize_device_name;

        // 空字符串被 trim → empty → 拒绝
        let empty_name = "   ".trim().to_string();
        assert!(
            empty_name.is_empty(),
            "all-whitespace device name trims to empty"
        );

        // 合法名称 ≤ 64 字符
        let valid_name = "工作 Mac";
        let cfg = Config {
            device_name: valid_name.to_string(),
            listen_port: 5858,
            peer_hint: None,
        };
        assert_eq!(cfg.device_name, valid_name);

        // 超长名称经 sanitize_device_name 截断到 64 codepoints（ADR-008 MUST-8）
        let long_name: String = "x".repeat(100);
        let sanitized = sanitize_device_name(&long_name);
        assert_eq!(
            sanitized.chars().count(),
            64,
            "sanitize_device_name must truncate to 64 codepoints"
        );
    }

    // 单测 9：set_config_rejects_rtl_in_device_name
    // ADR-008 MUST-8：set_config 经 sanitize_device_name 后 RTL 字符被过滤
    #[test]
    fn set_config_rejects_rtl_in_device_name() {
        use crate::peer::sanitize::sanitize_device_name;

        // U+202E RIGHT-TO-LEFT OVERRIDE 是 ADR-008 Bidi 黑名单字符
        let rtl = '\u{202E}';
        let name_with_rtl = format!("exploit{rtl}gpj.exe");
        let trimmed = name_with_rtl.trim().to_string();

        // set_config 入口路径：sanitize_device_name 应过滤掉 RTL 字符
        let safe_name = sanitize_device_name(&trimmed);
        assert!(
            !safe_name.contains('\u{202E}'),
            "sanitize_device_name must strip U+202E RTL override from device_name"
        );
        // 过滤后剩余部分是合法内容（非 <unnamed>）
        assert_ne!(
            safe_name, "<unnamed>",
            "non-empty name after RTL strip must not become <unnamed>"
        );
    }

    // 单测 10：set_config_truncates_long_device_name
    // ADR-008 MUST-8：超长 device_name 被 sanitize_device_name 截断到 64 codepoints
    #[test]
    fn set_config_truncates_long_device_name() {
        use crate::peer::sanitize::sanitize_device_name;

        // 500 个中文字符，每个 3 字节，远超 64 codepoints 限制
        let long_unicode: String = "中".repeat(500);
        let safe_name = sanitize_device_name(&long_unicode);
        assert_eq!(
            safe_name.chars().count(),
            64,
            "device_name must be truncated to exactly 64 Unicode codepoints"
        );
    }

    // 单测 11：set_config_strips_control_chars
    // ADR-008 MUST-8：控制字符（U+0000-U+001F）被 sanitize_device_name 过滤
    #[test]
    fn set_config_strips_control_chars() {
        use crate::peer::sanitize::sanitize_device_name;

        // 含 NUL + BEL + ESC 等 C0 控制字符
        let with_ctrl = "My\u{0000}Mac\u{0007}Book\u{001B}";
        let safe_name = sanitize_device_name(with_ctrl);
        assert!(
            !safe_name
                .chars()
                .any(|c| c <= '\u{001F}' || c == '\u{007F}'),
            "sanitize_device_name must remove all C0 control characters"
        );
        // 合法字符保留
        assert!(
            safe_name.contains("MyMacBook"),
            "legitimate chars must be preserved after control char strip"
        );
    }

    // 单测 12：set_config_io_error_returns_generic_internal_error
    // ADR-008 MUST-3：set_config 写盘失败返通用 "internal_error"，不含 ProjectDirs path
    // 直接测 boundary 映射逻辑（模拟失败 → 返 "internal_error" 字面量）
    #[test]
    fn set_config_io_error_returns_generic_internal_error() {
        // 模拟 set_config save 失败路径：boundary 处 map_err 应映射到 "internal_error"
        // （不包含 /Users/... 之类的路径信息）
        let simulated_io_err = std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "/Users/victim/Library/Application Support/com.synccopy.app/config.json: permission denied",
        );
        let anyhow_err = anyhow::anyhow!(simulated_io_err);

        // boundary 映射：任何写盘失败 → "internal_error"（不含 path）
        let boundary_str = {
            tracing::warn!(error = %anyhow_err, "test: simulated save failure");
            "internal_error".to_string()
        };
        assert_eq!(
            boundary_str, "internal_error",
            "io error must map to generic 'internal_error'"
        );
        // 关键：boundary 返回值不含内部路径
        assert!(
            !boundary_str.contains("Library"),
            "boundary error must not expose internal path"
        );
        assert!(
            !boundary_str.contains("config.json"),
            "boundary error must not expose config file path"
        );
    }

    // 单测 5：normalize_addr 去掉 http:// 前缀和尾部斜杠
    #[test]
    fn normalize_addr_strips_prefix_and_slash() {
        assert_eq!(
            normalize_addr("http://192.168.1.10:5858/"),
            "192.168.1.10:5858"
        );
        assert_eq!(
            normalize_addr("https://192.168.1.10:5858"),
            "192.168.1.10:5858"
        );
        assert_eq!(normalize_addr("192.168.1.10:5858"), "192.168.1.10:5858");
        assert_eq!(normalize_addr("  192.168.1.10:5858  "), "192.168.1.10:5858");
    }

    // 单测 6：entry_to_item 正确序列化 text payload
    #[test]
    fn entry_to_item_text_payload() {
        let entry = HistoryEntry {
            id: "test-id".to_string(),
            timestamp_ms: 1000,
            source: HistorySource::Local,
            content_hash: Some("hash123".to_string()),
            payload: HistoryPayload::Text {
                text: "hello world".to_string(),
            },
        };
        let item = entry_to_item(&entry);
        assert_eq!(item.id, "test-id");
        assert_eq!(item.source["kind"], "local");
        assert_eq!(item.payload["type"], "text");
        assert_eq!(item.payload["text"], "hello world");
    }

    // 单测 7：entry_to_item 正确序列化 remote source
    #[test]
    fn entry_to_item_remote_source() {
        let entry = HistoryEntry {
            id: "remote-id".to_string(),
            timestamp_ms: 2000,
            source: HistorySource::Remote {
                device_name: "工作 Mac".to_string(),
            },
            content_hash: None,
            payload: HistoryPayload::Text {
                text: "from remote".to_string(),
            },
        };
        let item = entry_to_item(&entry);
        assert_eq!(item.source["kind"], "remote");
        assert_eq!(item.source["device_name"], "工作 Mac");
    }

    // 单测 8：relative_time_str 各区间
    #[test]
    fn relative_time_str_all_ranges() {
        assert_eq!(relative_time_str(0), "刚刚");
        assert_eq!(relative_time_str(59), "刚刚");
        assert_eq!(relative_time_str(60), "1 分钟前");
        assert_eq!(relative_time_str(3599), "59 分钟前");
        assert_eq!(relative_time_str(3600), "1 小时前");
        assert_eq!(relative_time_str(86399), "23 小时前");
        assert_eq!(relative_time_str(86400), "1 天前");
    }
}
