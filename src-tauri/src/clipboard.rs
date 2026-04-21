use std::{
    borrow::Cow,
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

use arboard::ImageData;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};

use crate::{history::Source, network, state::AppState};

const MAX_TEXT_BYTES: usize = 1_000_000;
const MAX_IMAGE_BYTES: usize = 5_000_000; // PNG 5 MB 上限
const POLL_INTERVAL: Duration = Duration::from_millis(1000);
const TICK_INTERVAL: Duration = Duration::from_millis(80);

pub enum ClipboardCmd {
    /// 把文本写入剪切板并抑制下一次本地轮询回传
    SetTextSuppress(String),
    /// 把图片（PNG 字节）写入剪切板并抑制回传
    SetImageSuppress {
        png: Vec<u8>,
        width: u32,
        height: u32,
    },
}

pub fn spawn(app: AppHandle, state: Arc<AppState>) -> mpsc::Sender<ClipboardCmd> {
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
    let mut last_text: Option<String> = None;
    let mut last_image_hash: Option<[u8; 32]> = None;
    let mut last_poll = Instant::now() - POLL_INTERVAL;

    loop {
        // 1. 处理所有待写入请求
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                ClipboardCmd::SetTextSuppress(text) => match clipboard.set_text(&text) {
                    Ok(()) => {
                        last_text = Some(text);
                    }
                    Err(e) => tracing::warn!(error = %e, "clipboard set_text failed"),
                },
                ClipboardCmd::SetImageSuppress { png, width, height } => {
                    match decode_png_to_rgba(&png) {
                        Ok(rgba) => {
                            let img = ImageData {
                                width: width as usize,
                                height: height as usize,
                                bytes: Cow::Owned(rgba),
                            };
                            match clipboard.set_image(img) {
                                Ok(()) => {
                                    last_image_hash = Some(hash_bytes(&png));
                                    // 图片进剪切板时，文本可能被系统替换为空。重置 last_text 避免误识别
                                    last_text = None;
                                }
                                Err(e) => tracing::warn!(error = %e, "clipboard set_image failed"),
                            }
                        }
                        Err(e) => tracing::warn!(error = %e, "decode png failed"),
                    }
                }
            }
        }

        // 2. 轮询
        if last_poll.elapsed() >= POLL_INTERVAL {
            last_poll = Instant::now();

            // 先看图片
            let mut handled_image = false;
            if let Ok(img) = clipboard.get_image() {
                let width = img.width as u32;
                let height = img.height as u32;
                if width > 0 && height > 0 {
                    match encode_rgba_to_png(width, height, &img.bytes) {
                        Ok(png_bytes) => {
                            if png_bytes.len() <= MAX_IMAGE_BYTES {
                                let h = hash_bytes(&png_bytes);
                                if last_image_hash != Some(h) {
                                    last_image_hash = Some(h);
                                    // 图片变了：写入历史 + 广播
                                    let data_url =
                                        format!("data:image/png;base64,{}", B64.encode(&png_bytes));
                                    if state
                                        .history
                                        .push_image(width, height, data_url, Source::Local)
                                        .is_some()
                                    {
                                        let _ = app.emit("history-updated", ());
                                    }
                                    let state_cl = Arc::clone(&state);
                                    let png_cl = png_bytes.clone();
                                    tauri::async_runtime::spawn(async move {
                                        network::client::broadcast_image(
                                            state_cl, png_cl, width, height,
                                        )
                                        .await;
                                    });
                                    handled_image = true;
                                } else {
                                    handled_image = true; // 图片没变但"图片仍是当前内容"
                                }
                            } else {
                                tracing::debug!(
                                    bytes = png_bytes.len(),
                                    "image too large, skipping"
                                );
                                handled_image = true;
                            }
                        }
                        Err(e) => tracing::warn!(error = %e, "encode png failed"),
                    }
                }
            }

            // 没有图片才看文本（避免截图时把 image 的元数据文本也发出去）
            if !handled_image {
                if let Ok(text) = clipboard.get_text() {
                    if !text.is_empty()
                        && text.len() <= MAX_TEXT_BYTES
                        && last_text.as_deref() != Some(&text)
                    {
                        last_text = Some(text.clone());
                        if state
                            .history
                            .push_text(text.clone(), Source::Local)
                            .is_some()
                        {
                            let _ = app.emit("history-updated", ());
                        }
                        let state_cl = Arc::clone(&state);
                        let text_cl = text.clone();
                        tauri::async_runtime::spawn(async move {
                            network::client::broadcast_text(state_cl, text_cl).await;
                        });
                    }
                }
            }
        }

        thread::sleep(TICK_INTERVAL);
    }
}

fn hash_bytes(b: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b);
    h.finalize().into()
}

fn encode_rgba_to_png(width: u32, height: u32, rgba: &[u8]) -> anyhow::Result<Vec<u8>> {
    use image::ImageEncoder;
    let mut out = Vec::with_capacity(rgba.len() / 4);
    let encoder = image::codecs::png::PngEncoder::new_with_quality(
        &mut out,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::NoFilter,
    );
    encoder.write_image(rgba, width, height, image::ExtendedColorType::Rgba8)?;
    Ok(out)
}

fn decode_png_to_rgba(png: &[u8]) -> anyhow::Result<Vec<u8>> {
    let dynimg = image::load_from_memory_with_format(png, image::ImageFormat::Png)?;
    Ok(dynimg.into_rgba8().into_raw())
}
