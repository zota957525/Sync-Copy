<script lang="ts">
  /**
   * FloatingWindow — 主浮窗容器
   *
   * 布局（spec floating-window.md 第 6.3 节 wireframe）：
   *   顶部状态栏 36px（drag-region + StatusDot + 状态文字 + 按钮组：加入 / − / ⚙）
   *   中间内容区（历史列表 HistoryList / settings）
   *   底部 footer 24px（IP:PORT + 设备名）
   *   brand line 16px
   *
   * PR-FE-2 新增：
   *   - ⚙ 按钮切换到 settings view（SettingsPanel 组件）
   *   - main view 中渲染 ApprovalDialog 覆盖层
   *
   * PR-FE-3 新增：
   *   - 历史列表区域改为渲染 HistoryList 组件
   *   - 顶部 − 按钮触发 collapseToBall()（window setSize 48×48）
   *   - ball view 渲染 FloatingBall；单击 FloatingBall 展开回 window
   *
   * 视觉（第 6.5 节字典）：
   *   背景 rgba(28,28,32,0.88) + backdrop-filter:blur(20px) + 圆角 10px + 1px 微高亮边
   *
   * 拖拽：data-tauri-drag-region（capabilities/default.json 已含 core:window:allow-start-dragging）
   */
  import { getCurrentWindow, LogicalSize, currentMonitor, PhysicalPosition } from "@tauri-apps/api/window";
  import StatusDot from "./StatusDot.svelte";
  import SettingsPanel from "./SettingsPanel.svelte";
  import ApprovalDialog from "./ApprovalDialog.svelte";
  import HistoryList from "./HistoryList.svelte";
  import FloatingBall from "./FloatingBall.svelte";
  import {
    COLOR_WINDOW_BG,
    COLOR_WINDOW_BORDER,
    COLOR_TEXT_PRIMARY,
    COLOR_TEXT_SECONDARY,
    COLOR_TEXT_BRAND,
    COLOR_TEXT_SUCCESS,
    COLOR_BTN_PRIMARY_BG,
    COLOR_DIVIDER,
    FONT_FAMILY,
    FONT_SIZE_DEFAULT,
    FONT_SIZE_SECONDARY,
    FONT_SIZE_BRAND,
  } from "$lib/style/tokens";
  import { statusStore } from "$lib/stores/status.svelte";
  import { approvalStore } from "$lib/stores/approval.svelte";
  import { historyStore } from "$lib/stores/history.svelte";

  // ---------------------------------------------------------------------------
  // View 状态：main | settings | ball
  // ---------------------------------------------------------------------------

  type ViewState = "main" | "settings" | "ball";
  let currentView = $state<ViewState>("main");

  function openSettings(): void {
    currentView = "settings";
  }

  function closeSettings(): void {
    currentView = "main";
  }

  // ---------------------------------------------------------------------------
  // 折叠为球 / 展开（floating-ball.md 第 3 节）
  // ---------------------------------------------------------------------------

  const DEFAULT_EXPANDED = { w: 320, h: 420 };
  // 记住展开尺寸（不需要 rune，跨函数共享即可）
  let lastExpandedSize = { ...DEFAULT_EXPANDED };

  async function collapseToBall(): Promise<void> {
    try {
      const win = getCurrentWindow();
      const physical = await win.outerSize();
      const scale    = await win.scaleFactor();
      lastExpandedSize = {
        w: Math.round(physical.width  / scale),
        h: Math.round(physical.height / scale),
      };
      currentView = "ball";
      await win.setSize(new LogicalSize(48, 48));
    } catch (e) {
      // non-fatal：记录但不阻断
      console.warn("collapseToBall failed:", e);
    }
  }

  async function expandFromBall(): Promise<void> {
    try {
      const { w, h } = lastExpandedSize.w > 40 ? lastExpandedSize : DEFAULT_EXPANDED;
      currentView = "main";
      const win = getCurrentWindow();
      await win.setSize(new LogicalSize(w, h));
      // 视口校正：确保窗口在监视器内完全可见
      const monitor = await currentMonitor();
      if (monitor) {
        const pos   = await win.outerPosition();
        const scale = await win.scaleFactor();
        const monX = monitor.position.x;
        const monY = monitor.position.y;
        const monW = monitor.size.width;
        const monH = monitor.size.height;
        const winW = w * scale;
        const winH = h * scale;
        const maxX = monX + monW - winW;
        const maxY = monY + monH - winH;
        const newX = Math.max(monX, Math.min(pos.x, maxX));
        const newY = Math.max(monY, Math.min(pos.y, maxY));
        if (newX !== pos.x || newY !== pos.y) {
          await win.setPosition(new PhysicalPosition(newX, newY));
        }
      }
    } catch (e) {
      console.warn("expandFromBall failed:", e);
    }
  }

  // ---------------------------------------------------------------------------
  // 状态文字 + 圆点状态 派生
  // ---------------------------------------------------------------------------

  type DotState = "connected" | "disconnected" | "error" | "warning";

  let dotState = $derived<DotState>(
    statusStore.approvedCount > 0 ? "connected" : "disconnected"
  );

  let statusText = $derived(
    statusStore.approvedCount > 0
      ? `小组 · ${statusStore.approvedCount} 台`
      : "未连接 · 0 台"
  );

  // ---------------------------------------------------------------------------
  // IP:PORT 复制反馈
  // ---------------------------------------------------------------------------

  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  async function copyAddr(): Promise<void> {
    try {
      await navigator.clipboard.writeText(statusStore.listenAddr);
      copied = true;
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => {
        copied = false;
      }, 1200);
    } catch {
      // 剪切板写失败忽略，不展示错误
    }
  }

  // ---------------------------------------------------------------------------
  // historyCount → SettingsPanel disabled 判断
  // ---------------------------------------------------------------------------

  let historyCount = $derived(historyStore.items.length);

  // ---------------------------------------------------------------------------
  // 是否展示审批覆盖层
  // ---------------------------------------------------------------------------

  let showApproval = $derived(
    currentView === "main" && approvalStore.queue.length > 0
  );
