<script lang="ts">
  /**
   * 主入口 — 单层引用 FloatingWindow
   *
   * 职责：
   * 1. 挂载时调 initStatusStore()（status 订阅 + 首屏拉取）
   * 2. 挂载时调 initHistoryStore()（history 订阅 + 首屏拉取）
   * 3. 挂载时订阅 peer-pending 事件 → pushPending 到 approvalStore
   * 4. 挂载时订阅 window-shown 事件 → 触发 refreshStatus + refreshHistory
   * 5. 卸载时清理所有 unlisten + approval timer
   * 6. 不超过 80 行（v0 教训：单文件 1483 行）
   */
  import { onMount, onDestroy } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import FloatingWindow from "$lib/components/FloatingWindow.svelte";
  import { initStatusStore, refreshStatus } from "$lib/stores/status.svelte";
  import { initHistoryStore, refreshHistory } from "$lib/stores/history.svelte";
  import { pushPending, cleanupApprovalStore } from "$lib/stores/approval.svelte";
  import { onPeerPending } from "$lib/ipc";
  import type { UnlistenFn } from "@tauri-apps/api/event";

  let unlistenStatus: UnlistenFn | null = null;
  let unlistenHistory: UnlistenFn | null = null;
  let unlistenPeerPending: UnlistenFn | null = null;
  let unlistenWindowShown: UnlistenFn | null = null;

  onMount(async () => {
    // 1. 初始化 status store：拉取首屏 + 订阅 status-updated 事件
    unlistenStatus = await initStatusStore();

    // 2. 初始化 history store：拉取首屏 + 订阅 history-updated 事件
    unlistenHistory = await initHistoryStore();

    // 3. 订阅 peer-pending 事件 → 推入 approvalStore 队列（PR-FE-2）
    unlistenPeerPending = await onPeerPending((payload) => {
      pushPending(payload);
    });

    // 4. 订阅 window-shown 事件（托盘唤出时触发）→ 刷新状态 + 历史
    //    backend: src-tauri/src/lib.rs show_window handler emit("window-shown", ())
    unlistenWindowShown = await listen<void>("window-shown", () => {
      refreshStatus();
      refreshHistory();
    });
  });

  onDestroy(() => {
    unlistenStatus?.();
    unlistenHistory?.();
    unlistenPeerPending?.();
    unlistenWindowShown?.();
    cleanupApprovalStore();
  });
</script>

<FloatingWindow />
