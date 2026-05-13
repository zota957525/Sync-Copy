<script lang="ts">
  /**
   * 主入口 — 单层引用 FloatingWindow
   *
   * 职责：
   * 1. 挂载时调 initStatusStore()（status 订阅 + 首屏拉取）
   * 2. 挂载时订阅 peer-pending 事件 → pushPending 到 approvalStore
   * 3. 卸载时清理所有 unlisten + approval timer
   * 4. 不超过 80 行（v0 教训：单文件 1483 行）
   */
  import { onMount, onDestroy } from "svelte";
  import FloatingWindow from "$lib/components/FloatingWindow.svelte";
  import { initStatusStore } from "$lib/stores/status.svelte";
  import { pushPending, cleanupApprovalStore } from "$lib/stores/approval.svelte";
  import { onPeerPending } from "$lib/ipc";
  import type { UnlistenFn } from "@tauri-apps/api/event";

  let unlistenStatus: UnlistenFn | null = null;
  let unlistenPeerPending: UnlistenFn | null = null;

  onMount(async () => {
    // 1. 初始化 status store：拉取首屏 + 订阅 status-updated 事件
    unlistenStatus = await initStatusStore();

    // 2. 订阅 peer-pending 事件 → 推入 approvalStore 队列（PR-FE-2）
    unlistenPeerPending = await onPeerPending((payload) => {
      pushPending(payload);
    });
  });

  onDestroy(() => {
    unlistenStatus?.();
    unlistenPeerPending?.();
    cleanupApprovalStore();
  });
</script>

<FloatingWindow />
