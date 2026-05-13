/**
 * History store — Svelte 5 runes
 *
 * 管理历史列表状态，供 HistoryList 组件消费。
 * 规则（同 status.svelte.ts 模式）：状态封装在单一对象，对象引用不变。
 *
 * 命令（src-tauri/src/commands.rs 已验证）：
 *   get_history / delete_history_item / clear_history / recopy_history_item
 * 事件（src-tauri/src/lib.rs 已验证）：
 *   history-updated（delete/clear 后 emit）
 */

import {
  getHistory,
  deleteHistoryItem,
  clearHistory,
  recopyHistoryItem,
  onHistoryUpdated,
} from "$lib/ipc";
import type { HistoryItem } from "$lib/types";
import type { UnlistenFn } from "@tauri-apps/api/event";

// ---------------------------------------------------------------------------
// 全局状态对象
// ---------------------------------------------------------------------------

export const historyStore = $state({
  items: [] as HistoryItem[],
  loading: false,
  error: null as string | null,
});

// ---------------------------------------------------------------------------
// refresh：拉取历史列表一次
// ---------------------------------------------------------------------------

export async function refreshHistory(): Promise<void> {
  historyStore.loading = true;
  try {
    const items = await getHistory();
    historyStore.items = items;
    historyStore.error = null;
  } catch (e) {
    historyStore.error = e instanceof Error ? e.message : String(e);
  } finally {
    historyStore.loading = false;
  }
}

// ---------------------------------------------------------------------------
// del：乐观删除单条 + 触发 backend
// ---------------------------------------------------------------------------

export async function delHistoryItem(id: string): Promise<void> {
  // 乐观更新：先从本地移除（50ms 内消失体验）
  const prev = historyStore.items;
  historyStore.items = prev.filter((item) => item.id !== id);
  try {
    await deleteHistoryItem(id);
    // backend 会 emit history-updated，触发 refreshHistory，不需要重拉
  } catch (e) {
    // 回滚乐观更新
    historyStore.items = prev;
    historyStore.error = e instanceof Error ? e.message : String(e);
  }
}

// ---------------------------------------------------------------------------
// clearAll：清空所有历史
// ---------------------------------------------------------------------------

export async function clearAllHistory(): Promise<void> {
  const prev = historyStore.items;
  historyStore.items = [];
  try {
    await clearHistory();
  } catch (e) {
    historyStore.items = prev;
    historyStore.error = e instanceof Error ? e.message : String(e);
  }
}

// ---------------------------------------------------------------------------
// recopy：把历史条目写回剪切板
// 返回 true = 成功，false = 失败（file 条目返回 invalid_input）
// ---------------------------------------------------------------------------

export async function recopyItem(id: string): Promise<boolean> {
  try {
    await recopyHistoryItem(id);
    return true;
  } catch (_e) {
    return false;
  }
}

// ---------------------------------------------------------------------------
// initHistoryStore：挂载时调用，订阅事件 + 首屏拉取
// ---------------------------------------------------------------------------

export async function initHistoryStore(): Promise<UnlistenFn> {
  await refreshHistory();
  return onHistoryUpdated(refreshHistory);
}
