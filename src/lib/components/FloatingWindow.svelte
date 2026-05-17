<script lang="ts">
  /**
   * FloatingWindow — 主浮窗容器
   *
   * 布局（spec floating-window.md 第 6.3 节 wireframe）：
   *   顶部状态栏 36px（FloatingHeader 组件，含 drag-region + StatusDot + 按钮组）
   *   中间内容区（历史列表 HistoryList / settings）
   *   底部 footer 24px（IP:PORT + 设备名）
   *   brand line 16px
   *
   * PR-FE-2 新增：⚙ 按钮切换到 settings view + ApprovalDialog 覆盖层
   * PR-FE-3 新增：历史列表区域渲染 HistoryList；顶部 − 按钮折叠为 FloatingBall
   * PR-FE-3a 重构：collapseToBall/expandFromBall 提取到 useBallCollapse hook；
   *             顶部状态栏提取到 FloatingHeader 组件
   *
   * 视觉（第 6.5 节字典）：
   *   背景 rgba(28,28,32,0.88) + backdrop-filter:blur(20px) + 圆角 10px + 1px 微高亮边
   *
   * 拖拽：data-tauri-drag-region（capabilities/default.json 已含 core:window:allow-start-dragging）
   */
  import FloatingHeader from "./FloatingHeader.svelte";
  import SettingsPanel from "./SettingsPanel.svelte";
  import ApprovalDialog from "./ApprovalDialog.svelte";
  import HistoryList from "./HistoryList.svelte";
  import FloatingBall from "./FloatingBall.svelte";
  import JoinDialog from "./JoinDialog.svelte";
  import {
    COLOR_WINDOW_BG,
    COLOR_WINDOW_BORDER,
    COLOR_TEXT_SECONDARY,
    COLOR_TEXT_BRAND,
    COLOR_TEXT_SUCCESS,
    FONT_FAMILY,
    FONT_SIZE_SECONDARY,
    FONT_SIZE_BRAND,
  } from "$lib/style/tokens";
  import { statusStore } from "$lib/stores/status.svelte";
  import { approvalStore } from "$lib/stores/approval.svelte";
  import { historyStore } from "$lib/stores/history.svelte";
  import { createBallCollapse } from "$lib/hooks/useBallCollapse.svelte";

  // ---------------------------------------------------------------------------
  // View 状态：main | settings | ball | join
  // ---------------------------------------------------------------------------

  type ViewState = "main" | "settings" | "ball" | "join";
  let currentView = $state<ViewState>("main");

  function openSettings(): void {
    currentView = "settings";
  }

  function closeSettings(): void {
    currentView = "main";
  }

  function openJoin(): void {
    currentView = "join";
  }

  function closeJoin(): void {
    currentView = "main";
  }

  // ---------------------------------------------------------------------------
  // 折叠为球 / 展开（floating-ball.md 第 3 节，逻辑委托 useBallCollapse hook）
  // ---------------------------------------------------------------------------

  const { collapseToBall: _collapse, expandFromBall: _expand } = createBallCollapse();

  async function collapseToBall(): Promise<void> {
    await _collapse();
    currentView = "ball";
  }

  async function expandFromBall(): Promise<void> {
    currentView = "main";
    await _expand();
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

  {:else if currentView === "join"}
    <!-- ---- Join View ---- -->
    <JoinDialog oncancel={closeJoin} />

  {:else}
    <!-- ---- Main View ---- -->

    <!-- 顶部状态栏（FloatingHeader 组件） -->
    <FloatingHeader
      {dotState}
      {statusText}
      oncollapse={collapseToBall}
      onsettings={openSettings}
      onjoin={openJoin}
    />

    <!-- 分割线 -->
    <div class="divider" style:background="rgba(255,255,255,0.07)"></div>

    <!-- 历史列表区域（相对定位，ApprovalDialog 绝对叠加） -->
    <div class="history-area">
      <HistoryList />

      <!-- 审批弹框覆盖层（仅 main view + 队列非空时显示） -->
      {#if showApproval}
        <ApprovalDialog />
      {/if}
    </div>

    <!-- 分割线 -->
    <div class="divider" style:background="rgba(255,255,255,0.07)"></div>

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
