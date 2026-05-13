<script lang="ts">
  /**
   * SettingsPanel — 设置面板 view（spec settings-panel 第 6 节 wireframe）
   *
   * 浮窗次级 view；view 切换，不是新窗口。
   * 内容：设备名 input（失焦自动保存）+ 清除历史 + 退出应用 + 版本号。
   */
  import { getConfig, setConfig, clearHistory, quitApp } from "$lib/ipc";
  import { IpcError } from "$lib/types";
  import ClearConfirm from "./ClearConfirm.svelte";
  import pkg from "../../../package.json";

  interface Props {
    onclose: () => void;
    historyCount: number;
  }

  let { onclose, historyCount }: Props = $props();

  // 版本号
  const appVersion: string = pkg.version ?? "";

  // 设备名
  let deviceName = $state("");
  let originalName = $state("");
  let nameError = $state("");
  let loading = $state(true);

  $effect(() => { loadConfig(); });

  async function loadConfig(): Promise<void> {
    loading = true;
    try {
      const cfg = await getConfig();
      deviceName = cfg.device_name;
      originalName = cfg.device_name;
    } catch { /* 拉取失败：保持空值 */ }
    finally { loading = false; }
  }

  async function handleBlur(): Promise<void> {
    const trimmed = deviceName.trim();
    if (!trimmed) {
      nameError = "设备名不能为空";
      deviceName = originalName;
      return;
    }
    nameError = "";
    if (trimmed === originalName) return;
    try {
      await setConfig({ device_name: trimmed });
      originalName = trimmed;
      deviceName = trimmed;
    } catch (e) {
      nameError = e instanceof IpcError && e.code === "invalid_input"
        ? "设备名包含非法字符"
        : "保存失败，请重试";
      deviceName = originalName;
    }
  }

  function handleClose(): void {
    deviceName = originalName;
    nameError = "";
    onclose();
  }

  // 清除历史
  let showClearConfirm = $state(false);
  let clearingHistory = $state(false);

  async function handleClearConfirm(): Promise<void> {
    if (clearingHistory) return;
    clearingHistory = true;
    try { await clearHistory(); } catch { /* 忽略 */ }
    finally {
      clearingHistory = false;
      showClearConfirm = false;
    }
    onclose();
  }

  // 退出
  async function handleQuit(): Promise<void> {
    try { await quitApp(); } catch { /* 进程退出，catch 为防 runtime 错误 */ }
  }
</script>

<div class="panel">
  <!-- 顶部标题栏 -->
  <div class="header" data-tauri-drag-region>
    <span data-tauri-drag-region>⚙ 设置</span>
    <button class="btn-x" onclick={handleClose} aria-label="关闭设置">×</button>
  </div>
  <div class="divider"></div>

  <!-- 内容区 -->
  <div class="body">
    <label class="label" for="device-name-input">本机设备名</label>
    <input
      id="device-name-input"
      class="name-input"
      type="text"
      maxlength="64"
      placeholder="输入设备名"
      disabled={loading}
      bind:value={deviceName}
      onblur={handleBlur}
    />
    {#if nameError}
      <span class="name-error">{nameError}</span>
    {/if}

    <div class="section-divider"></div>

    {#if !showClearConfirm}
      <button
        class="btn-clear"
        class:is-disabled={historyCount === 0}
        onclick={() => { if (historyCount > 0) showClearConfirm = true; }}
        disabled={historyCount === 0}
      >
        清除历史
      </button>
    {:else}
      <ClearConfirm
        oncancel={() => { showClearConfirm = false; }}
        onconfirm={handleClearConfirm}
        disabled={clearingHistory}
      />
    {/if}

    <div class="section-divider"></div>
    <button class="btn-quit" onclick={handleQuit}>退出应用</button>
  </div>

  {#if appVersion}
    <div class="version">v{appVersion}</div>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    color: #f3f4f6;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    font-size: 13px;
  }
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
  .btn-x {
    width: 22px; height: 22px;
    display: flex; align-items: center; justify-content: center;
    border-radius: 50%; border: none; background: transparent;
    cursor: pointer; font-size: 15px; color: #9ca3af;
    transition: background 80ms;
    pointer-events: auto;
  }
  .btn-x:hover { background: rgba(255,255,255,0.12); }
  .divider { height: 1px; background: rgba(255,255,255,0.07); flex-shrink: 0; }
  .body {
    flex: 1; padding: 12px 10px 8px;
    display: flex; flex-direction: column; gap: 6px; overflow-y: auto;
  }
  .label { font-size: 12px; color: #9ca3af; }
  .name-input {
    width: 100%; box-sizing: border-box;
    background: rgba(255,255,255,0.06);
    border: 1px solid rgba(255,255,255,0.20);
    border-radius: 6px; padding: 6px 8px;
    font-size: 13px; color: #f3f4f6; font-family: inherit;
    outline: none; transition: border-color 100ms;
  }
  .name-input:focus { border-color: rgba(59,130,246,0.60); }
  .name-input:disabled { opacity: 0.5; }
  .name-error { font-size: 11px; color: #ef4444; margin-top: -2px; }
  .section-divider { height: 1px; background: rgba(255,255,255,0.08); margin: 2px 0; }
  .btn-clear {
    background: rgba(255,255,255,0.12); border: none; border-radius: 6px;
    padding: 6px 10px; font-size: 13px; color: #f3f4f6;
    cursor: pointer; font-family: inherit; text-align: left;
    transition: background 80ms;
  }
  .btn-clear:hover:not(:disabled) { background: rgba(255,255,255,0.18); }
  .btn-clear.is-disabled, .btn-clear:disabled { opacity: 0.4; cursor: not-allowed; }
  .btn-quit {
    width: 100%; background: #ef4444; border: none; border-radius: 6px;
    padding: 7px 0; font-size: 13px; color: #fff;
    cursor: pointer; font-family: inherit; font-weight: 500;
    transition: filter 80ms;
  }
  .btn-quit:hover { filter: brightness(1.1); }
  .version {
    height: 20px; display: flex; align-items: center; justify-content: center;
    font-size: 11px; color: #9ca3af; flex-shrink: 0; padding-bottom: 4px;
  }
</style>
