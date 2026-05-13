//! app 模块 — Lifecycle / ClientPool / AppState 聚合层
//! see specs/peer-heartbeat.md, decisions/ADR-010-lifecycle.md
//! see decisions/ADR-009-peer-registry.md (第 3.5 节 client_pool)
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.5 节)
//!
//! PR-3 范围（ADR-010 第 3 节全部落地）：
//! - lifecycle.rs：Phase 状态机 + Lifecycle struct + start/shutdown/phase
//! - client_pool.rs：per-peer reqwest::Client 池（禁止 lazy add）
//! - state.rs：AppState 聚合 Arc<PeerRegistry> + Arc<RateLimiter> + Arc<ClientPool> + Arc<Lifecycle>
//!
//! PR-6b 新增：
//! - heartbeat_worker.rs：主动 ping all peers + 隐形掉线检测（peer-heartbeat.md 第 1.1 节）
//!
//! PR-FE-0 新增：
//! - config.rs：Config 持久化（device_name / listen_port / peer_hint）
//! - history.rs：in-memory 历史列表 store（spec history-list.md MAX_HISTORY=50）

pub mod client_pool;
pub mod clipboard;
pub mod config;
pub mod heartbeat_worker;
pub mod history;
pub mod lifecycle;
pub mod state;
