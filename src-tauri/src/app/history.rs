//! in-memory 历史列表 store
//! see specs/history-list.md (第 3 节 HistoryItem 结构 / MAX_HISTORY=50)
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.5 节 AppState)
//!
//! 设计决策：
//! - 进程退出即清（spec 第 3 节 out of scope: 历史不持久化）
//! - VecDeque + RwLock + MAX_HISTORY=50（v0 沿用，spec 第 5.3 节 v2 应继承）
//! - push 时 content_hash 去重：head match → skip；非 head 同 hash → retain 移除 + push_front
//! - 不在本 store 里做 arboard 写入（那是 commands.rs recopy 的职责）
//!
//! PR-FE-0 范围：
//! - HistoryStore struct + push / get / remove / clear / snapshot 方法
//! - HistoryPayload enum（text / image / file 三态）

use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// HistoryPayload（spec 第 3 节 payload tagged enum）
// ---------------------------------------------------------------------------

/// 历史条目的内容载体。
///
/// 三态（text / image / file）对应 spec history-list.md 第 3 节。
#[derive(Debug, Clone)]
pub enum HistoryPayload {
    /// 纯文本剪切板内容。
    Text { text: String },
    /// 图片（PNG）——以 base64 data_url 形式保存。
    Image {
        width: u32,
        height: u32,
        /// base64 encoded PNG bytes（前端直接用 `data:image/png;base64,...`）
        data_b64: String,
    },
    /// 文件条目。
    File {
        filename: String,
        /// 字节数
        size: u64,
        /// 本机保存路径（发送方为 None，接收方 Some）
        saved_path: Option<String>,
        /// "sent" | "received" | "failed"
        file_status: String,
        /// 失败原因（file_status = "failed" 时）
        error: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// HistorySource（spec 第 3 节 source 字段）
// ---------------------------------------------------------------------------

/// 条目来源。
#[derive(Debug, Clone)]
pub enum HistorySource {
    /// 本机产生
    Local,
    /// 来自远端 peer（device_name 已 sanitize）
    Remote { device_name: String },
}

// ---------------------------------------------------------------------------
// HistoryEntry — 单条历史记录
// ---------------------------------------------------------------------------

/// 单条历史记录。
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// UUID v4 字符串（唯一标识）
    pub id: String,
    /// 创建时间（毫秒，UNIX epoch）
    pub timestamp_ms: u64,
    /// 来源（本机 / 远端）
    pub source: HistorySource,
    /// 内容哈希（SHA-256 hex；用于去重）
    pub content_hash: Option<String>,
    /// 内容载体
    pub payload: HistoryPayload,
}

// ---------------------------------------------------------------------------
// HistoryStore — in-memory 历史列表
// ---------------------------------------------------------------------------

/// 最大历史条数（spec history-list.md 第 3 节）。
pub const MAX_HISTORY: usize = 50;

/// in-memory 历史列表 store。
///
/// 使用 `Arc<HistoryStore>` 挂到 `AppState`，让 commands 和 network handler 共享访问。
/// RwLock<VecDeque<HistoryEntry>> 支持并发读 + 单写；写操作时间短（不含 IO）。
pub struct HistoryStore {
    inner: RwLock<VecDeque<HistoryEntry>>,
}

impl HistoryStore {
    /// 构造空历史 store。
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: RwLock::new(VecDeque::new()),
        })
    }

    /// 推入新历史条目（含去重 + MAX_HISTORY 截尾）。
    ///
    /// 去重规则（spec 第 3 节 + v0 第 5.3 节）：
    /// 1. 新条目 content_hash 与 head 相同 → 跳过（不更新）
    /// 2. 新条目 content_hash 与非 head 相同 → `retain` 移除旧条 + `push_front` 新条（保持最新位置）
    /// 3. content_hash = None → 不做去重，直接 push_front
    pub fn push(&self, entry: HistoryEntry) {
        let mut inner = self.inner.write();

        if let Some(ref hash) = entry.content_hash {
            // 规则 1：head 相同 hash → 跳过
            if let Some(head) = inner.front() {
                if head.content_hash.as_deref() == Some(hash.as_str()) {
                    return;
                }
            }
            // 规则 2：移除非 head 的同 hash 旧条
            let hash_owned = hash.clone();
            inner.retain(|e| e.content_hash.as_deref() != Some(&hash_owned));
        }

        // push_front（最新在顶部）
        inner.push_front(entry);

        // MAX_HISTORY 截尾：弹出最旧
        while inner.len() > MAX_HISTORY {
            inner.pop_back();
        }

        tracing::debug!(
            target: "app::history",
            count = inner.len(),
            "history entry pushed"
        );
    }

    /// 按 id 移除单条历史，返回是否找到并移除。
    pub fn remove(&self, id: &str) -> bool {
        let mut inner = self.inner.write();
        let before = inner.len();
        inner.retain(|e| e.id != id);
        let removed = inner.len() < before;
        if removed {
            tracing::debug!(target: "app::history", id = %id, "history entry removed");
        }
        removed
    }

    /// 清空所有历史。
    pub fn clear(&self) {
        self.inner.write().clear();
        tracing::debug!(target: "app::history", "history cleared");
    }

    /// 返回全部历史快照（时间倒序，最新在前）。
    pub fn snapshot(&self) -> Vec<HistoryEntry> {
        self.inner.read().iter().cloned().collect()
    }

    /// 按 id 获取单条历史（clone）。
    pub fn get(&self, id: &str) -> Option<HistoryEntry> {
        self.inner.read().iter().find(|e| e.id == id).cloned()
    }

    /// 返回当前条目数。
    pub fn count(&self) -> usize {
        self.inner.read().len()
    }
}

