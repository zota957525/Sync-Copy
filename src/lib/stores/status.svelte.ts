/**
 * Status store — Svelte 5 runes
 *
 * Svelte 5 规则：.svelte.ts 中不能 export 被重新赋值的顶层 $state。
 * 解决方案：将所有状态封装在单一对象里，export 该对象（对象引用不变，只改属性）。
 *
 * 持有全局连接状态，供 FloatingWindow / StatusBar / Footer 消费。
 * 通过 ipc.getStatus() 拉取，通过 ipc.onStatusUpdated() 订阅更新。
 */

import { getStatus, onStatusUpdated } from "$lib/ipc";
import type { UnlistenFn } from "@tauri-apps/api/event";

// ---------------------------------------------------------------------------
// 全局状态对象（属性可变，对象引用不变 → 满足 Svelte 5 export 约束）
// ---------------------------------------------------------------------------

export const statusStore = $state({
  myDeviceId:    "",
  listenAddr:    "--",
  peerCount:     0,
  approvedCount: 0,
  bannedCount:   0,
  loading:       false,
  lastError:     null as string | null,
});

// ---------------------------------------------------------------------------
// refreshStatus：拉取一次状态并更新 store
// ---------------------------------------------------------------------------

export async function refreshStatus(): Promise<void> {
  statusStore.loading = true;
  try {
    const s = await getStatus();
    statusStore.myDeviceId    = s.my_device_id;
    statusStore.listenAddr    = s.listen_addr;
    statusStore.peerCount     = s.peer_count;
    statusStore.approvedCount = s.approved_count;
    statusStore.bannedCount   = s.banned_count;
    statusStore.lastError     = null;
  } catch (e) {
    statusStore.lastError = e instanceof Error ? e.message : String(e);
  } finally {
    statusStore.loading = false;
  }
}

// ---------------------------------------------------------------------------
// initStatusStore：挂载时调用，返回清理函数
// ---------------------------------------------------------------------------

/**
 * 1. 立即拉取一次状态（首屏数据）
 * 2. 订阅 status-updated 事件，每次 emit 时刷新
 *
 * 返回 UnlistenFn，须在 onDestroy 里调用。
 */
export async function initStatusStore(): Promise<UnlistenFn> {
  await refreshStatus();
  return onStatusUpdated(refreshStatus);
}
