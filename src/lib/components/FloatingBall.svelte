<script lang="ts">
  /**
   * FloatingBall — 48×48 圆形悬浮球
   *
   * 规则（floating-ball.md 第 3 节 + 第 6 节）：
   *   - 48×48 logical px，border-radius 50%
   *   - 同浮窗磨砂玻璃背景 + 1px 微高亮边
   *   - app-icon SVG 居中（28×28px）
   *   - 手势消歧：移动 ≤ 8px + 抬起 ≤ 1500ms → 单击展开
   *   - 移动 > 8px → startDragging（原生窗口拖动）
   *   - cursor: grab / grabbing
   *
   * Props：
   *   onexpand — 单击展开时的回调，父组件处理 setSize + view 切换
   */
  import { getCurrentWindow } from "@tauri-apps/api/window";

  interface Props {
    onexpand: () => void;
  }
  let { onexpand }: Props = $props();

  // ---------------------------------------------------------------------------
  // 手势消歧状态（普通变量，不需要 rune）
  // ---------------------------------------------------------------------------

  let startX = 0;
  let startY = 0;
  let startAt = 0;
  let didDrag = false;
  let moveListener: ((e: MouseEvent) => void) | null = null;
  let upListener:   ((e: MouseEvent) => void) | null = null;

  const DRAG_THRESHOLD_PX = 8;
  const CLICK_MAX_MS = 1500;

  function cleanup(): void {
    if (moveListener) { window.removeEventListener("mousemove", moveListener); moveListener = null; }
    if (upListener)   { window.removeEventListener("mouseup",   upListener);   upListener   = null; }
  }

  function onBallMouseDown(ev: MouseEvent): void {
    if (ev.button !== 0) return;
    startX  = ev.screenX;
    startY  = ev.screenY;
    startAt = Date.now();
    didDrag = false;

    moveListener = (e: MouseEvent) => {
      const dx = Math.abs(e.screenX - startX);
      const dy = Math.abs(e.screenY - startY);
      if (Math.max(dx, dy) > DRAG_THRESHOLD_PX) {
        didDrag = true;
        cleanup();
        getCurrentWindow().startDragging().catch(() => {/* non-fatal */});
      }
    };

    upListener = (_e: MouseEvent) => {
      cleanup();
      const elapsed = Date.now() - startAt;
      if (!didDrag && elapsed < CLICK_MAX_MS) {
        onexpand();
      }
    };

    window.addEventListener("mousemove", moveListener);
    window.addEventListener("mouseup",   upListener);
  }
</script>

<div
  class="ball"
  role="button"
  tabindex="-1"
  aria-label="展开 Sync Copy"
  onmousedown={onBallMouseDown}
>
  <!-- App icon SVG（与浮窗顶栏 logo 同款，28×28px 居中） -->
  <svg
    width="28"
    height="28"
    viewBox="0 0 32 32"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    aria-hidden="true"
  >
    <!-- 两个交叉箭头表示"同步" -->
    <circle cx="16" cy="16" r="14" stroke="rgba(255,255,255,0.6)" stroke-width="1.5"/>
    <path d="M10 12h8l-2-2m0 0l-2 2m2-2v8" stroke="#22c55e" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>
    <path d="M22 20h-8l2 2m0 0l2-2m-2 2v-8" stroke="#3b82f6" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>
  </svg>
</div>

<style>
  .ball {
    width: 100vw;
    height: 100vh;
    border-radius: 50%;
    background: rgba(28, 28, 32, 0.88);
    -webkit-backdrop-filter: blur(20px);
    backdrop-filter: blur(20px);
    border: 1px solid rgba(255, 255, 255, 0.08);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: grab;
    user-select: none;
    box-sizing: border-box;
    overflow: hidden;
  }

  .ball:active {
    cursor: grabbing;
  }
</style>
