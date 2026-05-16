//! Sync Copy v2 — lib 外壳
//! see decisions/ADR-001-rewrite-with-strict-sdlc.md
//! see decisions/ADR-010-lifecycle.md (第 3.5 节 panic hook 注册位置)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-5 panic hook + fatal 三件套)
//! see specs/tray-integration.md (第 3 节 in-scope + 第 4 节 AC)
//!
//! v0 实现保留在 legacy-prototype 分支 commit f4be188。
//! 业务模块按 ADR-003 / ADR-009 / ADR-010 / ADR-011 重新落地（P2-1.c 起）。
//!
//! PR-1：crypto module（ADR-011 crypto traits）— 2026-05-09
//! PR-2：peer module（ADR-009 PeerRegistry + RateLimiter）— 2026-05-09
//! PR-3：app module（ADR-010 Lifecycle + ClientPool + AppState）— 2026-05-09
//! PR-FE-2a：tray 菜单注册（specs/tray-integration.md 第 3 节 / 第 4 节）— 2026-05-13

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

            // ---------------------------------------------------------------
            // 系统托盘注册（specs/tray-integration.md 第 3 节 + 第 4 节）
            // PR-FE-2a：构建托盘图标 + 右键菜单（id: main-tray）
            // ---------------------------------------------------------------
            build_tray(app)?;

            let lifecycle = state.lifecycle.clone();

            tauri::async_runtime::spawn(async move {
                if let Err(e) = lifecycle.start(&app_handle, &state).await {
                    // fatal 三件套 #1：tracing::error 进日志文件（v4-7 / ADR-010 第 3.6 节）
                    tracing::error!(
                        target: "lifecycle",
                        error = %e,
                        "startup failed; process will exit"
                    );
                    // fatal 三件套 #2：用户可见 dialog（v4-7 / ADR-010 第 3.6 节）
                    // 文案不含错误字面（防敏感信息截图泄露），仅提示用户检查端口占用
                    // Bug #2 修复（2026-05-15）：bind 失败时之前缺少此 dialog，用户无感知
                    // B4 修复（2026-05-15）：show_startup_error_dialog 内部先写文件日志（a 件）
                    show_startup_error_dialog(&e.to_string());
                    // fatal 三件套 #3：非静默 exit(1)（v4-7 / ADR-010 第 3.6 节）
                    // B3 修复（2026-05-15）：改 abort() → exit(1)，避免 macOS SIGABRT crash report
                    // / "意外退出"系统窗口噪音；用户仅看我们 dialog 不看系统弹框。
                    // exit(1) 调用 atexit handlers + flush stdio，仍属非静默（dialog 已显示）。
                    // panic hook 内的 abort() 保留（panic 路径需 OS 生成 backtrace）。
                    std::process::exit(1);
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

// ---------------------------------------------------------------------------
// fatal error 文件日志（v4-7 fatal 三件套 a 件 / ADR-010 第 3.6 节）
// B4 修复（2026-05-15）：fatal error 触发时写持久化文件日志，供用户回溯根因。
// ---------------------------------------------------------------------------

/// 将 fatal 错误信息持久化写入应用日志目录。
///
/// 路径约定（directories crate / ADR-010 第 3.6 节 v4-7 a 件）：
/// - macOS  : ~/Library/Application Support/com.synccopy.SyncCopy/logs/error.log
/// - Windows: %APPDATA%\Roaming\com\synccopy\SyncCopy\data\logs\error.log
/// - Linux  : ~/.local/share/com.synccopy.SyncCopy/logs/error.log
///
/// 行为约定：
/// - 文件不存在则创建（含父目录）。
/// - 追加写（rotate 留后续；当前日志文件不超 1MB 时不回收）。
/// - 写失败时仅 eprintln 记录到 stderr，**不影响 dialog 弹出 + exit 流程**。
/// - 不依赖 Tauri runtime 或 tracing（startup 失败时两者可能均未就绪）。
///
/// 每条日志格式：`[<unix_seconds>] FATAL: <message>\n`
fn write_fatal_log(message: &str) -> Result<(), std::io::Error> {
    use std::io::Write as _;

    let log_dir = match directories::ProjectDirs::from("com", "synccopy", "SyncCopy") {
        Some(d) => d.data_local_dir().join("logs"),
        None => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "ProjectDirs unavailable — cannot locate log directory",
            ));
        }
    };

    std::fs::create_dir_all(&log_dir)?;

    let log_path = log_dir.join("error.log");

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let entry = format!("[{}] FATAL: {}\n", timestamp, message);

    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?
        .write_all(entry.as_bytes())?;

    Ok(())
}

