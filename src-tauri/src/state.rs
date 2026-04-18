use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{atomic::{AtomicU64, Ordering}, Arc, mpsc},
};
use tokio::sync::oneshot;

use crate::{clipboard::ClipboardCmd, config::Config, history::History, peer::PeerRegistry};

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
