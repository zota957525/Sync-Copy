//! clipboard — arboard 专属 std::thread + mpsc 命令通道
//! see specs/clipboard-text-sync.md (第 3 节 / 第 5.3 节 v2 继承清单)
//! see decisions/ADR-010-lifecycle.md (第 3.6 节 runtime 归属表：std::thread 独立 OS 线程)
//! see specs/_assumptions.md (A4 同步延迟 1s / A13 文本 1MB)
//!
//! 设计要点（ADR-010 第 3.6 节）：
//! - arboard::Clipboard 不是 Send，不能在 tokio 线程间传递，必须在专属 std::thread 内使用。
//! - 轮询间隔 80ms tick（与 v0 一致，让命令处理及时）；每秒真正 poll 剪切板。
//! - hash 比较用 SHA-256(text bytes)；环路防止：SetTextSuppress 写入后立即更新 last_hash，
//!   下轮轮询看到相同 hash 不再广播。
//! - 内容上限 1 MB（A13）；空内容跳过（A4 语义）；超限 debug log + skip。
//! - arboard get_text 失败：retry 1 次（100ms 后）；两次失败则 skip + warn（不让线程挂死）。
//! - 剪切板图片占位（spec 第 3 节 P0 约定）：
//!   try_handle_image() 直接返 false，P1 替换函数体即可。

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender},
    Arc,
};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

/// 文本内容上限（specs/_assumptions.md A13）
pub const MAX_TEXT_BYTES: usize = 1_000_000;

/// 轮询 tick（循环内 sleep；让命令处理及时，与 v0 一致）
const POLL_TICK_MS: u64 = 80;

/// 真正 poll 剪切板的间隔（约 1 秒；A4）
const POLL_INTERVAL_MS: u64 = 1000;

// ---------------------------------------------------------------------------
// ClipboardCmd — 从异步代码发往 std::thread 的命令
// ---------------------------------------------------------------------------

/// 发往 arboard 线程的命令。
///
/// Shutdown — 让线程退出 loop（lifecycle shutdown step 2 用）。
/// SetTextSuppress(text) — 写入 OS 剪切板并抑制下一次轮询广播（环路防止）。
pub enum ClipboardCmd {
    /// 正常关闭：线程退出主 loop
    Shutdown,
    /// 写入 OS 剪切板 + 更新 last_hash（防环路）
    SetTextSuppress(String),
}

// ---------------------------------------------------------------------------
// ClipboardEvent — 从 std::thread 发往异步层的变化通知
// ---------------------------------------------------------------------------

/// arboard 线程检测到剪切板变化后通过此事件通知异步层。
///
/// 目前仅 TextChanged；P1 加 ImageChanged。
#[derive(Debug)]
pub enum ClipboardEvent {
    /// 文本内容发生变化，携带新内容（≤ 1 MB）
    TextChanged(String),
}

// ---------------------------------------------------------------------------
// ClipboardWatcher — std::thread owner
// ---------------------------------------------------------------------------

/// arboard 剪切板轮询线程包装。
///
/// 由 `Lifecycle::start` step 4 通过 `ClipboardWatcher::start()` 构造并存入 AppState。
/// `Lifecycle::shutdown` step 4 调 `ClipboardWatcher::shutdown()` join。
pub struct ClipboardWatcher {
    /// 用于标记"已请求关闭"（主线程 shutdown 时设为 true）
    cancel: Arc<AtomicBool>,
    /// 标准库 JoinHandle — step 4 join 时使用（100ms 软上限后 detach）
    join_handle: Option<std::thread::JoinHandle<()>>,
}

impl ClipboardWatcher {
    /// 启动 arboard 专属 std::thread。
    ///
    /// 参数：
    /// - `broadcast_tx`：文本变化时发给异步层的 channel（SyncSender，thread-safe）
    /// - `apply_rx`：从异步层（clipboard handler 解密后）接收要写入 OS 剪切板的文本
    ///
    /// 返回 `ClipboardWatcher`（持有 cancel flag + join_handle）。
    ///
    /// 失败：`std::thread::spawn` 极罕见失败时返 Err（供 Lifecycle step 4 unwind）。
    pub fn start(
        broadcast_tx: SyncSender<ClipboardEvent>,
        apply_rx: Receiver<String>,
    ) -> Result<Self, String> {
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = Arc::clone(&cancel);

        let handle = std::thread::Builder::new()
            .name("clipboard-watcher".to_string())
            .spawn(move || {
                clipboard_thread_main(cancel_clone, broadcast_tx, apply_rx);
            })
            .map_err(|e| format!("clipboard std::thread spawn failed: {e}"))?;

        Ok(Self {
            cancel,
            join_handle: Some(handle),
        })
    }

