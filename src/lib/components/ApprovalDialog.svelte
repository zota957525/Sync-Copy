<script lang="ts">
  /**
   * ApprovalDialog — 审批弹框覆盖层（spec group-approval 第 6 节 wireframe）
   *
   * 覆盖历史列表区，顶部状态栏 + 底部 footer 仍可见。
   * 展示 approvalStore.queue[0]；30s 倒计时三色（灰→橙→红）。
   */
  import { onDestroy } from "svelte";
  import { approvalStore, dismissCurrent, remainingSeconds } from "$lib/stores/approval.svelte";
  import { approvePeer, rejectPeer } from "$lib/ipc";

  // 倒计时 tick
  let seconds = $state(remainingSeconds());
  let tickTimer: ReturnType<typeof setInterval> | null = null;

  $effect(() => {
    if (approvalStore.queue.length > 0) {
      seconds = remainingSeconds();
      if (!tickTimer) tickTimer = setInterval(() => { seconds = remainingSeconds(); }, 1_000);
    } else {
      if (tickTimer) { clearInterval(tickTimer); tickTimer = null; }
    }
  });

  onDestroy(() => { if (tickTimer) clearInterval(tickTimer); });

  let timerColor = $derived(seconds > 15 ? "#9ca3af" : seconds > 5 ? "#f59e0b" : "#ef4444");
  let timerBold = $derived(seconds <= 5);
  let extraCount = $derived(approvalStore.queue.length - 1);
  let current = $derived(approvalStore.queue[0] ?? null);
  let sending = $state(false);

  async function decide(approve: boolean): Promise<void> {
    if (sending || !current) return;
    sending = true;
    try {
      if (approve) await approvePeer(current.subject_device_id);
      else await rejectPeer(current.subject_device_id);
    } catch { /* 忽略错误；backend not_found 可能出现 */ }
    sending = false;
    dismissCurrent();
  }
</script>

{#if current}
  <div class="overlay" role="dialog" aria-modal="true" aria-label="审批申请">
    <div class="card">

      <div class="card-head">
        <span>📥 有设备申请加入</span>
        {#if extraCount > 0}
          <span class="extra">还有 {extraCount} 个待处理</span>
        {/if}
      </div>

      <div class="sep"></div>

      <div class="device-info">
        <div class="device-name">{current.subject_device_name}</div>
        <div class="device-ip">{current.subject_ip}</div>
      </div>

      <div
        class="timer"
        style:color={timerColor}
        style:font-weight={timerBold ? "700" : "400"}
      >
        ⏱ 还剩 {seconds} 秒
      </div>

      <div class="btn-row">
        <button class="btn btn-reject" onclick={() => decide(false)} disabled={sending}>
          {sending ? "已拒绝" : "拒绝"}
        </button>
        <button class="btn btn-approve" onclick={() => decide(true)} disabled={sending}>
          {sending ? "已发送 ✓" : "同意"}
        </button>
      </div>

    </div>
  </div>
{/if}

<style>
  .overlay {
    position: absolute;
    inset: 0;
    background: rgba(0,0,0,0.50);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    border-radius: inherit;
  }
  .card {
    background: rgba(28,28,32,0.96);
    border: 1px solid rgba(255,255,255,0.12);
    border-radius: 8px;
    width: 220px;
    overflow: hidden;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  }
  .card-head {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 12px 8px;
    font-size: 13px;
    color: #f3f4f6;
  }
  .extra { font-size: 11px; color: #9ca3af; }
  .sep { height: 1px; background: rgba(255,255,255,0.08); }
  .device-info { padding: 10px 12px 4px; }
  .device-name { font-size: 14px; color: #f3f4f6; font-weight: 600; margin-bottom: 2px; word-break: break-all; }
  .device-ip { font-size: 12px; color: #9ca3af; }
  .timer { font-size: 12px; padding: 6px 12px 8px; transition: color 300ms; }
  .btn-row { display: flex; gap: 6px; padding: 0 12px 12px; }
  .btn {
    flex: 1; padding: 5px 0; border-radius: 6px; border: none;
    font-size: 13px; cursor: pointer; font-family: inherit; transition: filter 80ms;
  }
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-reject { background: rgba(255,255,255,0.12); color: #f3f4f6; }
  .btn-reject:not(:disabled):hover { background: rgba(239,68,68,0.12); }
  .btn-approve { background: #3b82f6; color: #fff; }
  .btn-approve:not(:disabled):hover { filter: brightness(1.1); }
</style>
