//! Sync Copy v2 — 最小 lib 外壳（v0 业务代码已清空，ADR-001）
//! see decisions/ADR-001-rewrite-with-strict-sdlc.md
//!
//! v0 实现保留在 legacy-prototype 分支 commit f4be188。
//! 业务模块将在 P2-1.c 起按 ADR-003 / ADR-009 / ADR-010 / ADR-011 重新落地。

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
