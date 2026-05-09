//! Sync Copy v2 — lib 外壳
//! see decisions/ADR-001-rewrite-with-strict-sdlc.md
//!
//! v0 实现保留在 legacy-prototype 分支 commit f4be188。
//! 业务模块按 ADR-003 / ADR-009 / ADR-010 / ADR-011 重新落地（P2-1.c 起）。
//!
//! PR-1：crypto module（ADR-011 crypto traits）— 2026-05-09

// crypto module（ADR-011 crypto traits / ADR-008 MUST-1 AAD 绑值 / MUST-2 zeroize）
pub mod crypto;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .try_init()
        .ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            tracing::info!(
                version = app.package_info().version.to_string(),
                "Sync Copy v2 shell started"
            );
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Sync Copy 启动失败：tauri runtime 初始化错（v2 重写阶段最小外壳）");
}