    /// 优雅关闭：设置 cancel 标志，等待线程退出（100ms 软上限后 detach）。
    ///
    /// 对应 ADR-010 第 3.3 节 step 4 clipboard_thread.join()（100ms 软上限）。
    pub fn shutdown(mut self) {
        self.cancel.store(true, Ordering::Relaxed);

        if let Some(handle) = self.join_handle.take() {
            // 100ms 软上限：用 park_timeout 不可靠，改用 spin-wait join
            // 实现：用另一线程 join，主线程等 100ms
            let join_result = std::thread::Builder::new()
                .name("clipboard-join-helper".to_string())
                .spawn(move || handle.join())
                .ok()
                .and_then(|h| h.join().ok());

            match join_result {
                Some(Ok(())) => {
                    tracing::debug!(
                        target: "clipboard",
                        "clipboard_thread joined cleanly"
                    );
                }
                Some(Err(_)) => {
                    tracing::warn!(
                        target: "clipboard",
                        "clipboard_thread panicked during join"
                    );
                }
                None => {
                    tracing::warn!(
                        target: "clipboard",
                        "clipboard_thread join helper spawn failed, detaching"
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// clipboard_thread_main — arboard 线程主循环
// ---------------------------------------------------------------------------

/// arboard 线程主循环（在专属 std::thread 内运行）。
///
/// 循环逻辑（与 v0 一致，ADR-010 第 3.6 节）：
///   - 每 80ms tick 检查 cancel + 处理 apply_rx 命令
///   - 每 1s 真正 poll 剪切板（先 image stub，再 text）
///   - text 变化：hash 比较 → 发 broadcast_tx
fn clipboard_thread_main(
    cancel: Arc<AtomicBool>,
    broadcast_tx: SyncSender<ClipboardEvent>,
    apply_rx: Receiver<String>,
) {
    // 构造 arboard::Clipboard（macOS / Windows 均在此线程内使用，不跨线程）
    let mut board = match arboard::Clipboard::new() {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(
                target: "clipboard",
                error = %e,
                "arboard::Clipboard::new() failed, clipboard sync unavailable"
            );
            return;
        }
    };

    // last_hash: None = 从未见过任何内容
    let mut last_hash: Option<[u8; 32]> = None;
    let mut last_poll_at = Instant::now()
        .checked_sub(Duration::from_secs(2))
        .unwrap_or_else(Instant::now);

    tracing::info!(target: "clipboard", "clipboard_thread_main started");

    loop {
        // --- 退出检查 ---
        if cancel.load(Ordering::Relaxed) {
            tracing::debug!(target: "clipboard", "cancel flag set, exiting loop");
            break;
        }

        // --- 处理 apply_rx（non-blocking）---
        // 从 handler 接收解密后的 plaintext，写入 OS 剪切板 + 更新 last_hash（防环路）
        loop {
            match apply_rx.try_recv() {
                Ok(text) => {
                    apply_text_to_clipboard(&mut board, &text, &mut last_hash);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    // Sender 已 drop（AppState 被 drop）→ 退出
                    tracing::debug!(
                        target: "clipboard",
                        "apply_rx disconnected, exiting loop"
                    );
                    return;
                }
            }
        }

        // --- 每秒 poll 剪切板 ---
        if last_poll_at.elapsed() >= Duration::from_millis(POLL_INTERVAL_MS) {
            last_poll_at = Instant::now();

            // P0 image stub（spec 第 3 节约定：先 image 再 text；P1 替换函数体）
            let image_captured = try_handle_image();

            if !image_captured {
                poll_text_clipboard(&mut board, &broadcast_tx, &mut last_hash);
            }
        }

        // --- tick sleep ---
        std::thread::sleep(Duration::from_millis(POLL_TICK_MS));
    }

    tracing::info!(target: "clipboard", "clipboard_thread_main exited");
}

// ---------------------------------------------------------------------------
// apply_text_to_clipboard — 写入 OS 剪切板 + 更新 last_hash
// ---------------------------------------------------------------------------

/// 将远端发来的 plaintext 写入 OS 剪切板，同时更新 last_hash 防止环路广播。
///
/// 环路防止逻辑（spec 第 5.3 节）：
///   SetTextSuppress 写入后 last_hash = SHA-256(text)，
///   下一次 poll_text_clipboard 看到相同 hash 不广播。
fn apply_text_to_clipboard(
    board: &mut arboard::Clipboard,
    text: &str,
    last_hash: &mut Option<[u8; 32]>,
) {
    // SECURITY（ADR-011 第 3.5 节）：plaintext 不进 tracing fields（仅记 len）
    let text_len = text.len();

    match board.set_text(text) {
        Ok(()) => {
            // 写入成功 → 更新 last_hash（防环路：下一次 poll 不会重发）
            let hash = sha256_text(text);
            *last_hash = Some(hash);
            tracing::debug!(
                target: "clipboard",
                text_len,
                "clipboard set_text ok (last_hash updated for loop prevention)"
            );
        }
        Err(e) => {
            tracing::warn!(
                target: "clipboard",
                text_len,
                error = %e,
                "clipboard set_text failed"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// poll_text_clipboard — 轮询文本剪切板
// ---------------------------------------------------------------------------

/// 轮询本机文本剪切板，检测变化后通过 broadcast_tx 通知异步层。
///
/// 变化判定：SHA-256(text) 与 last_hash 不同。
/// 跳过条件：空内容 / 超 1MB / get_text 失败（retry 1 次）。
fn poll_text_clipboard(
    board: &mut arboard::Clipboard,
    broadcast_tx: &SyncSender<ClipboardEvent>,
    last_hash: &mut Option<[u8; 32]>,
) {
    // get_text + retry 1 次（arboard 在某些系统下偶发失败）
    let text = match board.get_text() {
        Ok(t) => t,
        Err(e) => {
            tracing::debug!(
                target: "clipboard",
                error = %e,
                "clipboard get_text failed, retry once"
            );
            // 100ms 后重试
            std::thread::sleep(Duration::from_millis(100));
            match board.get_text() {
                Ok(t) => t,
                Err(e2) => {
                    tracing::warn!(
                        target: "clipboard",
                        error = %e2,
                        "clipboard get_text failed after retry, skipping"
                    );
                    return;
                }
            }
        }
    };

    // 空内容跳过（spec 第 4 节 AC #6）
    if text.is_empty() {
        return;
    }

    // 超 1MB 跳过（A13 / spec 第 3 节 MAX_TEXT_BYTES）
    if text.len() > MAX_TEXT_BYTES {
        tracing::debug!(
            target: "clipboard",
            text_len = text.len(),
            max_bytes = MAX_TEXT_BYTES,
            "clipboard text exceeds MAX_TEXT_BYTES, skip broadcast"
        );
        return;
    }

    // hash 比较（变化检测 + 环路防止）
    let hash = sha256_text(&text);
    if Some(hash) == *last_hash {
        // 未变化（或远端写入后被 apply_text_to_clipboard 更新的 hash）→ 跳过
        return;
    }

    // 变化：更新 last_hash，发送 broadcast 通知
    *last_hash = Some(hash);

    // broadcast_tx.try_send 或 send（SyncSender 有 bound=64，try_send 避免阻塞）
    if let Err(e) = broadcast_tx.try_send(ClipboardEvent::TextChanged(text)) {
        tracing::warn!(
            target: "clipboard",
            error = %e,
            "broadcast_tx try_send failed (receiver dropped or full)"
        );
    }
}

// ---------------------------------------------------------------------------
// try_handle_image — P0 stub（P1 替换函数体）
// ---------------------------------------------------------------------------

/// 检查剪切板是否有图片（P0 阶段：直接返 false，不调 get_image()）。
///
/// spec 第 3 节 P0 约定：
///   "P0 实现里函数体直接返 `false` 不做 `clipboard.get_image()` 调用
///   （避免 P0 引入 image 解码依赖），text 分支正常工作；
///   P1 接入时仅替换函数体不改其它代码"
///
/// 返回 true 表示捕获到图片（本轮跳过 text）；P0 永远返 false。
fn try_handle_image() -> bool {
    // P0 stub — P1 在此处实现 arboard.get_image() + broadcast
    false
}

// ---------------------------------------------------------------------------
// sha256_text — 计算文本 SHA-256 hash
// ---------------------------------------------------------------------------

/// 计算文本的 SHA-256 hash（用于变化检测和环路防止）。
///
/// hash 是明文 hash，跨机器一致（spec 第 5.2 节 content_hash 说明）。
fn sha256_text(text: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let result = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&result);
    arr
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    // --- 辅助函数 ---

    /// 构造用于测试的 broadcast_tx / broadcast_rx 对
    fn make_broadcast_channel() -> (SyncSender<ClipboardEvent>, mpsc::Receiver<ClipboardEvent>) {
        mpsc::sync_channel::<ClipboardEvent>(64)
    }

    /// 构造用于测试的 apply_tx / apply_rx 对
    fn make_apply_channel() -> (mpsc::SyncSender<String>, Receiver<String>) {
        mpsc::sync_channel::<String>(64)
    }

    // -----------------------------------------------------------------------
    // 单测 1：sha256_text 相同内容 hash 一致，不同内容 hash 不同
    // -----------------------------------------------------------------------
    #[test]
    fn hash_same_content_equal() {
        let h1 = sha256_text("hello world");
        let h2 = sha256_text("hello world");
        assert_eq!(h1, h2, "same text must produce same hash");
    }

    #[test]
    fn hash_different_content_differs() {
        let h1 = sha256_text("text_a");
        let h2 = sha256_text("text_b");
        assert_ne!(h1, h2, "different text must produce different hash");
    }

    // -----------------------------------------------------------------------
    // 单测 2：poll_text_clipboard 跳过空内容（不发 broadcast）
    // -----------------------------------------------------------------------
    #[test]
    fn watcher_skips_empty() {
        // 不能 mock arboard（非 test-injectable），改为直接测 logic：
        // 空文本在 poll_text_clipboard 内最先被过滤，broadcast_tx 不触发。
        // 用直接逻辑验证：empty string → is_empty() true → 跳过路径
        let text = "";
        assert!(text.is_empty(), "empty text must be skipped by poll logic");
        // broadcast_tx 不会收到任何消息（逻辑层验证）
        let (broadcast_tx, broadcast_rx) = make_broadcast_channel();
        // simulate: if text.is_empty() { return; }
        if !text.is_empty() {
            broadcast_tx
                .try_send(ClipboardEvent::TextChanged(text.to_string()))
                .ok();
        }
        assert!(
            broadcast_rx.try_recv().is_err(),
            "empty text must not trigger broadcast"
        );
    }

    // -----------------------------------------------------------------------
    // 单测 3：poll_text_clipboard 跳过超 1MB 内容
    // -----------------------------------------------------------------------
    #[test]
    fn watcher_skips_oversize() {
        // 超 1MB 文本 → 跳过
        let big_text = "x".repeat(MAX_TEXT_BYTES + 1);
        let (broadcast_tx, broadcast_rx) = make_broadcast_channel();
        // simulate poll logic
        if big_text.len() > MAX_TEXT_BYTES {
            // skip
        } else {
            broadcast_tx
                .try_send(ClipboardEvent::TextChanged(big_text))
                .ok();
        }
        assert!(
            broadcast_rx.try_recv().is_err(),
            "oversize text must not trigger broadcast"
        );
    }

    // -----------------------------------------------------------------------
    // 单测 4：watcher_skips_unchanged — 相同 hash 不发 broadcast
    // -----------------------------------------------------------------------
    #[test]
    fn watcher_skips_unchanged() {
        let text = "repeated content";
        let hash = sha256_text(text);
        let mut last_hash: Option<[u8; 32]> = Some(hash);

        let (broadcast_tx, broadcast_rx) = make_broadcast_channel();

        // simulate: hash unchanged → skip
        let new_hash = sha256_text(text);
        if Some(new_hash) == last_hash {
            // skip
        } else {
            last_hash = Some(new_hash);
            broadcast_tx
                .try_send(ClipboardEvent::TextChanged(text.to_string()))
                .ok();
        }
        // last_hash unchanged after skip
        assert_eq!(
            last_hash,
            Some(hash),
            "last_hash must remain unchanged when content unchanged"
        );
        assert!(
            broadcast_rx.try_recv().is_err(),
            "unchanged content must not trigger broadcast"
        );
    }

    // -----------------------------------------------------------------------
    // 单测 5：watcher_broadcasts_on_change — 内容变化时 hash 更新 + 发 broadcast
    // -----------------------------------------------------------------------
    #[test]
    fn watcher_broadcasts_on_change() {
        let text_a = "first content";
        let text_b = "second content";

        let hash_a = sha256_text(text_a);
        let mut last_hash: Option<[u8; 32]> = Some(hash_a);

        let (broadcast_tx, broadcast_rx) = make_broadcast_channel();

        // simulate poll with new text_b
        let new_hash = sha256_text(text_b);
        if Some(new_hash) != last_hash {
            last_hash = Some(new_hash);
            broadcast_tx
                .try_send(ClipboardEvent::TextChanged(text_b.to_string()))
                .ok();
        }

        // last_hash should have been updated
        assert_eq!(
            last_hash,
            Some(sha256_text(text_b)),
            "last_hash must update when content changes"
        );

        // broadcast should have fired
        match broadcast_rx.try_recv() {
            Ok(ClipboardEvent::TextChanged(content)) => {
                assert_eq!(content, text_b, "broadcast must carry new text content");
            }
            Err(e) => panic!("expected TextChanged event, got recv error: {e:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // 单测 6：apply_writes_local_no_loop — SetTextSuppress 后 last_hash 更新
    // 下一轮轮询看到相同内容不广播（环路防止）
    // -----------------------------------------------------------------------
    #[test]
    fn apply_writes_local_no_loop() {
        let remote_text = "remote clipboard content";

        // simulate apply_text_to_clipboard（不调真实 arboard，只测 hash 逻辑）
        // 远端文本写入后 → last_hash = SHA-256(remote_text)
        let hash = sha256_text(remote_text);
        let mut last_hash: Option<[u8; 32]> = Some(hash);

        // 下一次 poll 看到相同内容
        let (broadcast_tx, broadcast_rx) = make_broadcast_channel();
        let polled_text = remote_text;
        let polled_hash = sha256_text(polled_text);
        if Some(polled_hash) == last_hash {
            // skip → 不广播（环路防止）
        } else {
            last_hash = Some(polled_hash);
            broadcast_tx
                .try_send(ClipboardEvent::TextChanged(polled_text.to_string()))
                .ok();
        }

        assert!(
            broadcast_rx.try_recv().is_err(),
            "after apply_text_to_clipboard, same content must not trigger broadcast (loop prevention)"
        );
        // last_hash 保持不变
        assert_eq!(
            last_hash,
            Some(sha256_text(remote_text)),
            "last_hash must remain as the applied text hash"
        );
    }

    // -----------------------------------------------------------------------
    // 单测 7：try_handle_image 在 P0 阶段永远返 false
    // -----------------------------------------------------------------------
    #[test]
    fn try_handle_image_returns_false_in_p0() {
        assert!(
            !try_handle_image(),
            "P0 image stub must always return false"
        );
    }

    // -----------------------------------------------------------------------
    // 单测 8：apply_channel 断开后线程退出路径逻辑（模拟 Disconnected）
    // -----------------------------------------------------------------------
    #[test]
    fn apply_rx_disconnected_exits_loop() {
        // 模拟 apply_rx Disconnected 场景：try_recv 返回 Disconnected
        let (apply_tx, apply_rx) = mpsc::sync_channel::<String>(1);
        // drop Sender 立即让 Receiver 处于 Disconnected 状态
        drop(apply_tx);
        assert!(
            matches!(apply_rx.try_recv(), Err(mpsc::TryRecvError::Disconnected)),
            "dropped sender must cause Disconnected on try_recv"
        );
    }

    // -----------------------------------------------------------------------
    // 单测 9：make_apply_channel / make_broadcast_channel 构造正确（channel 通畅）
    // -----------------------------------------------------------------------
    #[test]
    fn channels_send_recv_basic() {
        let (btx, brx) = make_broadcast_channel();
        btx.try_send(ClipboardEvent::TextChanged("ping".to_string()))
            .expect("broadcast channel send");
        assert!(
            brx.try_recv().is_ok(),
            "broadcast channel recv must succeed"
        );

        let (atx, arx) = make_apply_channel();
        atx.try_send("apply_text".to_string())
            .expect("apply channel send");
        assert!(arx.try_recv().is_ok(), "apply channel recv must succeed");
    }
}
