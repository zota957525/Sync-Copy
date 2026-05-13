//! Sync Copy v2 — lib 外壳
//! see decisions/ADR-001-rewrite-with-strict-sdlc.md
//! see decisions/ADR-010-lifecycle.md (第 3.5 节 panic hook 注册位置)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-5 panic hook + fatal 三件套)
//!
//! v0 实现保留在 legacy-prototype 分支 commit f4be188。
//! 业务模块按 ADR-003 / ADR-009 / ADR-010 / ADR-011 重新落地（P2-1.c 起）。
//!
//! PR-1：crypto module（ADR-011 crypto traits）— 2026-05-09
//! PR-2：peer module（ADR-009 PeerRegistry + RateLimiter）— 2026-05-09
//! PR-3：app module（ADR-010 Lifecycle + ClientPool + AppState）— 2026-05-09

// crypto module（ADR-011 crypto traits / ADR-008 MUST-1 AAD 绑值 / MUST-2 zeroize）
pub mod crypto;

// peer module（ADR-009 PeerRegistry / TrustState / RateLimiter）
// PR-2：纯逻辑层（struct + 状态机 + 7 单测）；client_pool 集成留 PR-3 Lifecycle。
pub mod peer;

// app module（ADR-010 Lifecycle + ClientPool + AppState）
// PR-3：基础设施三件套最后一件（启动 7 步 + 关闭 7 步 + Phase 状态机 + panic hook）
pub mod app;

// network module（ADR-003 第 3.2 节 12 端点 + ADR-008 MUST-3/6/7/8）
// PR-4：axum router skeleton + handlers + error 层 + lifecycle step 5 真正 bind
pub mod network;

// commands module（PR-FE-0 Tauri IPC 命令层）
// 为前端 UI（PR-FE-1/2/3）提供所有 IPC 命令入口
pub mod commands;

// ---------------------------------------------------------------------------
// Tauri 入口（ADR-010 第 3.5 节：panic hook 在最早入口注册）
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // ADR-010 第 3.5 节 + ADR-008 MUST-5：
    // panic hook 必须在最早入口（在 Builder::default() 之前 + Lifecycle::new() 之前）注册，
    // 确保 Tauri Builder init 自身的 panic 也被捕获。
    // 注意：不在 Lifecycle::start 内注册（runtime 可能已死时 hook 不能依赖 Tauri）。
    install_panic_hook();

    // 初始化 tracing（PR-3 最小外壳；tracing-appender rolling 留 PR-4）
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(true)
        .try_init()
        .ok();

    // 构造 AppState（ADR-010 第 3.2 节 step 3 顺序在 AppState::new() 内）
    let app_state = app::state::AppState::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            quit_app,
            // PR-FE-0：前端 UI 命令（specs: floating-window / settings-panel / history-list / group-approval / group-discovery）
            commands::get_status,
            commands::get_peers,
            commands::join_group,
            commands::get_config,
            commands::set_config,
            commands::approve_peer,
            commands::reject_peer,
            commands::get_history,
            commands::delete_history_item,
            commands::clear_history,
            commands::recopy_history_item,
        ])
        .setup(|app| {
            use tauri::Manager as _;

            tracing::info!(
                version = app.package_info().version.to_string(),
                "Sync Copy v2 started (PR-3 lifecycle scaffold)"
            );

            // Lifecycle::start 在 setup 闭包内 async 执行
            // ADR-010 第 3.2 节：start() 7 步占位
            let app_handle = app.handle().clone();
            let state = app.state::<crate::app::state::AppState>().inner().clone();

            // PR-FE-1b：注入 AppHandle 到 AppState，使 axum handler 可 emit Tauri 事件。
            // 此时 Tauri runtime 已就绪（setup 回调内），AppHandle 有效。
            // 注入后 axum handshake handler 可 emit "peer-pending" 事件（group-approval 弹框）。
            *state.app_handle.write() = Some(app_handle.clone());
            tracing::debug!(target: "app::state", "AppHandle injected into AppState (PR-FE-1b)");

            let lifecycle = state.lifecycle.clone();

            tauri::async_runtime::spawn(async move {
                if let Err(e) = lifecycle.start(&app_handle, &state).await {
                    tracing::error!(
                        target: "lifecycle",
                        error = %e,
                        "startup failed; process will exit"
                    );
                    // 启动失败：Phase → Dead（start 内部已设），进程退出
                    std::process::abort();
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Sync Copy 启动失败：tauri runtime 初始化错");
}

// ---------------------------------------------------------------------------
// panic hook（ADR-010 第 3.5 节 + ADR-008 MUST-5）
// ---------------------------------------------------------------------------

/// 注册全局 panic hook。
///
/// 约束（ADR-008 MUST-5 + ADR-010 第 3.5 节）：
/// 1. hook 不依赖 Tauri runtime（不调 app.emit / tauri::dialog）
/// 2. hook 只记 location + payload 字面（不含 backtrace 栈变量值）
/// 3. dialog 文案不显示 panic message 字面（防用户截图泄露敏感信息）
/// 4. mac/Win cfg 隔离 native dialog；Linux fallback eprintln
/// 5. process::abort 不静默（fatal 三件套 #3）
fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // 1. 取 location + payload 字面（ADR-008 MUST-5：不含运行时变量插值）
        let loc = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "<unknown>".into());

        let msg: &str = info
            .payload()
            .downcast_ref::<&'static str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("<non-string panic payload>");

        // 2. fatal 三件套 #1：eprintln + tracing::error
        // step 1 之前 panic（tracing 未 init）→ tracing! 是 no-op，仅 eprintln
        eprintln!("[FATAL] panic at {} : {}", loc, msg);
        tracing::error!(
            target: "panic",
            location = %loc,
            payload = %msg,
            "fatal panic"
        );

        // 3. fatal 三件套 #2：native dialog 兜底（文案不含 payload）
        // ADR-010 第 3.5 节：文案统一 — 不显示 panic message，防截图泄露
        show_native_fatal_dialog(&loc);

        // SECURITY (ADR-008 MUST-5): 默认 backtrace 含函数符号 + 行号，
        // release 模式不含栈变量值；进 stderr 不进文件。已审接受面。
        // ADR-010 第 7.3 节 P1 补丁：保留原 hook 链让 OS / runtime 默认 backtrace 仍生效。
        prev(info);

        // 4. fatal 三件套 #3：process::abort 不静默
        // v4-7 硬约束；不允许 std::process::exit(0) 静默吞 panic
        std::process::abort();
    }));
}

