<script lang="ts">
  /**
   * JoinDialog — 加入小组对话框（floating-window spec 第 6.3 节 btn-join 触发）
   *
   * 功能：
   *   - 输入目标地址 IP:PORT（如 192.168.1.51:5858）
   *   - 提交触发 joinGroup IPC，loading 期间禁止重复提交
   *   - 成功后展示"已发送，等待对端审批"中间态提示
   *   - 错误时展示用户友好文字（invalid_input / forbidden / internal_error）
   *   - 取消返回 main view
   *
   * Props：
   *   oncancel — 取消 / 关闭，返回 main view
   *
   * 视觉：复用 tokens.ts（floating-window.md 第 6.5 节字典）
   */
  import { joinGroup } from "$lib/ipc";
  import { IpcError } from "$lib/types";
  import {
    COLOR_BTN_PRIMARY_BG,
    COLOR_BTN_GHOST_BG,
    COLOR_BTN_GHOST_TEXT,
    COLOR_BTN_DISABLED_BG,
    COLOR_BTN_DISABLED_TEXT,
    COLOR_TEXT_PRIMARY,
    COLOR_TEXT_SECONDARY,
    COLOR_TEXT_DANGER,
    COLOR_TEXT_SUCCESS,
    FONT_SIZE_DEFAULT,
    FONT_SIZE_SECONDARY,
    FONT_FAMILY,
  } from "$lib/style/tokens";

  interface Props {
    oncancel: () => void;
  }

  let { oncancel }: Props = $props();

  // ---------------------------------------------------------------------------
  // 表单状态
  // ---------------------------------------------------------------------------

  let targetAddr = $state("");
  let loading = $state(false);
  let sent = $state(false); // 成功中间态
  let errorMsg = $state("");

  // 地址校验：粗略格式 ip:port（不做 DNS lookup，backend 有更精确的 parse）
  let addrTrimmed = $derived(targetAddr.trim());
  let isValid = $derived(addrTrimmed.length > 0);

  // ---------------------------------------------------------------------------
  // 用户友好错误文字映射
  // ---------------------------------------------------------------------------

  function errorText(code: string): string {
    switch (code) {
      case "invalid_input": return "地址格式无效，请输入 IP:端口（如 192.168.1.10:5858）";
      case "forbidden":     return "对端拒绝连接，请确认目标设备已开启 Sync Copy";
      case "internal_error": return "连接失败，请稍后重试";
      default:              return "未知错误，请重试";
    }
  }

  // ---------------------------------------------------------------------------
  // 提交
  // ---------------------------------------------------------------------------

  async function handleSubmit(): Promise<void> {
    if (loading || sent || !isValid) return;
    errorMsg = "";
    loading = true;
    try {
      await joinGroup(addrTrimmed);
      sent = true;
    } catch (e) {
      const code = e instanceof IpcError ? e.code : "unknown";
      errorMsg = errorText(code);
    } finally {
      loading = false;
    }
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === "Enter") handleSubmit();
    if (e.key === "Escape") oncancel();
  }
</script>

