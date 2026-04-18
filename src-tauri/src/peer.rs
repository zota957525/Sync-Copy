use parking_lot::RwLock;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct Peer {
    pub device_id: String,
    pub device_name: String,
    /// "ip:port" 形式
    pub addr: String,
}

pub struct PeerRegistry {
    peers: RwLock<HashMap<String, Peer>>, // keyed by device_id
}

impl PeerRegistry {
    pub fn new() -> Self {
        Self {
            peers: RwLock::new(HashMap::new()),
        }
    }

    pub fn upsert(&self, peer: Peer) {
        self.peers.write().insert(peer.device_id.clone(), peer);
    }

    pub fn remove(&self, device_id: &str) {
        self.peers.write().remove(device_id);
    }

    pub fn clear(&self) {
        self.peers.write().clear();
    }

    pub fn count(&self) -> usize {
        self.peers.read().len()
    }

    pub fn snapshot(&self) -> Vec<Peer> {
        self.peers.read().values().cloned().collect()
    }
}
