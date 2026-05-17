<script lang="ts">
  /**
   * FloatingHeader — 浮窗顶部状态栏（36px）
   *
   * 布局（floating-window.md 第 6.3 节 wireframe）：
   *   整行 drag-region；左：StatusDot + 状态文字；右：[加入] 胶囊 + − + ⚙
   *
   * Props：
   *   dotState    — StatusDot 的 state
   *   statusText  — 状态文字（如"小组 · 2 台"）
   *   oncollapse  — 点击 − 触发折叠回调
   *   onsettings  — 点击 ⚙ 触发设置回调
   */
  import StatusDot from "./StatusDot.svelte";
  import {
    COLOR_BTN_PRIMARY_BG,
    COLOR_TEXT_PRIMARY,
    COLOR_TEXT_SECONDARY,
    FONT_SIZE_SECONDARY,
  } from "$lib/style/tokens";

  interface Props {
    dotState: "connected" | "disconnected" | "error" | "warning";
    statusText: string;
    oncollapse: () => void;
    onsettings: () => void;
    onjoin: () => void;
  }

  let { dotState, statusText, oncollapse, onsettings, onjoin }: Props = $props();
</script>

<div class="statusbar" data-tauri-drag-region>
  <div class="statusbar-left" data-tauri-drag-region>
    <StatusDot state={dotState} size={8} />
    <span
      class="status-text"
      style:color={COLOR_TEXT_SECONDARY}
      style:font-size={FONT_SIZE_SECONDARY}
    >
      {statusText}
    </span>
  </div>
  <div class="statusbar-right">
    <!-- [加入] 胶囊按钮（floating-window spec 第 6.3 节 → JoinDialog view）-->
    <button
      class="btn-join"
      style:background={COLOR_BTN_PRIMARY_BG}
      style:color={COLOR_TEXT_PRIMARY}
      style:font-size={FONT_SIZE_SECONDARY}
      onclick={onjoin}
      aria-label="加入小组"
    >
      加入
    </button>
    <!-- − 折叠为球按钮（floating-ball spec 第 3 节）-->
    <button
      class="btn-icon"
      style:color={COLOR_TEXT_SECONDARY}
      onclick={oncollapse}
      aria-label="折叠为球"
    >
      −
    </button>
    <!-- ⚙ 设置按钮 -->
    <button
      class="btn-icon"
      style:color={COLOR_TEXT_SECONDARY}
      onclick={onsettings}
      aria-label="设置"
    >
      ⚙
    </button>
  </div>
</div>

<style>
  .statusbar {
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 10px;
    flex-shrink: 0;
    cursor: grab;
  }

  .statusbar:active {
    cursor: grabbing;
  }

  .statusbar-left {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }

  .status-text {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .statusbar-right {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
    pointer-events: auto;
  }

  /* [加入] 胶囊按钮 */
  .btn-join {
    padding: 2px 8px;
    border-radius: 10px;
    border: none;
    cursor: pointer;
    font-weight: 500;
    transition: filter 80ms ease;
  }

  .btn-join:hover {
    filter: brightness(1.1);
  }

  .btn-join:active {
    transform: scale(0.95);
  }

  /* − / ⚙ 图标按钮 */
  .btn-icon {
    width: 22px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    border: none;
    background: transparent;
    cursor: pointer;
    font-size: 13px;
    transition: background 80ms ease;
  }

  .btn-icon:hover {
    background: rgba(255, 255, 255, 0.12);
  }
</style>
