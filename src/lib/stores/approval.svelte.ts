/**
 * Approval store — Svelte 5 runes
 *
 * 管理 peer-pending 审批队列（spec group-approval 第 3 节）。
 *
 * 设计：
 * - pendingQueue: PeerPendingPayload[] — 先进先出
 * - 始终展示 queue[0]；处理完后 shift
 * - 30s 本地 timer：若 backend 未发 dismiss，自动移除当前头部
 *
 * 注意：backend 目前无 handshake-dismissed 事件（grep src-tauri 确认），
 * dismiss 依赖 approve/reject 操作后本地移除 + 30s 超时。
 */

import type { PeerPendingPayload } from "$lib/types";

// ---------------------------------------------------------------------------
// 状态
// ---------------------------------------------------------------------------

export const approvalStore = $state({
  queue: [] as PeerPendingPayload[],
  /** 当前头部是否已发送决定（等待后端确认 / 自动 dismiss）*/
  sentDecision: false,
});

// 30s 自动 dismiss timer（只跟 queue[0] 绑定）
let autoTimer: ReturnType<typeof setTimeout> | null = null;

// ---------------------------------------------------------------------------
// 内部：启动/停止 30s 自动 dismiss
// ---------------------------------------------------------------------------

function startAutoTimer(): void {
  stopAutoTimer();
  if (approvalStore.queue.length === 0) return;

  const head = approvalStore.queue[0];
  const elapsed = Date.now() - head.timestamp_ms;
  const remaining = Math.max(0, 30_000 - elapsed);

  autoTimer = setTimeout(() => {
    shiftQueue();
  }, remaining);
}

function stopAutoTimer(): void {
  if (autoTimer !== null) {
    clearTimeout(autoTimer);
    autoTimer = null;
  }
}

function shiftQueue(): void {
  stopAutoTimer();
  approvalStore.queue.shift();
  approvalStore.sentDecision = false;
  startAutoTimer();
}

// ---------------------------------------------------------------------------
// 公开 API
// ---------------------------------------------------------------------------

/**
 * 收到 peer-pending 事件时推入队列。
 * 若队列之前为空，启动 timer。
 */
export function pushPending(payload: PeerPendingPayload): void {
  // 去重：同一 request_id 不重复入队
  if (approvalStore.queue.some((p) => p.request_id === payload.request_id)) return;
  const wasEmpty = approvalStore.queue.length === 0;
  approvalStore.queue.push(payload);
  if (wasEmpty) startAutoTimer();
}

/**
 * 用户点"同意"或"拒绝"后调用：标记已发送，移出队列。
 * approve/reject IPC 调用由组件自行处理，此函数只做状态清理。
 */
export function dismissCurrent(): void {
  shiftQueue();
}

/**
 * 当前头部的剩余秒数（每秒在组件侧用 setInterval 读取）。
 */
export function remainingSeconds(): number {
  if (approvalStore.queue.length === 0) return 0;
  const head = approvalStore.queue[0];
  const elapsed = Math.floor((Date.now() - head.timestamp_ms) / 1000);
  return Math.max(0, 30 - elapsed);
}

/**
 * 组件卸载时调用，清理 timer。
 */
export function cleanupApprovalStore(): void {
  stopAutoTimer();
}