<div class="join-panel" style:font-family={FONT_FAMILY}>

  <!-- 顶部标题栏（data-tauri-drag-region 保持拖拽） -->
  <div class="header" data-tauri-drag-region>
    <span
      class="title"
      style:color={COLOR_TEXT_PRIMARY}
      style:font-size={FONT_SIZE_DEFAULT}
      data-tauri-drag-region
    >
      加入小组
    </span>
    <button
      class="btn-x"
      style:color={COLOR_TEXT_SECONDARY}
      onclick={oncancel}
      aria-label="取消"
    >
      ×
    </button>
  </div>

  <div class="divider"></div>

  <!-- 内容区 -->
  <div class="body">

    {#if sent}
      <!-- 成功中间态 -->
      <div class="sent-tip" style:color={COLOR_TEXT_SUCCESS}>
        已发送加入请求，等待对端审批...
      </div>
      <p class="hint" style:color={COLOR_TEXT_SECONDARY} style:font-size={FONT_SIZE_SECONDARY}>
        对端审批通过后状态点将变绿。
      </p>
      <button
        class="btn-primary"
        style:background={COLOR_BTN_PRIMARY_BG}
        style:color={COLOR_TEXT_PRIMARY}
        onclick={oncancel}
      >
        完成
      </button>

    {:else}
      <!-- 地址输入态 -->
      <label
        class="label"
        for="join-addr-input"
        style:color={COLOR_TEXT_SECONDARY}
        style:font-size={FONT_SIZE_SECONDARY}
      >
        目标地址（IP:端口）
      </label>

      <input
        id="join-addr-input"
        class="addr-input"
        type="text"
        placeholder="192.168.1.10:5858"
        maxlength="64"
        disabled={loading}
        bind:value={targetAddr}
        onkeydown={handleKeydown}
        autocomplete="off"
        spellcheck={false}
      />

      {#if errorMsg}
        <span class="error-msg" style:color={COLOR_TEXT_DANGER} style:font-size={FONT_SIZE_SECONDARY}>
          {errorMsg}
        </span>
      {/if}

      <p class="hint" style:color={COLOR_TEXT_SECONDARY} style:font-size={FONT_SIZE_SECONDARY}>
        对端收到请求后需手动审批。
      </p>

      <!-- 操作行 -->
      <div class="btn-row">
        <button
          class="btn-ghost"
          style:background={COLOR_BTN_GHOST_BG}
          style:color={COLOR_BTN_GHOST_TEXT}
          onclick={oncancel}
          disabled={loading}
        >
          取消
        </button>

        <button
          class="btn-primary"
          style:background={loading || !isValid ? COLOR_BTN_DISABLED_BG : COLOR_BTN_PRIMARY_BG}
          style:color={loading || !isValid ? COLOR_BTN_DISABLED_TEXT : COLOR_TEXT_PRIMARY}
          onclick={handleSubmit}
          disabled={loading || !isValid}
          aria-busy={loading}
        >
          {#if loading}
            <span class="spinner" aria-hidden="true"></span>
            发送中...
          {:else}
            发送请求
          {/if}
        </button>
      </div>
    {/if}

  </div>
</div>

<style>
  .join-panel {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    box-sizing: border-box;
  }

  /* 顶部标题栏 */
  .header {
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 10px;
    flex-shrink: 0;
    cursor: grab;
  }

  .header:active { cursor: grabbing; }

  .title {
    font-weight: 500;
  }

  .btn-x {
    width: 22px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    border: none;
    background: transparent;
    cursor: pointer;
    font-size: 15px;
    transition: background 80ms ease;
    pointer-events: auto;
  }

  .btn-x:hover { background: rgba(255,255,255,0.12); }

  /* 分割线 */
  .divider {
    height: 1px;
    background: rgba(255,255,255,0.07);
    flex-shrink: 0;
  }

  /* 内容区 */
  .body {
    flex: 1;
    padding: 12px 10px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow-y: auto;
  }

  .label {
    display: block;
  }

  /* 地址输入框 */
  .addr-input {
    width: 100%;
    box-sizing: border-box;
    background: rgba(255,255,255,0.06);
    border: 1px solid rgba(255,255,255,0.20);
    border-radius: 6px;
    padding: 6px 8px;
    font-size: 13px;
    color: #f3f4f6;
    font-family: inherit;
    outline: none;
    transition: border-color 100ms ease;
  }

  .addr-input:focus { border-color: rgba(59,130,246,0.60); }
  .addr-input:disabled { opacity: 0.5; }
  .addr-input::placeholder { color: rgba(156,163,175,0.6); }

  /* 错误提示 */
  .error-msg {
    margin-top: -2px;
  }

  /* 提示文字 */
  .hint {
    margin: 0;
    line-height: 1.4;
  }

  /* 成功提示 */
  .sent-tip {
    font-size: 13px;
    font-weight: 500;
    margin-bottom: 2px;
  }

  /* 操作按钮行 */
  .btn-row {
    display: flex;
    gap: 6px;
    margin-top: 4px;
  }

  .btn-ghost {
    flex: 0 0 auto;
    padding: 6px 12px;
    border-radius: 6px;
    border: none;
    font-size: 13px;
    cursor: pointer;
    font-family: inherit;
    transition: filter 80ms ease;
  }

  .btn-ghost:hover:not(:disabled) { filter: brightness(1.15); }
  .btn-ghost:disabled { opacity: 0.5; cursor: not-allowed; }

  .btn-primary {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 6px 12px;
    border-radius: 6px;
    border: none;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    font-family: inherit;
    transition: filter 80ms ease;
  }

  .btn-primary:hover:not(:disabled) { filter: brightness(1.1); }
  .btn-primary:disabled { cursor: not-allowed; }

  /* 加载中小圆圈 spinner */
  .spinner {
    display: inline-block;
    width: 10px;
    height: 10px;
    border: 1.5px solid rgba(255,255,255,0.35);
    border-top-color: #fff;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
    flex-shrink: 0;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