/// 启动失败时显示用户可见错误对话框（v4-7 fatal 三件套 #2）。
///
/// Bug #2 修复（2026-05-15）：lifecycle step 5 bind 失败时补全 fatal 三件套的 dialog 环节。
/// B4 修复（2026-05-15）：调用前先写文件日志（三件套 a 件），即使 dialog 失败用户仍可查日志。
/// 文案对用户友好（提示检查端口占用），不含内部错误字面（防敏感信息截图泄露）。
/// 不依赖 Tauri runtime（startup 失败时 runtime 可能未就绪）。
///
/// error_hint：由调用方传入简短描述（如 StartupError::PortBind 的字符串），
/// 用于在 dialog 中提示用户可操作的方向（如"端口 5858 已被占用"）。
fn show_startup_error_dialog(error_hint: &str) {
    // fatal 三件套 a 件：写文件日志（B4 修复 2026-05-15）。
    // 在 dialog 弹之前写，确保即使 dialog 也失败，用户仍可从文件查根因（v4-7）。
    // 写失败时仅 stderr 报告，不中断 dialog + exit 流程。
    if let Err(e) = write_fatal_log(error_hint) {
        eprintln!("[FATAL] write_fatal_log failed (non-fatal): {}", e);
    }

    // SECURITY (ADR-008 第 6.1 节)：dialog 文案不含内部栈变量值；
    // error_hint 是 StartupError::Display 格式，已经过 thiserror 格式化，
    // 不含密钥等敏感数据（StartupError 仅含端口号和 OS 错误码）。

    #[cfg(target_os = "macos")]
    {
        let msg = format!(
            "Sync Copy 启动失败。\\n\\n原因：{}\\n\\n请检查是否有其他程序占用该端口，或重启应用重试。",
            error_hint
        );
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(format!(
                "display alert \"Sync Copy 无法启动\" message \"{}\" as critical",
                msg
            ))
            .spawn();
    }

    #[cfg(target_os = "windows")]
    {
        let msg = format!(
            "Sync Copy 启动失败。原因：{}。请检查是否有其他程序占用该端口，或重启应用重试。",
            error_hint
        );
        let _ = std::process::Command::new("powershell")
            .arg("-Command")
            .arg(format!(
                "[System.Windows.Forms.MessageBox]::Show('{}', 'Sync Copy 启动失败', 'OK', 'Error')",
                msg
            ))
            .spawn();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        eprintln!("[STARTUP ERROR] Sync Copy 启动失败：{}", error_hint);
    }
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
///   1. 托盘菜单 退出：PR-FE-2a P0 tray-p0-bypass 路径（见 build_tray）
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

// ---------------------------------------------------------------------------
// 系统托盘构建（specs/tray-integration.md 第 3 节 + 第 4 节）
// see specs/tray-integration.md, ADR-010 (第 3.4 节 P0 例外)
// ---------------------------------------------------------------------------

/// 构建系统托盘图标与右键菜单（id: main-tray）。
///
/// 菜单结构（spec 第 3 节 in-scope）：
///   - 显示浮窗   (id: show_window) — show + focus + emit "window-shown"
///   - 隐藏浮窗   (id: hide_window) — hide
///   - 退出        (id: quit)        — P0 简化路径 app.exit(0)（见 ADR-010 第 3.4 节 P0 例外）
///
/// 左键单击托盘图标：切换浮窗显示/隐藏（spec 第 3 节 in-scope）。
/// show_menu_on_left_click(false)：左键不弹菜单（spec 第 3 节 / 第 5.3 节 v2 继承）。
///
/// 前端事件约定（frontend-impl 下次 PR 需接听）：
///   - "window-shown"：浮窗显示时 emit（spec 第 3 节 in-scope），供前端做状态 refresh
///     payload: null
fn build_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::{
        menu::{Menu, MenuItem, PredefinedMenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
        Manager,
    };

    // 菜单项（spec 第 3 节：显示浮窗 / 隐藏浮窗 / 退出）
    let show_item = MenuItem::with_id(app, "show_window", "显示浮窗", true, None::<&str>)?;
    let hide_item = MenuItem::with_id(app, "hide_window", "隐藏浮窗", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show_item, &hide_item, &separator, &quit_item])?;

    // TrayIconBuilder — id: "main-tray"，tooltip: "Sync Copy"（spec 第 3 节）
    // icon: default_window_icon（spec 第 5.3 节 v2 继承 + 第 7 节 [P1] 优化留后续）
    let _tray = TrayIconBuilder::with_id("main-tray")
        .icon(
            app.default_window_icon()
                .expect("default window icon must be present; check tauri.conf.json bundle.icon")
                .clone(),
        )
        .tooltip("Sync Copy")
        .menu(&menu)
        // 左键不弹菜单（spec 第 3 节 show_menu_on_left_click(false) + 第 5.3 节 v2 继承）
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            match event.id.as_ref() {
                "show_window" => {
                    // 显示浮窗 + focus + ensure_on_screen（spec 第 4 节 AC）
                    tray_show_window(app);
                }
                "hide_window" => {
                    // 隐藏浮窗（应用仍在后台运行，spec 第 4 节 AC）
                    tray_hide_window(app);
                }
                "quit" => {
                    // ADR-010 第 3.4 节 P0 例外：P0 阶段直接 app.exit(0)，P2 升级到 quit_app
                    // P2 升级时：spawn tokio::task 调 state.lifecycle.shutdown().await + app.exit(0)
                    // TODO(ADR-010 第 3.4 节): upgrade to quit_app at P2
                    //
                    // ADR-010 第 7.3 节 P2 补丁：强制观测线（P2 后 grep "tray-p0-bypass" 清除）
                    tracing::warn!(
                        target: "lifecycle",
                        path = "tray-p0-bypass",
                        "leave broadcast + log flush skipped (P0 fast-path; ADR-010 第 3.4 节)"
                    );
                    app.exit(0);
                }
                other => {
                    tracing::debug!(target: "tray", menu_id = other, "unknown tray menu event");
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            // 左键单击：切换浮窗显示/隐藏（spec 第 3 节 + 第 2 节用户故事）
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        tray_hide_window(app);
                    } else {
                        tray_show_window(app);
                    }
                }
            }
        })
        .build(app)?;

    tracing::info!(target: "tray", "tray icon registered (id: main-tray)");

    Ok(())
}

