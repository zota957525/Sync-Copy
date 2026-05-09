//! Sync Copy v2 — 最小 Tauri 入口外壳（v0 业务代码已清空，ADR-001）
//! see decisions/ADR-001-rewrite-with-strict-sdlc.md
//!
//! v0 实现保留在 legacy-prototype 分支 commit f4be188。
//! 业务模块将在 P2-1.c 起按 ADR-003 / ADR-009 / ADR-010 / ADR-011 重新落地。

// Windows release 模式下不弹额外控制台窗口——Tauri 官方要求，不可删除
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    sync_copy_lib::run();
}
