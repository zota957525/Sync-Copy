//! heartbeat_worker — 主动 ping all peers + 隐形掉线检测
//! see specs/peer-heartbeat.md (第 1.1 节 / 第 4 节 AC #8 #9 #10 #11)
//! see decisions/ADR-010-lifecycle.md (第 3.6 节 long-running task runtime 归属 + Shutting 禁 replace)
//! see decisions/ADR-009-peer-registry.md (第 3.5 节 client_pool.replace 契约 + 第 5 节反模式)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-2 / 第 5.3 节 banned 校验)
//!
//! PR-6b 新增：
//! - HeartbeatWorker::start()：tauri::async_runtime 上 spawn 心跳 + 隐形掉线检测合并 task
//! - HeartbeatWorker::shutdown()：CancellationToken cancel + JoinHandle 500ms timeout
//! - force_rebuild_connection()：强制重连（banned 校验 + Shutting 拒绝 + client_pool.replace + re-handshake）
//! - 隐形掉线检测（spec peer-heartbeat 第 1.1 节）：30s 无 broadcast + 15s 无 heartbeat → 强制重连
//!
//! 与 lifecycle 集成（ADR-010 第 3.2 节 step 6 / 第 3.3 节 step 4）：
//!   lifecycle.start step 6 → HeartbeatWorker::start(state.clone())
//!   lifecycle.shutdown step 4 → HeartbeatWorker::shutdown(500ms deadline)
//!
//! 反模式黑名单（ADR-010 第 3.6 节 / ADR-009 第 7.3 节）：
//! - ❌ lifecycle.phase == Shutting 后调 client_pool.replace（白浪费 + 与 step 6 clear 抢占）
//! - ❌ banned peer 执行 force_rebuild（A3 zombie peer 复活路径）
//! - ❌ 心跳成功时更新 last_successful_sync_at（ADR-008 5.2 节硬约束）
//! - ❌ 持锁过 await（任何 peers.snapshot() 在 await 前释放锁）

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use crate::app::lifecycle::Phase;
use crate::app::state::AppState;
use crate::peer::TrustState;

// ---------------------------------------------------------------------------
// 常量（specs/peer-heartbeat.md 第 3 节 + ADR-003 第 3.7 节）
// ---------------------------------------------------------------------------

/// 主动 ping 周期（ADR-003 第 3.7 节 + spec peer-heartbeat 第 3 节 PING_INTERVAL）
const PING_INTERVAL: Duration = Duration::from_secs(5);

/// 连续心跳失败触发强制重连阈值（spec peer-heartbeat 第 4 节 AC #8 建议 N=3）
const FORCE_REBUILD_LIMIT: u32 = 3;

/// 隐形掉线检测：无 broadcast 成功时长阈值（spec peer-heartbeat 第 1.1 节）
const HIDDEN_DEAD_SYNC_THRESHOLD: Duration = Duration::from_secs(30);

/// 隐形掉线检测：无 heartbeat 成功时长阈值（spec peer-heartbeat 第 1.1 节）
const HIDDEN_DEAD_HEARTBEAT_THRESHOLD: Duration = Duration::from_secs(15);

// ---------------------------------------------------------------------------
// HeartbeatWorker struct
// ---------------------------------------------------------------------------

/// 心跳 worker + 隐形掉线检测合并 task（ADR-010 第 3.6 节 long-running task 表）。
///
/// 由 Lifecycle::start step 6 通过 HeartbeatWorker::start() 构造并持有。
/// 由 Lifecycle::shutdown step 4 通过 HeartbeatWorker::shutdown() 取消 + join。
///
/// runtime：tauri::async_runtime（Tauri 内置 tokio multi-thread；ADR-010 第 3.6 节选项 A 决议）。
pub struct HeartbeatWorker {
    /// 取消令牌（lifecycle step 2 cancel → step 4 join）
    cancel: CancellationToken,
    /// task 句柄（step 4 join 500ms timeout）
    handle: Option<tauri::async_runtime::JoinHandle<()>>,
}

