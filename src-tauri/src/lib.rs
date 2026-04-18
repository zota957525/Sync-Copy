mod clipboard;
mod commands;
mod config;
mod history;
mod network;
mod peer;
mod state;

use std::sync::Arc;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
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
            commands::join_group,
            commands::leave_group,
            commands::get_local_ip,
            commands::respond_handshake,
        ])
        .setup(move |app| {
            build_tray(app.handle())?;
            // 启动剪切板线程并把 tx 存进 AppState
            let tx = clipboard::spawn(app.handle().clone(), Arc::clone(&app_state));
            *app_state.clipboard_tx.lock() = Some(tx);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
                    let _ = w.show();
                    let _ = w.set_focus();
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
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}