impl Default for HistoryStore {
    fn default() -> Self {
        Self {
            inner: RwLock::new(VecDeque::new()),
        }
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_text_entry(id: &str, hash: Option<&str>, text: &str) -> HistoryEntry {
        HistoryEntry {
            id: id.to_string(),
            timestamp_ms: 1000,
            source: HistorySource::Local,
            content_hash: hash.map(|h| h.to_string()),
            payload: HistoryPayload::Text {
                text: text.to_string(),
            },
        }
    }

    /// push + count + snapshot 基础行为
    #[test]
    fn push_and_snapshot() {
        let store = HistoryStore::new();
        store.push(make_text_entry("id-1", Some("hash-a"), "hello"));
        store.push(make_text_entry("id-2", Some("hash-b"), "world"));
        assert_eq!(store.count(), 2);
        let snap = store.snapshot();
        // 最新在前（id-2 是后 push 的）
        assert_eq!(snap[0].id, "id-2");
        assert_eq!(snap[1].id, "id-1");
    }

    /// head 同 hash → skip（去重规则 1）
    #[test]
    fn push_head_duplicate_skips() {
        let store = HistoryStore::new();
        store.push(make_text_entry("id-1", Some("hash-a"), "hello"));
        // 同 hash，head match → skip
        store.push(make_text_entry("id-2", Some("hash-a"), "hello again"));
        assert_eq!(
            store.count(),
            1,
            "duplicate head hash must not push new entry"
        );
        assert_eq!(store.snapshot()[0].id, "id-1");
    }

    /// 非 head 同 hash → 移除旧条 + push_front（去重规则 2）
    #[test]
    fn push_non_head_duplicate_moves_to_top() {
        let store = HistoryStore::new();
        store.push(make_text_entry("id-1", Some("hash-a"), "hello"));
        store.push(make_text_entry("id-2", Some("hash-b"), "world"));
        // id-2 是 head，现在重推 hash-a（非 head）
        store.push(make_text_entry("id-3", Some("hash-a"), "hello again"));
        let snap = store.snapshot();
        // id-3 应在顶部，id-1 已被移除
        assert_eq!(
            snap[0].id, "id-3",
            "new entry with old hash must be at front"
        );
        assert!(
            snap.iter().all(|e| e.id != "id-1"),
            "old entry with same hash must be removed"
        );
        assert_eq!(store.count(), 2);
    }

    /// MAX_HISTORY 截尾
    #[test]
    fn max_history_truncates_oldest() {
        let store = HistoryStore::new();
        for i in 0..(MAX_HISTORY + 5) {
            store.push(make_text_entry(
                &format!("id-{i}"),
                Some(&format!("hash-{i}")),
                &format!("text-{i}"),
            ));
        }
        assert_eq!(
            store.count(),
            MAX_HISTORY,
            "store must not exceed MAX_HISTORY"
        );
    }

    /// remove + get
    #[test]
    fn remove_and_get() {
        let store = HistoryStore::new();
        store.push(make_text_entry("id-1", Some("hash-a"), "hello"));
        assert!(store.get("id-1").is_some(), "get must find existing entry");
        let removed = store.remove("id-1");
        assert!(removed, "remove must return true");
        assert!(
            store.get("id-1").is_none(),
            "get must return None after remove"
        );
        // 移除不存在的条目
        let not_found = store.remove("nonexistent");
        assert!(!not_found, "remove nonexistent must return false");
    }

    /// clear
    #[test]
    fn clear_empties_store() {
        let store = HistoryStore::new();
        store.push(make_text_entry("id-1", Some("hash-a"), "hello"));
        store.push(make_text_entry("id-2", Some("hash-b"), "world"));
        store.clear();
        assert_eq!(store.count(), 0, "clear must empty the store");
    }
}
