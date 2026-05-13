<script lang="ts">
  /**
   * ClearConfirm — 清除历史内联确认（spec settings-panel 第 6.3 节）
   */
  interface Props {
    oncancel: () => void;
    onconfirm: () => void;
    disabled?: boolean;
  }

  let { oncancel, onconfirm, disabled = false }: Props = $props();
</script>

<div class="clear-confirm">
  <p class="hint">将同步清空所有设备的历史</p>
  <p class="hint">无法恢复</p>
  <div class="btns">
    <button class="btn-ghost" onclick={oncancel} {disabled}>取消</button>
    <button class="btn-danger" onclick={onconfirm} {disabled}>确认清除</button>
  </div>
</div>

<style>
  .clear-confirm {
    background: rgba(255,255,255,0.04);
    border: 1px solid rgba(255,255,255,0.08);
    border-radius: 6px;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .hint {
    font-size: 11px;
    color: #9ca3af;
    margin: 0;
    line-height: 1.4;
  }
  .btns {
    display: flex;
    gap: 6px;
    margin-top: 6px;
  }
  .btn-ghost, .btn-danger {
    border: none;
    border-radius: 6px;
    padding: 4px 10px;
    font-size: 12px;
    cursor: pointer;
    font-family: inherit;
    transition: filter 80ms ease;
  }
  .btn-ghost:disabled, .btn-danger:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .btn-ghost {
    background: rgba(255,255,255,0.12);
    color: #f3f4f6;
  }
  .btn-ghost:hover:not(:disabled) { background: rgba(255,255,255,0.18); }
  .btn-danger {
    background: #ef4444;
    color: #fff;
  }
  .btn-danger:hover:not(:disabled) { filter: brightness(1.1); }
</style>
