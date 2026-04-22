use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    sync::{atomic::{AtomicU64, Ordering}, Arc, mpsc},
};
use tokio::sync::oneshot;

use crate::{clipboard::ClipboardCmd, config::Config, history::History, peer::PeerRegistry};

/// 每个 peer 对应的对称密钥（X25519 ECDH → HKDF → AES-256-GCM key）
/// 只存内存，进程重启后丢失，下次握手重新协商
pub struct PeerKeys {
    keys: RwLock<HashMap<String, [u8; 32]>>,
}

impl PeerKeys {
    pub fn new() -> Self {
        Self {
            keys: RwLock::new(HashMap::new()),
        }
    }
    pub fn set(&self, device_id: String, key: [u8; 32]) {
        self.keys.write().insert(device_id, key);
    }
    pub fn get(&self, device_id: &str) -> Option<[u8; 32]> {
        self.keys.read().get(device_id).copied()
    }
    pub fn remove(&self, device_id: &str) {
        self.keys.write().remove(device_id);
    }
    pub fn clear(&self) {
        self.keys.write().clear();
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConnectionStatus {
    Idle,
    /// 我已启动服务端，但还没人握手成功
    Listening,
    Connecting,
    Connected { peers: usize },
    Error { message: String },
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        Self::Idle
    }
}

pub struct AppState {
    pub config: RwLock<Config>,
    pub status: RwLock<ConnectionStatus>,
    pub history: History,
    pub peers: PeerRegistry,

    /// 发消息到剪切板线程（写入系统剪切板并抑制回环）
    pub clipboard_tx: Mutex<Option<mpsc::Sender<ClipboardCmd>>>,

    /// 关闭 axum 服务器的 oneshot
    pub server_shutdown: Mutex<Option<oneshot::Sender<()>>>,

    /// 出站剪切板消息的单调递增序列
    pub seq: AtomicU64,

    /// 接收去重：每个 peer 最后见过的最大 seq
    pub last_seen_seq: RwLock<HashMap<String, u64>>,

    /// 握手审批队列：服务端收到握手后挂在这里，等前端点「同意/拒绝」再继续
    pub pending_approvals: Mutex<HashMap<String, oneshot::Sender<bool>>>,

    /// 文件保存审批队列
    pub pending_file_saves: Mutex<HashMap<String, PendingFileSave>>,

    /// 每个 peer 的对称加密密钥
    pub peer_keys: PeerKeys,

    /// 已经被本机（或任一小组成员）同意过的设备 id —— 收到这些设备的握手时
    /// 不再弹审批框，直接通过。通过 /peers/trust gossip 在组内传播。
    pub approved_device_ids: RwLock<HashSet<String>>,

    /// 已经被本机（或任一小组成员）拒绝过的设备 id —— 收到这些设备的握手
    /// 直接返回 403。通过 /peers/ban gossip 在组内传播。
    pub banned_device_ids: RwLock<HashSet<String>>,
}

#[allow(dead_code)]
pub struct PendingFileSave {
    pub filename: String,
    pub size: u64,
    pub origin_device_name: String,
    pub tx: oneshot::Sender<bool>,
}

impl AppState {
    pub fn new(config: Config) -> Arc<Self> {
        Arc::new(Self {
            config: RwLock::new(config),
            status: RwLock::new(ConnectionStatus::Idle),
            history: History::new(),
            peers: PeerRegistry::new(),
            clipboard_tx: Mutex::new(None),
            server_shutdown: Mutex::new(None),
            seq: AtomicU64::new(1),
            last_seen_seq: RwLock::new(HashMap::new()),
            pending_approvals: Mutex::new(HashMap::new()),
            pending_file_saves: Mutex::new(HashMap::new()),
            peer_keys: PeerKeys::new(),
            approved_device_ids: RwLock::new(HashSet::new()),
            banned_device_ids: RwLock::new(HashSet::new()),
        })
    }

    pub fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::SeqCst)
    }

    /// 检查并更新指定 origin 的最大 seq。返回 true 表示这是新消息应当处理
    pub fn seen_seq_and_update(&self, origin: &str, seq: u64) -> bool {
        let mut map = self.last_seen_seq.write();
        let prev = map.get(origin).copied().unwrap_or(0);
        if seq <= prev {
            return false;
        }
        map.insert(origin.to_string(), seq);
        true
    }
}

/// 根据 peers 数量更新 status 字段
pub fn update_status_connected(state: &AppState) {
    let n = state.peers.count();
    let mut st = state.status.write();
    *st = if n > 0 {
        ConnectionStatus::Connected { peers: n }
    } else {
        ConnectionStatus::Listening
    };
}
