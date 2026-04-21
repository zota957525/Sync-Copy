use parking_lot::RwLock;
use serde::Serialize;
use std::collections::VecDeque;

const MAX_HISTORY: usize = 50;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    Local,
    Remote { device_name: String },
}

/// 历史条目内容：文本或图片
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HistoryPayload {
    Text {
        text: String,
    },
    Image {
        width: u32,
        height: u32,
        /// PNG 图像 base64，用作前端 <img src="data:image/png;base64,..."> 的缩略图
        data_url: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryItem {
    pub id: String,
    pub timestamp_ms: u64,
    pub source: Source,
    #[serde(flatten)]
    pub payload: HistoryPayload,
}

pub struct History {
    items: RwLock<VecDeque<HistoryItem>>,
}

impl History {
    pub fn new() -> Self {
        Self {
            items: RwLock::new(VecDeque::new()),
        }
    }

    fn push_inner(&self, payload: HistoryPayload, source: Source) -> Option<HistoryItem> {
        let mut items = self.items.write();
        // 去重：文本按内容，图片按 data_url
        let matches_top = |top: &HistoryItem| match (&top.payload, &payload) {
            (HistoryPayload::Text { text: a }, HistoryPayload::Text { text: b }) => a == b,
            (
                HistoryPayload::Image { data_url: a, .. },
                HistoryPayload::Image { data_url: b, .. },
            ) => a == b,
            _ => false,
        };
        if matches!(items.front(), Some(top) if matches_top(top)) {
            return None;
        }
        // 删掉所有同内容的旧项，再 bump 到顶部
        items.retain(|it| !matches_top(it));

        let item = HistoryItem {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp_ms: now_ms(),
            source,
            payload,
        };
        items.push_front(item.clone());
        while items.len() > MAX_HISTORY {
            items.pop_back();
        }
        Some(item)
    }

    pub fn push_text(&self, text: String, source: Source) -> Option<HistoryItem> {
        self.push_inner(HistoryPayload::Text { text }, source)
    }

    pub fn push_image(
        &self,
        width: u32,
        height: u32,
        data_url: String,
        source: Source,
    ) -> Option<HistoryItem> {
        self.push_inner(
            HistoryPayload::Image {
                width,
                height,
                data_url,
            },
            source,
        )
    }

    pub fn snapshot(&self) -> Vec<HistoryItem> {
        self.items.read().iter().cloned().collect()
    }

    pub fn remove(&self, id: &str) -> bool {
        let mut items = self.items.write();
        let before = items.len();
        items.retain(|it| it.id != id);
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
