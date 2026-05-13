<script lang="ts">
  /**
   * 主入口 — 单层引用 FloatingWindow
   *
   * 职责：
   * 1. 挂载时调 initStatusStore()（status 订阅 + 首屏拉取）
   * 2. 挂载时监听 peer-pending 事件（本批 console.log；PR-FE-2 做完整弹框）
   * 3. 不超过 100 行（v0 教训：单文件 1483 行）
   */
  import { onMount, onDestroy } from "svelte";
  import FloatingWindow from "$lib/components/FloatingWindow.svelte";
  import { initStatusStore } from "$lib/stores/status.svelte";
  import { onPeerPending } from "$lib/ipc";
  import type { UnlistenFn } from "@tauri-apps/api/event";

  let unlistenStatus: UnlistenFn | null = null;
  let unlistenPeerPending: UnlistenFn | null = null;

  onMount(async () => {
    // 1. 初始化 status store：拉取首屏 + 订阅 status-updated 事件
    unlistenStatus = await initStatusStore();

    // 2. 订阅 peer-pending 事件（PR-FE-2 将在此处打开 ApprovalDialog）
    unlistenPeerPending = await onPeerPending((payload) => {
      console.log("[peer-pending]", payload);
    });
  });

  onDestroy(() => {
    unlistenStatus?.();
    unlistenPeerPending?.();
  });
</script>

<FloatingWindow />
