<script lang="ts">
  /**
   * StatusDot — 状态圆点
   * props: state / size
   * 颜色严格按 floating-window.md 第 6.5 节状态点颜色字典。
   */
  import {
    COLOR_DOT_CONNECTED,
    COLOR_DOT_DISCONNECTED,
    COLOR_DOT_ERROR,
    COLOR_DOT_PENDING,
    DOT_SIZE_DEFAULT,
  } from "$lib/style/tokens";

  interface Props {
    /** 连接状态 */
    state: "connected" | "disconnected" | "error" | "warning";
    /** 圆点直径 px（默认 DOT_SIZE_DEFAULT = 8）*/
    size?: number;
  }

  let { state, size = DOT_SIZE_DEFAULT }: Props = $props();

  const colorMap: Record<Props["state"], string> = {
    connected:    COLOR_DOT_CONNECTED,
    disconnected: COLOR_DOT_DISCONNECTED,
    error:        COLOR_DOT_ERROR,
    warning:      COLOR_DOT_PENDING,
  };

  let color = $derived(colorMap[state] ?? COLOR_DOT_DISCONNECTED);
</script>

<span
  class="dot"
  style="width:{size}px; height:{size}px; background:{color};"
  aria-label={state}
></span>

<style>
  .dot {
    display: inline-block;
    border-radius: 50%;
    flex-shrink: 0;
    /* 轻微 glow，增强状态可见性 */
    box-shadow: 0 0 4px currentColor;
  }
</style>