</script>

<div
  class="window"
  style:font-family={FONT_FAMILY}
  style:background={COLOR_WINDOW_BG}
  style:border-color={COLOR_WINDOW_BORDER}
>

  {#if currentView === "ball"}
    <!-- ---- Ball View ---- -->
    <FloatingBall onexpand={expandFromBall} />

  {:else if currentView === "settings"}
    <!-- ---- Settings View ---- -->
    <SettingsPanel onclose={closeSettings} {historyCount} />

  {:else}
    <!-- ---- Main View ---- -->

    <!-- 顶部状态栏：36px，整行 drag-region，按钮独立 pointer-events -->
    <div class="statusbar" data-tauri-drag-region>
      <div class="statusbar-left" data-tauri-drag-region>
        <StatusDot state={dotState} size={8} />
        <span class="status-text" style:color={COLOR_TEXT_SECONDARY} style:font-size={FONT_SIZE_SECONDARY}>
          {statusText}
        </span>
      </div>
      <div class="statusbar-right">
        <!-- [加入] 胶囊按钮（PR-FE-3 接入 JoinDialog）-->
        <button
          class="btn-join"
          style:background={COLOR_BTN_PRIMARY_BG}
          style:color={COLOR_TEXT_PRIMARY}
          style:font-size={FONT_SIZE_SECONDARY}
          aria-label="加入小组"
        >
          加入
        </button>
        <!-- − 折叠为球按钮（floating-ball spec 第 3 节）-->
        <button
          class="btn-icon"
          style:color={COLOR_TEXT_SECONDARY}
          onclick={collapseToBall}
          aria-label="折叠为球"
        >
          −
        </button>
        <!-- ⚙ 设置按钮 -->
        <button
          class="btn-icon"
          style:color={COLOR_TEXT_SECONDARY}
          onclick={openSettings}
          aria-label="设置"
        >
          ⚙
        </button>
      </div>
    </div>

    <!-- 分割线 -->
    <div class="divider" style:background={COLOR_DIVIDER}></div>

    <!-- 历史列表区域（相对定位，ApprovalDialog 绝对叠加） -->
    <div class="history-area">
      <HistoryList />

      <!-- 审批弹框覆盖层（仅 main view + 队列非空时显示） -->
      {#if showApproval}
        <ApprovalDialog />
      {/if}
    </div>

    <!-- 分割线 -->
    <div class="divider" style:background={COLOR_DIVIDER}></div>

    <!-- 底部 footer：24px，IP:PORT + 设备名 -->
    <div class="footer">
      <button
        class="addr-btn"
        onclick={copyAddr}
        aria-label="复制地址"
        style:color={copied ? COLOR_TEXT_SUCCESS : COLOR_TEXT_SECONDARY}
        style:font-size={FONT_SIZE_SECONDARY}
      >
        {copied ? "已复制" : statusStore.listenAddr}
      </button>
    </div>

    <!-- brand line -->
    <div class="brand" style:color={COLOR_TEXT_BRAND} style:font-size={FONT_SIZE_BRAND}>
      Made with Claude · by Tao
    </div>
  {/if}

</div>

<style>
  .window {
    width: 100vw;
    height: 100vh;
    display: flex;
    flex-direction: column;
    border-radius: 10px;
    border: 1px solid;
    overflow: hidden;
    box-sizing: border-box;
    /* backdrop-filter: Win 若不支持则退化为半透明纯色（第 6.5 节 Win fallback） */
    -webkit-backdrop-filter: blur(20px);
    backdrop-filter: blur(20px);
    color: #f3f4f6;
    user-select: none;
    /* 子组件 ApprovalDialog 需要 position:relative 作为定位上下文 */
    position: relative;
  }

  /* ---- 顶部状态栏 ---- */
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

  /* ---- 分割线 ---- */
  .divider {
    height: 1px;
    flex-shrink: 0;
  }

  /* ---- 历史区域 ---- */
  .history-area {
    flex: 1;
    overflow: hidden;
    position: relative;
  }

  /* ---- 底部 footer ---- */
  .footer {
    height: 24px;
    display: flex;
    align-items: center;
    padding: 0 10px;
    flex-shrink: 0;
  }

  .addr-btn {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    font-family: inherit;
    transition: color 100ms ease;
    text-decoration: none;
  }

  .addr-btn:hover {
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  /* ---- brand line ---- */
  .brand {
    height: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 4px 0;
    flex-shrink: 0;
    letter-spacing: 0.2px;
  }
</style>