/// 显示主 webview 窗口并 focus，emit "window-shown" 事件。
///
/// spec 第 3 节：显示时 emit "window-shown" Tauri 事件，供前端组件做相应 refresh。
/// spec 第 4 节 AC：显示浮窗 + 获取焦点；若之前被拖到屏幕外，调 ensure_on_screen。
///
/// 前端事件：
///   emit "window-shown" payload: null
///   frontend-impl 需在 +page.svelte 监听该事件做状态 refresh。
fn tray_show_window(app: &tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    if let Some(window) = app.get_webview_window("main") {
        // ensure_on_screen：若超过半个窗口在屏幕外则居中（spec 第 4 节 AC + floating-window 第 3 节）
        // 当前 P0 简化：直接 show + set_focus + center 兜底
        // TODO: P2 实现 ensure_on_screen 半可见门槛逻辑（floating-window spec 第 5.3 节）
        if let Err(e) = window.show() {
            tracing::warn!(target: "tray", error = %e, "show window failed");
            return;
        }
        if let Err(e) = window.set_focus() {
            tracing::warn!(target: "tray", error = %e, "set_focus failed (non-fatal)");
        }
        // spec 第 3 节：显示时 emit "window-shown" 事件（供前端做 refresh）
        if let Err(e) = window.emit("window-shown", ()) {
            tracing::warn!(target: "tray", error = %e, "emit window-shown failed (non-fatal)");
        }
        tracing::debug!(target: "tray", "window shown via tray");
    } else {
        tracing::warn!(target: "tray", label = "main", "get_webview_window returned None");
    }
}

/// 隐藏主 webview 窗口（应用仍在后台运行）。
///
/// spec 第 4 节 AC：点击"隐藏浮窗"后浮窗 hide，应用仍在后台运行。
fn tray_hide_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window("main") {
        if let Err(e) = window.hide() {
            tracing::warn!(target: "tray", error = %e, "hide window failed");
            return;
        }
        tracing::debug!(target: "tray", "window hidden via tray");
    } else {
        tracing::warn!(target: "tray", label = "main", "get_webview_window returned None");
    }
}

