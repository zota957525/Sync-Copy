use std::{
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

use tauri::{AppHandle, Emitter};

use crate::{history::Source, network, state::AppState};

const MAX_TEXT_BYTES: usize = 1_000_000;
const POLL_INTERVAL: Duration = Duration::from_millis(1000);
const TICK_INTERVAL: Duration = Duration::from_millis(80);

/// 主线程和剪切板线程之间的指令
pub enum ClipboardCmd {
    /// 把文本写入系统剪切板，并抑制下一次本地轮询把它当新内容再次广播
    SetSuppress(String),
}

/// 启动剪切板线程，返回发送端。调用方应把发送端存到 AppState.clipboard_tx
pub fn spawn(
    app: AppHandle,
    state: Arc<AppState>,
) -> mpsc::Sender<ClipboardCmd> {
    let (tx, rx) = mpsc::channel::<ClipboardCmd>();
    thread::spawn(move || run(app, state, rx));
    tx
}

fn run(app: AppHandle, state: Arc<AppState>, rx: mpsc::Receiver<ClipboardCmd>) {
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(cb) => cb,
        Err(e) => {
            tracing::error!(error = %e, "clipboard init failed, polling disabled");
            return;
        }
    };
    let mut last_seen: Option<String> = None;
    let mut last_poll = Instant::now() - POLL_INTERVAL;

    loop {
        // 1. 处理所有待写入请求
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                ClipboardCmd::SetSuppress(text) => {
                    match clipboard.set_text(&text) {
                        Ok(()) => {
                            tracing::debug!(bytes = text.len(), "clipboard set from remote");
                            last_seen = Some(text);
                        }
                        Err(e) => tracing::warn!(error = %e, "clipboard set_text failed"),
                    }
                }
            }
        }

        // 2. 到时间就轮询
        if last_poll.elapsed() >= POLL_INTERVAL {
            last_poll = Instant::now();
            if let Ok(text) = clipboard.get_text() {
                if !text.is_empty()
                    && text.len() <= MAX_TEXT_BYTES
                    && last_seen.as_deref() != Some(&text)
                {
                    last_seen = Some(text.clone());
                    if state.history.push(text.clone(), Source::Local).is_some() {
                        let _ = app.emit("history-updated", ());
                    }
                    // 异步广播给所有 peer
                    let state_cl = Arc::clone(&state);
                    let text_cl = text.clone();
                    tauri::async_runtime::spawn(async move {
                        network::client::broadcast_clipboard(state_cl, text_cl).await;
                    });
                }
            }
        }

        thread::sleep(TICK_INTERVAL);
    }
}
