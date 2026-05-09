//! network/handlers — HTTP handler 子模块
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节 12 端点)
//!
//! 12 端点分布：
//!   handshake.rs  — POST /handshake
//!   clipboard.rs  — POST /clipboard
//!   file.rs       — POST /file
//!   heartbeat.rs  — POST /heartbeat
//!   leave.rs      — POST /peers/leave
//!   peers.rs      — POST /peers/announce /peers/trust /peers/ban /peers/approval/{forward,decide,dismiss}
//!   history.rs    — POST /delete_history  POST /history/clear

pub mod clipboard;
pub mod file;
pub mod handshake;
pub mod heartbeat;
pub mod history;
pub mod leave;
pub mod peers;
