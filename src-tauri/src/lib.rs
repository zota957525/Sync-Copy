mod clipboard;
mod commands;
mod config;
mod crypto;
mod history;
mod network;
mod peer;
mod state;

use std::sync::Arc;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, PhysicalPosition, WebviewWindow,
};

use crate::{config::Config, state::AppState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,sync_copy_lib=debug")),
        )
        .with_target(false)
        .try_init()
        .ok();

    let config = Config::load_or_default();
    if let Err(e) = config.save() {
        tracing::warn!(error = %e, "failed to persist initial config");
    }
    let app_state = AppState::new(config);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state.clone())
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config,
            commands::get_status,
            commands::get_peers,
            commands::get_history,
            commands::delete_history_item,
            commands::clear_history,
            commands::recopy_history_item,
            commands::join_group,
            commands::leave_group,
            commands::get_local_ip,
            commands::respond_handshake,
            commands::respond_file_save,
            commands::send_files,
            commands::quit_app,
            commands::hide_window,
            commands::reveal_file,
        ])
        .setup(move |app| {
            build_tray(app.handle())?;
            // 启动剪切板线程并把 tx 存进 AppState
            let tx = clipboard::spawn(app.handle().clone(), Arc::clone(&app_state));
            *app_state.clipboard_tx.lock() = Some(tx);

            // 已配置密码 → 自动上线，省掉用户手动点「上线」
            let state_cl = Arc::clone(&app_state);
            let app_cl = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                commands::auto_listen_on_startup(state_cl, app_cl).await;
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// 如果窗口大部分在屏幕外（边缘吸附隐藏 / 多屏幕切换等情况），把它挪回当前显示器中央。
/// 不破坏用户自己拖到的正常位置。
pub fn ensure_on_screen(w: &WebviewWindow) {
    let (Ok(pos), Ok(size), Ok(Some(monitor))) = (
        w.outer_position(),
        w.outer_size(),
        w.current_monitor(),
    ) else {
        return;
    };
    let mpos = monitor.position();
    let msize = monitor.size();

    let size_w = size.width as i32;
    let size_h = size.height as i32;
    let msize_w = msize.width as i32;
    let msize_h = msize.height as i32;

    let vx0 = pos.x.max(mpos.x);
    let vy0 = pos.y.max(mpos.y);
    let vx1 = (pos.x + size_w).min(mpos.x + msize_w);
    let vy1 = (pos.y + size_h).min(mpos.y + msize_h);
    let visible_w = (vx1 - vx0).max(0);
    let visible_h = (vy1 - vy0).max(0);

    // 至少一半宽高都在屏幕内才算"没丢"
    let mostly_visible = visible_w * 2 >= size_w && visible_h * 2 >= size_h;
    if mostly_visible {
        return;
    }

    let cx = mpos.x + (msize_w - size_w) / 2;
    let cy = mpos.y + (msize_h - size_h) / 2;
    let _ = w.set_position(PhysicalPosition::new(cx, cy));
    tracing::info!(new_x = cx, new_y = cy, "window was mostly off-screen, recentered");
}

fn build_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let show_i = MenuItem::with_id(app, "show", "显示浮窗", true, None::<&str>)?;
    let hide_i = MenuItem::with_id(app, "hide", "隐藏浮窗", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_i, &hide_i, &quit_i])?;

    let _ = TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().cloned().unwrap())
        .tooltip("Sync Copy")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    ensure_on_screen(&w);
                    let _ = w.show();
                    let _ = w.set_focus();
                    let _ = app.emit("window-shown", ());
                }
            }
            "hide" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    if w.is_visible().unwrap_or(false) {
                        let _ = w.hide();
                    } else {
                        ensure_on_screen(&w);
                        let _ = w.show();
                        let _ = w.set_focus();
                        let _ = app.emit("window-shown", ());
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}