/// 显示系统原生 fatal 错误对话框（不依赖 Tauri runtime）。
///
/// mac/Win cfg 隔离（ADR-010 第 3.5 节 + ADR-008 第 7.2 节 7.3 节 P1 补丁）。
/// 文案不含 panic payload（ADR-008 第 6.1 节）。
fn show_native_fatal_dialog(location: &str) {
    // SECURITY (ADR-008 第 6.1 节 + ADR-010 第 3.5 节):
    // 文案不含 panic message 字面（防特定 payload 让用户截图发敏感数据）。
    // location 是编译期 file!:line! 值，攻击者无法控制其内容。
    // SECURITY: location is compile-time file!:line!, attacker-uncontrollable.

    #[cfg(target_os = "macos")]
    {
        // macOS：用 osascript 子进程弹 AppleScript dialog
        // 不调 Tauri AppHandle（runtime 可能已死）
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(format!(
                "display alert \"Sync Copy 致命错误\" message \"程序遇到致命错误，已写入日志。请导出日志后联系开发者。\\n\\n位置: {}\" as critical",
                location
            ))
            .spawn();
        // 不等待 osascript 完成（spawn 后立即返回，让 abort 及时执行）
    }

    #[cfg(target_os = "windows")]
    {
        // Windows：用 Win32 MessageBoxW（通过 std::process::Command powershell 兜底）
        // 不调 Tauri AppHandle（runtime 可能已死）
        let msg = format!(
            "Sync Copy 遇到致命错误，已写入日志（{}）。请导出日志后联系开发者。",
            location
        );
        let _ = std::process::Command::new("powershell")
            .arg("-Command")
            .arg(format!(
                "[System.Windows.Forms.MessageBox]::Show('{}', 'Sync Copy 致命错误', 'OK', 'Error')",
                msg
            ))
            .spawn();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // Linux / 其他：eprintln fallback
        eprintln!("[FATAL DIALOG] Sync Copy 遇到致命错误，位置: {}", location);
    }
}

// ---------------------------------------------------------------------------
// Tauri IPC 命令（最小集 — quit_app）
// ---------------------------------------------------------------------------

/// quit_app — 4 退出路径收敛入口（ADR-010 第 3.4 节）。
///
/// 调用 Lifecycle::shutdown（7 步关闭序列）。
/// 重入幂等：Lifecycle::shutdown 内部 Phase 检查保证只执行一次。
///
/// 4 退出路径（ADR-010 第 3.4 节）：
///   1. 托盘菜单 退出：PR-4 落地（tray-integration P0 暂用 app.exit，P2 升级到此）
///   2. 设置面板 退出按钮：前端 invoke("quit_app")
///   3. macOS Cmd+Q：on_window_event CloseRequested + prevent_close() + invoke
///   4. Windows X 关闭：同上
///
/// TODO(ADR-010 第 3.4 节): P0 tray quit 路径升级到 quit_app at P2
#[tauri::command]
async fn quit_app(state: tauri::State<'_, app::state::AppState>) -> Result<(), String> {
    let lifecycle = state.lifecycle.clone();
    let app_state = state.inner().clone();

    // 注意：不持 state 锁过 await（编码风格硬要求）
    let _duration = lifecycle.shutdown(&app_state).await;

    tracing::info!(target: "lifecycle", "quit_app: shutdown complete via IPC command");
    Ok(())
}
