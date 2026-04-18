use parking_lot::RwLock;
use serde::Serialize;
use std::collections::VecDeque;

const MAX_HISTORY: usize = 50;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    /// 本机自己复制的内容
    Local,
    /// 来自 LAN 里某台 peer 的同步
    Remote { device_name: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryItem {
    pub id: String,
    pub text: String,
    pub timestamp_ms: u64,
    pub source: Source,
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

    /// 推入新条目。若已有相同文本的条目，先移除旧的再插入（bump 到顶部）
    /// 如果要推入的文本和当前顶部一致，直接返回 None，避免无意义的 bump/刷新
    pub fn push(&self, text: String, source: Source) -> Option<HistoryItem> {
        let mut items = self.items.write();
        if matches!(items.front(), Some(top) if top.text == text) {
            return None;
        }
        items.retain(|it| it.text != text);
        let item = HistoryItem {
            id: uuid::Uuid::new_v4().to_string(),
            text,
            timestamp_ms: now_ms(),
            source,
        };
        items.push_front(item.clone());
        while items.len() > MAX_HISTORY {
            items.pop_back();
        }
        Some(item)
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