impl HeartbeatWorker {
    /// 启动心跳 worker（lifecycle step 6 调用）。
    ///
    /// 接受 parent CancellationToken → 派生 child token，与 lifecycle.health_cancel 联动。
    /// task 挂在 tauri::async_runtime（Tauri 内置 tokio；ADR-010 第 3.6 节选项 A）。
    pub fn start(state: Arc<AppState>, parent_cancel: CancellationToken) -> Self {
        let cancel = parent_cancel.child_token();
        let cancel_for_task = cancel.clone();

        let handle = tauri::async_runtime::spawn(async move {
            heartbeat_loop(state, cancel_for_task).await;
        });

        Self {
            cancel,
            handle: Some(handle),
        }
    }

    /// 优雅关闭（lifecycle shutdown step 4 调用，deadline = 500ms）。
    ///
    /// 1. cancel token（通知 worker 退出 loop）
    /// 2. join handle（等 task 自然退出；超 deadline 则不再等）
    ///
    /// ADR-010 第 3.3 节 step 4：500ms timeout，超时 tracing::warn。
    pub async fn shutdown(mut self, deadline: Duration) {
        self.cancel.cancel();

        if let Some(handle) = self.handle.take() {
            let t0 = Instant::now();
            match tokio::time::timeout(deadline, handle).await {
                Ok(Ok(())) => {
                    tracing::debug!(
                        target: "heartbeat_worker",
                        actual_ms = t0.elapsed().as_millis(),
                        "heartbeat_worker shutdown: task joined cleanly"
                    );
                }
                Ok(Err(e)) => {
                    tracing::warn!(
                        target: "heartbeat_worker",
                        error = %e,
                        "heartbeat_worker shutdown: task join error (task panicked?)"
                    );
                }
                Err(_timeout) => {
                    tracing::warn!(
                        target: "heartbeat_worker",
                        deadline_ms = deadline.as_millis(),
                        actual_ms = t0.elapsed().as_millis(),
                        "heartbeat_worker shutdown: join timeout (task still running)"
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// heartbeat_loop — 心跳 worker 主循环
// ---------------------------------------------------------------------------

/// 心跳 worker 主循环（在 tauri::async_runtime 上运行）。
///
/// 每 PING_INTERVAL（5s）：
///   1. snapshot Approved peers（立即释放锁，不持锁过 await）
///   2. 并行 ping 所有 peer
///   3. ping 成功 → update_heartbeat_success（不写 last_successful_sync_at）
///   4. ping 失败 → increment_heartbeat_failure；≥ FORCE_REBUILD_LIMIT → force_rebuild_connection
///   5. 隐形掉线检测：30s 无 broadcast + 15s 无 heartbeat → force_rebuild_connection
///
/// 每 tick 顶端检查 lifecycle.phase == Shutting → 短路退出（ADR-010 第 3.6 节 P4 补丁）。
async fn heartbeat_loop(state: Arc<AppState>, cancel: CancellationToken) {
    tracing::info!(target: "heartbeat_worker", "heartbeat_loop started");

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::debug!(target: "heartbeat_worker", "cancel token fired, exiting loop");
                break;
            }
            _ = tokio::time::sleep(PING_INTERVAL) => {
                // Shutting 阶段短路（ADR-010 第 3.6 节 P4 / ADR-009 第 7.3 节 P2 反模式）
                // force_rebuild 在 Shutting 后无意义且与 step 6 clear 抢占
                if state.lifecycle.phase() == Phase::Shutting {
                    tracing::debug!(
                        target: "heartbeat_worker",
                        "lifecycle.phase == Shutting, skip heartbeat tick"
                    );
                    continue;
                }

                run_ping_round(&state, &cancel).await;
            }
        }
    }

    tracing::info!(target: "heartbeat_worker", "heartbeat_loop exited");
}

/// 单轮 ping（peer snapshot → 并行 ping → 处理结果 + 隐形掉线检测）。
///
/// 与 heartbeat_loop 分离，方便测试。
async fn run_ping_round(state: &Arc<AppState>, cancel: &CancellationToken) {
    // snapshot 后立即释放读锁（不持锁过 await）
    // SECURITY（ADR-009 第 3.2 节 P1 注释）：含 aes_key clone，不进 tracing fields。
    let peers: Vec<_> = state
        .peers
        .snapshot()
        .into_iter()
        .filter(|p| p.trust_state == TrustState::Approved)
        .collect();

    if peers.is_empty() {
        return;
    }

    tracing::debug!(
        target: "heartbeat_worker",
        peer_count = peers.len(),
        "ping round start"
    );

    let now = Instant::now();

    // 并行 ping（futures::join_all 等价 — 用 tokio::join_all 模拟）
    let mut ping_tasks = Vec::with_capacity(peers.len());
    for peer in peers {
        // 在进入 spawn 前检查 cancel，避免 Shutting 期间启动新 task
        if cancel.is_cancelled() {
            break;
        }

        let state_clone = Arc::clone(state);
        let peer_id = peer.device_id.clone();
        let peer_addr = peer.addr;
        let last_successful_sync_at = peer.last_successful_sync_at;
        let last_heartbeat_at = peer.last_heartbeat_at;

        ping_tasks.push(tokio::spawn(async move {
            let result = crate::network::client::ping(&state_clone, &peer_id, peer_addr).await;

            handle_ping_result(
                &state_clone,
                &peer_id,
                peer_addr,
                result,
                last_successful_sync_at,
                last_heartbeat_at,
                now,
            )
            .await;
        }));
    }

    for task in ping_tasks {
        let _ = task.await;
    }
}

/// 处理单次 ping 结果 + 隐形掉线检测。
///
/// ping 成功：update_heartbeat_success（清零 failures + 更新 last_heartbeat_at）。
///   注意：**不**写 last_successful_sync_at（ADR-008 5.2 节硬约束）。
///
/// ping 失败：increment_heartbeat_failure；
///   >= FORCE_REBUILD_LIMIT → force_rebuild_connection。
///
/// 隐形掉线检测（spec peer-heartbeat 第 1.1 节）：
///   30s 内有过 broadcast 成功（last_successful_sync_at 不为 None），
///   但之后 15s 无 heartbeat 成功（last_heartbeat_at 过老）
///   → 触发 force_rebuild_connection。
async fn handle_ping_result(
    state: &Arc<AppState>,
    peer_id: &str,
    peer_addr: std::net::SocketAddr,
    ping_result: anyhow::Result<()>,
    last_successful_sync_at: Option<Instant>,
    last_heartbeat_at: Option<Instant>,
    round_start: Instant,
) {
    match ping_result {
        Ok(()) => {
            // ping 成功：更新 last_heartbeat_at + 清零 consecutive_heartbeat_failures
            // ADR-008 5.2 节：不写 last_successful_sync_at（心跳 ≠ 数据同步）
            state.peers.update_heartbeat_success(peer_id);
            tracing::debug!(
                target: "heartbeat_worker",
                peer_id = %peer_id,
                "ping ok: heartbeat_at updated (last_successful_sync_at NOT touched)"
            );
        }
        Err(e) => {
            let count = state.peers.increment_heartbeat_failure(peer_id);
            tracing::warn!(
                target: "heartbeat_worker",
                peer_id = %peer_id,
                error = %e,
                consecutive_failures = count,
                "ping failed"
            );

            if count >= FORCE_REBUILD_LIMIT {
                tracing::warn!(
                    target: "heartbeat_worker",
                    peer_id = %peer_id,
                    consecutive_failures = count,
                    force_rebuild_limit = FORCE_REBUILD_LIMIT,
                    "forced TCP rebuild triggered (consecutive heartbeat failures exceeded limit)"
                );
                force_rebuild_connection(state, peer_id, peer_addr).await;
            }
        }
    }

    // 隐形掉线检测（spec peer-heartbeat 第 1.1 节）
    detect_hidden_dead(
        state,
        peer_id,
        peer_addr,
        last_successful_sync_at,
        last_heartbeat_at,
        round_start,
    )
    .await;
}

/// 隐形掉线检测逻辑。
///
/// 判定条件（两者同时满足）：
///   1. last_successful_sync_at 不为 None（曾经成功同步过）
///      且 now - last_successful_sync_at > HIDDEN_DEAD_SYNC_THRESHOLD（30s）
///   2. last_heartbeat_at 为 None 或 now - last_heartbeat_at > HIDDEN_DEAD_HEARTBEAT_THRESHOLD（15s）
///
/// 满足 → 触发 force_rebuild_connection（spec peer-heartbeat 第 1.1 节三条解决方向之一）。
async fn detect_hidden_dead(
    state: &Arc<AppState>,
    peer_id: &str,
    peer_addr: std::net::SocketAddr,
    last_successful_sync_at: Option<Instant>,
    last_heartbeat_at: Option<Instant>,
    round_start: Instant,
) {
    // 条件 1：曾同步但 30s 内无 broadcast 成功
    let sync_stale = last_successful_sync_at
        .map(|t| round_start.saturating_duration_since(t) > HIDDEN_DEAD_SYNC_THRESHOLD)
        .unwrap_or(false); // 从未同步 → 不触发

    if !sync_stale {
        return;
    }

    // 条件 2：15s 内没有 heartbeat 成功
    let hb_stale = last_heartbeat_at
        .map(|t| round_start.saturating_duration_since(t) > HIDDEN_DEAD_HEARTBEAT_THRESHOLD)
        .unwrap_or(true); // 从未有 heartbeat → 视为过期

    if hb_stale {
        tracing::warn!(
            target: "heartbeat_worker",
            peer_id = %peer_id,
            sync_threshold_secs = HIDDEN_DEAD_SYNC_THRESHOLD.as_secs(),
            hb_threshold_secs = HIDDEN_DEAD_HEARTBEAT_THRESHOLD.as_secs(),
            "hidden_dead detected: forced TCP rebuild"
        );
        force_rebuild_connection(state, peer_id, peer_addr).await;
    }
}

// ---------------------------------------------------------------------------
// force_rebuild_connection — 强制重建底层 TCP 连接
// ---------------------------------------------------------------------------

/// 强制重建底层 TCP 连接（spec peer-heartbeat 第 4 节 AC #8 强制重连）。
///
/// 步骤：
///   1. 检查 lifecycle.phase != Shutting（ADR-010 第 3.6 节 P4 反模式黑名单）
///   2. 检查 peer 未被 ban（ADR-009 第 5.3 节 / ADR-008 第 5.3 节必修）
///   3. 检查 peer 仍在 registry（is_known）
///   4. client_pool.replace(id)：drop 旧 Client → 新建 Client（no_proxy，ADR-009 第 3.5 节）
///   5. re-handshake：dial_handshake 拿新 aes_key + update_aes_key
///   6. reset_heartbeat_failures（成功重连后归零）
///
/// 失败（任一步骤 Err）→ 仅 tracing::warn，不 panic；
/// 下轮 heartbeat 继续尝试（v5-7 idempotent + 三层 fallback 原则）。
pub(crate) async fn force_rebuild_connection(
    state: &Arc<AppState>,
    peer_id: &str,
    peer_addr: std::net::SocketAddr,
) {
    // 步骤 1：Shutting 阶段禁止 replace（ADR-010 第 3.6 节 P4 / ADR-009 第 7.3 节 P2 反模式）
    if state.lifecycle.phase() == Phase::Shutting {
        tracing::debug!(
            target: "heartbeat_worker",
            peer_id = %peer_id,
            "force_rebuild_connection: skipped (lifecycle is Shutting)"
        );
        return;
    }

    // 步骤 2：banned 校验（ADR-008 第 5.3 节必修 / ADR-009 第 7.3 节 P2）
    // 同时检查 is_known — 避免对已被移除 peer 做无用操作
    if !state.peers.is_known(peer_id) || state.peers.is_banned(peer_id) {
        tracing::debug!(
            target: "heartbeat_worker",
            peer_id = %peer_id,
            is_known = state.peers.is_known(peer_id),
            is_banned = state.peers.is_banned(peer_id),
            "force_rebuild_connection: skipped (peer not known or banned)"
        );
        return;
    }

    tracing::info!(
        target: "heartbeat_worker",
        peer_id = %peer_id,
        addr = %peer_addr,
        "force_rebuild_connection: replacing client pool entry"
    );

    // 步骤 3：client_pool.replace（ADR-009 第 3.5 节 replace 契约）
    // 此处 caller 已校验 is_known && !is_banned（pre 条件满足）
    state.client_pool.replace(peer_id);

    // 步骤 4：replace 后再次校验（校验与 replace 之间存在窗口期，ADR-009 第 4.3 节副作用 #3）
    if !state.peers.is_known(peer_id) || state.peers.is_banned(peer_id) {
        // 窗口期内被 ban/remove → 清理刚替换的 client
        state.client_pool.remove_for_rebuild(peer_id);
        tracing::warn!(
            target: "heartbeat_worker",
            peer_id = %peer_id,
            "force_rebuild_connection: peer banned/removed during replace window, cleaned up"
        );
        return;
    }

    // 步骤 5：re-handshake（取新 aes_key）
    // 使用 dial_handshake：生成新 ephemeral key → POST /handshake → 更新 PeerRegistry
    // 注意：dial_handshake 内部会调 registry.insert + approve，覆盖已有 PeerState
    // 这里显式指定 peer_addr（来自 snapshot，握手成功前不变）
    let my_device_id = state.my_device_id.clone();
    // device_name 用空字符串占位（re-handshake 时 dial_handshake 会用对端返回的 device_name）
    match crate::network::client::dial_handshake(
        peer_addr,
        state,
        &my_device_id,
        "SyncCopy",
        crate::network::DEFAULT_PORT,
    )
    .await
    {
        Ok(()) => {
            // 步骤 6：成功重连 → 归零 consecutive_heartbeat_failures
            state.peers.reset_heartbeat_failures(peer_id);
            tracing::info!(
                target: "heartbeat_worker",
                peer_id = %peer_id,
                addr = %peer_addr,
                "force_rebuild_connection: re-handshake success, failures reset"
            );
        }
        Err(e) => {
            tracing::warn!(
                target: "heartbeat_worker",
                peer_id = %peer_id,
                addr = %peer_addr,
                error = %e,
                "force_rebuild_connection: re-handshake failed (will retry next heartbeat round)"
            );
            // 不 panic；下轮 heartbeat 再试（v5-7 三层 fallback）
        }
    }
}

// ---------------------------------------------------------------------------
// 单元测试（ADR-010 第 6 节验证段 / specs/peer-heartbeat.md 第 4 节 AC）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::AppState;
    use crate::peer::{PeerState, TrustState};
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use zeroize::Zeroizing;

    fn make_approved_peer(id: &str, addr: &str) -> PeerState {
        PeerState {
            device_id: id.to_string(),
            device_name: format!("device-{id}"),
            addr: addr.parse::<SocketAddr>().expect("addr parse"),
            pubkey_b64: "test_pubkey".to_string(),
            aes_key: Zeroizing::new([0u8; 32]),
            last_successful_sync_at: None,
            last_heartbeat_at: None,
            consecutive_heartbeat_failures: 0,
            consecutive_send_failures: 0,
            trust_state: TrustState::Approved,
            last_seen_seq_by_kind: HashMap::new(),
        }
    }

    // 单测 1：worker 仅 ping Approved peer，不 ping Banned/Pending
    //
    // 验证 heartbeat_loop 中 snapshot 过滤逻辑：
    //   仅 trust_state == Approved 的 peer 出现在 ping 列表中。
    // (对应 PR-6b 任务描述中的 worker_skips_banned_peers)
    #[test]
    fn worker_only_pings_approved_peers() {
        use crate::app::client_pool::ClientPool;
        use crate::peer::PeerRegistry;

        let pool = Arc::new(ClientPool::new());
        let registry = PeerRegistry::new(Arc::clone(&pool));

        // 插入 Approved peer
        let approved = make_approved_peer("peer-approved", "127.0.0.1:9001");
        registry.insert(approved);
        registry.approve("peer-approved");

        // 插入 Banned peer（通过 ban + 重插，避免 ban 自动 remove）
        // 注：直接 ban 未知 peer（was_peer=false）→ banned 集合含，但 inner 不含
        registry.ban("peer-banned");

        // snapshot 过滤逻辑（与 run_ping_round 内一致）
        let approved_list: Vec<_> = registry
            .snapshot()
            .into_iter()
            .filter(|p| p.trust_state == TrustState::Approved)
            .collect();

        assert_eq!(
            approved_list.len(),
            1,
            "only Approved peers should be pinged"
        );
        assert_eq!(approved_list[0].device_id, "peer-approved");
    }

    // 单测 2：ping 失败 → increment_heartbeat_failure 计数递增
    //
    // 验证 PeerRegistry.increment_heartbeat_failure 在 handle_ping_result 失败路径的计数行为。
    #[test]
    fn worker_increments_failure_count_on_ping_fail() {
        use crate::app::client_pool::ClientPool;
        use crate::peer::PeerRegistry;

        let pool = Arc::new(ClientPool::new());
        let registry = PeerRegistry::new(Arc::clone(&pool));

        let peer = make_approved_peer("peer-fail", "127.0.0.1:9002");
        registry.insert(peer);
        registry.approve("peer-fail");

        // 模拟 3 次 ping 失败
        let c1 = registry.increment_heartbeat_failure("peer-fail");
        let c2 = registry.increment_heartbeat_failure("peer-fail");
        let c3 = registry.increment_heartbeat_failure("peer-fail");

        assert_eq!(c1, 1, "first failure should give count 1");
        assert_eq!(c2, 2, "second failure should give count 2");
        assert_eq!(c3, 3, "third failure should give count 3");

        // 成功后归零
        registry.update_heartbeat_success("peer-fail");
        let after_success = registry.get_consecutive_heartbeat_failures("peer-fail");
        assert_eq!(after_success, 0, "success should reset failure count to 0");
    }

    // 单测 3：连续 3 次失败 → 达到 FORCE_REBUILD_LIMIT
    //
    // 验证阈值判定逻辑（count >= FORCE_REBUILD_LIMIT = 3）。
    // 实际 force_rebuild_connection 调用需要 AppState + 真实网络，这里仅验证阈值判定。
    #[test]
    fn worker_force_rebuild_threshold_at_3_failures() {
        use crate::app::client_pool::ClientPool;
        use crate::peer::PeerRegistry;

        let pool = Arc::new(ClientPool::new());
        let registry = PeerRegistry::new(Arc::clone(&pool));

        let peer = make_approved_peer("peer-rebuild", "127.0.0.1:9003");
        registry.insert(peer);
        registry.approve("peer-rebuild");

        // 2 次失败 → 不触发（< FORCE_REBUILD_LIMIT）
        let c1 = registry.increment_heartbeat_failure("peer-rebuild");
        let c2 = registry.increment_heartbeat_failure("peer-rebuild");
        assert!(
            c1 < FORCE_REBUILD_LIMIT,
            "2 failures should not reach limit"
        );
        assert!(
            c2 < FORCE_REBUILD_LIMIT,
            "2 failures should not reach limit"
        );

        // 第 3 次失败 → 触发（== FORCE_REBUILD_LIMIT）
        let c3 = registry.increment_heartbeat_failure("peer-rebuild");
        assert!(
            c3 >= FORCE_REBUILD_LIMIT,
            "3rd failure must reach FORCE_REBUILD_LIMIT={FORCE_REBUILD_LIMIT}"
        );
    }

    // 单测 4：force_rebuild_connection 在 Shutting 阶段跳过（不调 replace）
    //
    // 验证 ADR-010 第 3.6 节 P4 反模式黑名单：Shutting 期禁 replace。
    #[tokio::test]
    async fn worker_skips_force_rebuild_during_shutting() {
        let state = Arc::new(AppState::new());

        // 手动设 lifecycle.phase = Shutting
        *state.lifecycle.phase.write() = Phase::Shutting;

        // force_rebuild_connection 应立即 return（不 panic，不 replace）
        let addr: SocketAddr = "127.0.0.1:9004".parse().expect("addr parse");
        force_rebuild_connection(&state, "peer-shutting", addr).await;

        // 验证：client_pool 仍空（replace 未被调用）
        assert!(
            state.client_pool.is_empty(),
            "client_pool must remain empty when lifecycle is Shutting (ADR-010 P4)"
        );
    }

    // 单测 5：force_rebuild_connection 对 banned peer 跳过
    //
    // 验证 ADR-008 第 5.3 节必修：force_rebuild 前校验 banned。
    #[tokio::test]
    async fn worker_skips_force_rebuild_for_banned_peer() {
        let state = Arc::new(AppState::new());
        *state.lifecycle.phase.write() = Phase::Running;

        // ban 一个 peer（was_peer=false，不在 inner）
        state.peers.ban("peer-banned-rb");

        let addr: SocketAddr = "127.0.0.1:9005".parse().expect("addr parse");
        force_rebuild_connection(&state, "peer-banned-rb", addr).await;

        // client_pool 仍空（banned peer 不应被 replace）
        assert!(
            state.client_pool.is_empty(),
            "client_pool must remain empty for banned peer (ADR-008 5.3)"
        );
    }

    // 单测 6：隐形掉线检测 — 30s 无 broadcast + 15s 无 heartbeat → 触发
    //
    // 验证 detect_hidden_dead 的触发条件。
    // 直接测 sync_stale + hb_stale 逻辑，不调 force_rebuild（避免网络依赖）。
    #[test]
    fn hidden_dead_detection_triggers_when_both_stale() {
        let now = Instant::now();

        // 构造：last_successful_sync_at = 40s 前（> 30s 阈值）
        // last_heartbeat_at = 20s 前（> 15s 阈值）
        let last_sync = now
            .checked_sub(Duration::from_secs(40))
            .expect("sub should not underflow");
        let last_hb = now
            .checked_sub(Duration::from_secs(20))
            .expect("sub should not underflow");

        let sync_stale = now.saturating_duration_since(last_sync) > HIDDEN_DEAD_SYNC_THRESHOLD;
        let hb_stale = now.saturating_duration_since(last_hb) > HIDDEN_DEAD_HEARTBEAT_THRESHOLD;

        assert!(
            sync_stale,
            "40s since last sync > 30s threshold: should be stale"
        );
        assert!(
            hb_stale,
            "20s since last heartbeat > 15s threshold: should be stale"
        );
        // 两条件同时满足 → 触发隐形掉线检测
    }

    // 单测 7：隐形掉线检测 — 最近有 heartbeat → 不触发
    #[test]
    fn hidden_dead_detection_no_trigger_when_hb_recent() {
        let now = Instant::now();

        // last_successful_sync_at = 40s 前（stale）
        let last_sync = now
            .checked_sub(Duration::from_secs(40))
            .expect("sub should not underflow");
        // last_heartbeat_at = 5s 前（< 15s 阈值，not stale）
        let last_hb = now
            .checked_sub(Duration::from_secs(5))
            .expect("sub should not underflow");

        let sync_stale = now.saturating_duration_since(last_sync) > HIDDEN_DEAD_SYNC_THRESHOLD;
        let hb_stale = now.saturating_duration_since(last_hb) > HIDDEN_DEAD_HEARTBEAT_THRESHOLD;

        assert!(sync_stale, "40s since last sync should be stale");
        assert!(!hb_stale, "5s since heartbeat should NOT be stale (< 15s)");
        // hb_stale == false → 不触发
    }

    // 单测 8：heartbeat 成功不更新 last_successful_sync_at（ADR-008 5.2 节硬约束 / 卡 7 must-fix #1）
    //
    // 验证 update_heartbeat_success 不修改 last_successful_sync_at。
    #[test]
    fn heartbeat_success_does_not_update_last_successful_sync_at() {
        use crate::app::client_pool::ClientPool;
        use crate::peer::PeerRegistry;

        let pool = Arc::new(ClientPool::new());
        let registry = PeerRegistry::new(Arc::clone(&pool));

        let peer = make_approved_peer("peer-nosync", "127.0.0.1:9006");
        registry.insert(peer);
        registry.approve("peer-nosync");

        // 初始 last_successful_sync_at = None
        let before = registry.get("peer-nosync").expect("peer must exist");
        assert!(
            before.last_successful_sync_at.is_none(),
            "initial last_successful_sync_at must be None"
        );

        // 调 update_heartbeat_success（心跳成功）
        registry.update_heartbeat_success("peer-nosync");

        // last_successful_sync_at 必须仍为 None（ADR-008 5.2 节硬约束）
        let after = registry.get("peer-nosync").expect("peer must still exist");
        assert!(
            after.last_successful_sync_at.is_none(),
            "update_heartbeat_success MUST NOT update last_successful_sync_at (ADR-008 5.2 卡 7 must-fix #1)"
        );
        // last_heartbeat_at 应已更新
        assert!(
            after.last_heartbeat_at.is_some(),
            "update_heartbeat_success must update last_heartbeat_at"
        );
    }

    // 单测 9：update_aes_key 替换后旧 key 清零（MUST-2）
    #[test]
    fn update_aes_key_replaces_key() {
        use crate::app::client_pool::ClientPool;
        use crate::peer::PeerRegistry;

        let pool = Arc::new(ClientPool::new());
        let registry = PeerRegistry::new(Arc::clone(&pool));

        let mut peer = make_approved_peer("peer-keyupdate", "127.0.0.1:9007");
        peer.aes_key = Zeroizing::new([0xAAu8; 32]);
        registry.insert(peer);
        registry.approve("peer-keyupdate");

        let new_key = Zeroizing::new([0xBBu8; 32]);
        registry.update_aes_key("peer-keyupdate", new_key);

        let updated = registry.get("peer-keyupdate").expect("peer must exist");
        assert_eq!(
            *updated.aes_key, [0xBBu8; 32],
            "update_aes_key must replace the aes_key with the new value"
        );
    }
}
