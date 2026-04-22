use parking_lot::RwLock;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::VecDeque;

const MAX_HISTORY: usize = 50;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    Local,
    Remote { device_name: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HistoryPayload {
    Text {
        text: String,
    },
    Image {
        width: u32,
        height: u32,
        data_url: String,
    },
    File {
        filename: String,
        size: u64,
        /// 接收端：本地保存的绝对路径；发送端或失败时为 None
        saved_path: Option<String>,
        /// "sent" | "received" | "failed"
        file_status: String,
        /// 失败原因
        error: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryItem {
    pub id: String,
    pub timestamp_ms: u64,
    pub source: Source,
    /// 内容 hash（SHA-256 hex），用于跨机器同步删除。对于同一份文本/图片/文件，
    /// 两台机器算出来的 hash 一致。None 表示无法跨机器识别（保守）
    pub content_hash: Option<String>,
    #[serde(flatten)]
    pub payload: HistoryPayload,
}

pub struct History {
    items: RwLock<VecDeque<HistoryItem>>,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

impl History {
    pub fn new() -> Self {
        Self {
            items: RwLock::new(VecDeque::new()),
        }
    }

    fn insert(
        &self,
        payload: HistoryPayload,
        content_hash: Option<String>,
        source: Source,
    ) -> Option<HistoryItem> {
        let mut items = self.items.write();
        // 去重：按 content_hash（存在时）或按内容等值
        if let Some(ref h) = content_hash {
            if matches!(items.front(), Some(top) if top.content_hash.as_deref() == Some(h)) {
                return None;
            }
            items.retain(|it| it.content_hash.as_deref() != Some(h));
        }
        let item = HistoryItem {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp_ms: now_ms(),
            source,
            content_hash,
            payload,
        };
        items.push_front(item.clone());
        while items.len() > MAX_HISTORY {
            items.pop_back();
        }
        Some(item)
    }

    pub fn push_text(&self, text: String, source: Source) -> Option<HistoryItem> {
        let hash = sha256_hex(text.as_bytes());
        self.insert(HistoryPayload::Text { text }, Some(hash), source)
    }

    pub fn push_image(
        &self,
        width: u32,
        height: u32,
        data_url: String,
        content_hash: String,
        source: Source,
    ) -> Option<HistoryItem> {
        self.insert(
            HistoryPayload::Image {
                width,
                height,
                data_url,
            },
            Some(content_hash),
            source,
        )
    }

    pub fn push_file(
        &self,
        filename: String,
        size: u64,
        saved_path: Option<String>,
        file_status: &str,
        error: Option<String>,
        content_hash: Option<String>,
        source: Source,
    ) -> Option<HistoryItem> {
        self.insert(
            HistoryPayload::File {
                filename,
                size,
                saved_path,
                file_status: file_status.to_string(),
                error,
            },
            content_hash,
            source,
        )
    }

    pub fn snapshot(&self) -> Vec<HistoryItem> {
        self.items.read().iter().cloned().collect()
    }

    pub fn remove(&self, id: &str) -> Option<HistoryItem> {
        let mut items = self.items.write();
        let idx = items.iter().position(|it| it.id == id)?;
        items.remove(idx)
    }

    /// 按内容 hash 删除所有匹配项，返回是否删掉东西
    pub fn remove_by_hash(&self, content_hash: &str) -> bool {
        let mut items = self.items.write();
        let before = items.len();
        items.retain(|it| it.content_hash.as_deref() != Some(content_hash));
        items.len() != before
    }

    pub fn clear(&self) {
        self.items.write().clear();
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