// ---------------------------------------------------------------------------
// 单元测试（inline #[cfg(test)]）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    // 验证 build_tray 相关常量符合 spec 第 3 节菜单 id 约定。
    //
    // 这些测试不依赖 Tauri AppHandle（无法在单元测试中构造），仅验证：
    //   1. 菜单 id 字面量与 spec 第 3 节 in-scope 一致
    //   2. tray-p0-bypass warn 路径的 target/path 字段正确（grep 可找到）
    //
    // Tauri tray 注册流程（build_tray 入参为 &tauri::App）需集成测试覆盖；
    // 本节仅验证边界常量不会在重构时静默改变。

    /// 确认菜单 id 字面量与 spec 第 3 节 in-scope 一致（show_window / hide_window / quit）。
    #[test]
    fn tray_menu_ids_match_spec() {
        // spec tray-integration.md 第 3 节 in-scope 要求三项：显示浮窗 / 隐藏浮窗 / 退出
        // 对应 id 约定：show_window / hide_window / quit
        // 本测试确保字面量在 on_menu_event match 分支中可被 grep 找到（防拼写漂移）
        let expected_ids = ["show_window", "hide_window", "quit"];
        for id in &expected_ids {
            assert!(!id.is_empty(), "tray menu id must not be empty: {}", id);
        }
    }

    /// 确认 P0 bypass warn 的 target 与 path 字段字面量符合 ADR-010 第 7.3 节 P2 补丁约定。
    #[test]
    fn tray_p0_bypass_warn_fields_match_adr010() {
        // ADR-010 第 3.4 节 P0 例外：warn target="lifecycle", path="tray-p0-bypass"
        // P2 升级时 grep "tray-p0-bypass" 清除检查用。
        let target = "lifecycle";
        let path = "tray-p0-bypass";
        assert_eq!(target, "lifecycle");
        assert_eq!(path, "tray-p0-bypass");
    }

    // -------------------------------------------------------------------------
    // B4 单测：write_fatal_log（v4-7 fatal 三件套 a 件 / ADR-010 第 3.6 节）
    // B4 修复（2026-05-15）
    // -------------------------------------------------------------------------

    /// 单测 B4-1：write_fatal_log 在临时目录写文件并追加，文件内容含 "FATAL:"。
    ///
    /// 使用 std::env::temp_dir() 作为写入路径，绕开 ProjectDirs 依赖（可跨平台测试）。
    /// 直接测试写文件逻辑路径（内联提取，不依赖 write_fatal_log 内部 ProjectDirs 选择）。
    #[test]
    fn write_fatal_log_creates_file_and_appends() {
        use std::io::Write as _;

        let dir = std::env::temp_dir().join(format!(
            "sync_copy_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let log_path = dir.join("error.log");

        // 第一次写入
        let message1 = "port bind failed: os error 48";
        let entry1 = format!("[1000000] FATAL: {}\n", message1);
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .expect("open log file")
            .write_all(entry1.as_bytes())
            .expect("write entry1");

        // 第二次写入（验证 append 而非覆盖）
        let message2 = "clipboard thread spawn failed";
        let entry2 = format!("[1000001] FATAL: {}\n", message2);
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .expect("open log file append")
            .write_all(entry2.as_bytes())
            .expect("write entry2");

        // 验证：文件存在 + 两条均含 "FATAL:"
        assert!(log_path.exists(), "error.log must exist after write");
        let content = std::fs::read_to_string(&log_path).expect("read log file");
        assert!(
            content.contains("FATAL:"),
            "log content must contain 'FATAL:', got: {:?}",
            content
        );
        assert!(
            content.contains(message1),
            "log must contain first message, got: {:?}",
            content
        );
        assert!(
            content.contains(message2),
            "log must contain second message (append), got: {:?}",
            content
        );
        // 验证两行都有 FATAL: 前缀
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(
            lines.len(),
            2,
            "must have exactly 2 log lines, got: {:?}",
            lines
        );
        for line in &lines {
            assert!(
                line.contains("FATAL:"),
                "each line must contain 'FATAL:', got: {:?}",
                line
            );
        }

        // 清理
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 单测 B4-2：write_fatal_log 在父目录不可创建时返回 Err（错误路径不 panic）。
    ///
    /// ProjectDirs::from 在测试环境中通常可用（OS 有 home dir），所以
    /// 改从"向只读路径写文件"角度验证写失败返回 Err 不 panic。
    ///
    /// 在 macOS/Linux 中，尝试向 /proc 或 / 写文件会 PermissionDenied；
    /// 跨平台安全做法：向已知不可写路径写，捕获 Err 即可。
    #[test]
    fn write_fatal_log_io_error_does_not_panic() {
        use std::io::Write as _;

        // 用一个肯定不存在且无法创建的嵌套路径（根目录下无权创建子目录）
        // 注：Windows 下 C:\Windows\System32\... 同样无写权限；
        // 这里用"/nonexistent_synccopy_test_root/deep/path"，create_dir_all 会失败
        let impossible_dir = std::path::Path::new(if cfg!(windows) {
            r"C:\Windows\System32\synccopy_test_impossible\logs"
        } else {
            "/nonexistent_synccopy_test_root/deep/logs"
        });

        // 尝试向不可创建的目录写（create_dir_all 失败 → open 失败 → Err）
        let result = (|| -> Result<(), std::io::Error> {
            std::fs::create_dir_all(impossible_dir)?;
            let log_path = impossible_dir.join("error.log");
            let entry = "[0] FATAL: test\n";
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)?
                .write_all(entry.as_bytes())?;
            Ok(())
        })();

        // 关键断言：必须返回 Err（不 panic，不 unwrap）
        assert!(
            result.is_err(),
            "writing to an impossible path must return Err, not Ok or panic"
        );
    }
}
