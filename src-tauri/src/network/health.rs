//! 后台周期性 ping 每个已知 peer。连续失败 >= FAIL_LIMIT 就把它从 peers 表里摘掉
//! （并更新状态 / 发 event 让前端刷新 "小组 N 台" 计数）。
use std::{collections::HashMap, sync::Arc, time::Duration};

use tauri::{AppHandle, Emitter};

use crate::state::{update_status_connected, AppState};

const PING_INTERVAL: Duration = Duration::from_secs(10);
const PING_TIMEOUT: Duration = Duration::from_secs(2);
const FAIL_LIMIT: u32 = 2;

pub fn spawn(state: Arc<AppState>, app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let client = match reqwest::Client::builder()
            .no_proxy()
            .timeout(PING_TIMEOUT)
            .connect_timeout(Duration::from_secs(1))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "health-check: cannot build reqwest client");
                return;
            }
        };
        let mut fail_counts: HashMap<String, u32> = HashMap::new();

        loop {
            tokio::time::sleep(PING_INTERVAL).await;
            let peers = state.peers.snapshot();
            if peers.is_empty() {
                fail_counts.clear();
                continue;
            }
            let alive_ids: std::collections::HashSet<String> =
                peers.iter().map(|p| p.device_id.clone()).collect();
            // 清理那些已经不在 peers 表里的计数项
            fail_counts.retain(|k, _| alive_ids.contains(k));

            let mut removed_any = false;
            for peer in peers {
                let url = format!("http://{}/ping", peer.addr);
                let ok = match client.get(&url).send().await {
                    Ok(r) => r.status().is_success(),
                    Err(_) => false,
                };
                if ok {
                    fail_counts.remove(&peer.device_id);
                    continue;
                }
                let count = fail_counts
                    .entry(peer.device_id.clone())
                    .and_modify(|c| *c += 1)
                    .or_insert(1);
                if *count >= FAIL_LIMIT {
                    tracing::info!(peer = %peer.device_name, "peer failed health check, removing");
                    state.peers.remove(&peer.device_id);
                    state.peer_keys.remove(&peer.device_id);
                    fail_counts.remove(&peer.device_id);
                    removed_any = true;
                }
            }
            if removed_any {
                update_status_connected(&state);
                let _ = app.emit("status-updated", ());
            }
        }
    });
}
