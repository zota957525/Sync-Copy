<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";

  type StatusKind =
    | { kind: "idle" }
    | { kind: "listening" }
    | { kind: "connecting" }
    | { kind: "connected"; peers: number }
    | { kind: "error"; message: string };

  type Source = { kind: "local" } | { kind: "remote"; device_name: string };

  type HistoryItem = {
    id: string;
    text: string;
    timestamp_ms: number;
    source: Source;
  };

  type ConfigView = {
    port: number;
    password: string;
    device_name: string;
    peer_hint: string | null;
    device_id: string;
  };

  let status = $state<StatusKind>({ kind: "idle" });
  let history = $state<HistoryItem[]>([]);
  let settingsOpen = $state(false);
  let showPassword = $state(false);
  let unlistenFns: UnlistenFn[] = [];

  function randomPassword(len = 8): string {
    // 去除易混淆字符 (0/O/o, 1/l/I)
    const pool =
      "abcdefghjkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    const arr = new Uint32Array(len);
    crypto.getRandomValues(arr);
    let out = "";
    for (let i = 0; i < len; i++) out += pool[arr[i] % pool.length];
    return out;
  }

  function generatePassword() {
    form.password = randomPassword(8);
    showPassword = true;
  }

  async function openSettings() {
    // 进入设置前从 backend 拉最新配置，避免显示上一次未保存的残留
    await loadConfig();
    showPassword = false;
    settingsOpen = true;
  }

  async function closeSettings() {
    // × 关闭 = 放弃未保存修改：从 backend 重新加载覆盖 form
    await loadConfig();
    showPassword = false;
    settingsOpen = false;
  }

  let joining = $state(false);

  async function join() {
    joining = true;
    try {
      await invoke("join_group");
    } catch (e) {
      alert("加入失败: " + e);
    } finally {
      joining = false;
      await refreshStatus();
    }
  }

  async function leave() {
    try {
      await invoke("leave_group");
    } catch (e) {
      console.warn("leave_group failed", e);
    }
    await refreshStatus();
  }

  let form = $state<ConfigView>({
    port: 5858,
    password: "",
    device_name: "",
    peer_hint: null,
    device_id: "",
  });

  async function refreshStatus() {
    try {
      status = (await invoke("get_status")) as StatusKind;
    } catch (e) {
      console.error("get_status failed", e);
    }
  }

  async function refreshHistory() {
    try {
      history = (await invoke("get_history")) as HistoryItem[];
    } catch (e) {
      console.error("get_history failed", e);
    }
  }

  async function loadConfig() {
    const cfg = (await invoke("get_config")) as ConfigView;
    form = { ...cfg };
  }

  async function saveConfig() {
    try {
      const update = {
        port: form.port,
        password: form.password,
        device_name: form.device_name,
        peer_hint:
          form.peer_hint && form.peer_hint.trim() ? form.peer_hint.trim() : null,
      };
      form = (await invoke("set_config", { update })) as ConfigView;
      settingsOpen = false;
    } catch (e) {
      alert("保存失败: " + e);
    }
  }

  async function deleteItem(id: string, ev: MouseEvent) {
    ev.stopPropagation();
    await invoke("delete_history_item", { id });
    await refreshHistory();
  }

  async function clearAll() {
    if (history.length === 0) return;
    await invoke("clear_history");
    await refreshHistory();
  }

  async function copyItem(text: string) {
    // 让用户点击历史项就能把该条重新写回剪切板
    try {
      await navigator.clipboard.writeText(text);
    } catch (e) {
      console.warn("clipboard.writeText failed", e);
    }
  }

  function onHeaderMouseDown(ev: MouseEvent) {
    // 保险兜底：原生 data-tauri-drag-region 失效时用 JS startDragging
    if (ev.button !== 0) return;
    const t = ev.target as HTMLElement;
    if (t.closest("button") || t.closest("input")) return;
    getCurrentWindow().startDragging().catch((e) =>
      console.warn("startDragging failed:", e)
    );
  }

  onMount(() => {
    refreshStatus();
    refreshHistory();
    loadConfig();
    listen("history-updated", () => refreshHistory()).then((fn) =>
      unlistenFns.push(fn)
    );
    listen("status-updated", () => refreshStatus()).then((fn) =>
      unlistenFns.push(fn)
    );
    return () => {
      unlistenFns.forEach((fn) => fn());
    };
  });

  const peerCount = $derived.by(() =>
    status.kind === "connected" ? status.peers : 0
  );

  const statusText = $derived.by(() => {
    switch (status.kind) {
      case "idle":
        return `未连接 · ${peerCount} 台`;
      case "listening":
        return `等待中 · ${peerCount} 台`;
      case "connecting":
        return `连接中 · ${peerCount} 台`;
      case "connected":
        return `已连接 · ${peerCount} 台`;
      case "error":
        return status.message || "错误";
    }
  });

  const statusColor = $derived.by(() => {
    switch (status.kind) {
      case "idle":
        return "#9ca3af";
      case "listening":
        return "#60a5fa";
      case "connecting":
        return "#fbbf24";
      case "connected":
        return "#22c55e";
      case "error":
        return "#ef4444";
    }
  });

  const canJoin = $derived.by(
    () => status.kind === "idle" || status.kind === "error"
  );
  const isOnline = $derived.by(
    () =>
      status.kind === "listening" ||
      status.kind === "connecting" ||
      status.kind === "connected"
  );

  function timeAgo(ms: number): string {
    const diff = Date.now() - ms;
    if (diff < 60_000) return "刚刚";
    if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前`;
    if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)} 小时前`;
    return `${Math.floor(diff / 86_400_000)} 天前`;
  }

  function sourceLabel(src: Source): string {
    return src.kind === "local" ? "本机" : src.device_name;
  }
</script>

<div class="window">
  <!-- 顶部栏：状态 + 设置；data-tauri-drag-region 是 Tauri 原生拖拽 hook，onmousedown 作为兜底 -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="header"
    data-tauri-drag-region
    onmousedown={onHeaderMouseDown}
    ondblclick={() => getCurrentWindow().setFocus()}
  >
    <span class="dot" data-tauri-drag-region style="background:{statusColor}"></span>
    <span class="status" data-tauri-drag-region>{statusText}</span>
    {#if !settingsOpen}
      {#if canJoin}
        <button class="pill" onclick={join} disabled={joining} title="启动服务端，握手 peer_hint">
          {joining ? "…" : "加入"}
        </button>
      {:else if isOnline}
        <button class="pill ghost-pill" onclick={leave} title="断开并停止服务">断开</button>
      {/if}
      <button class="icon-btn" onclick={openSettings} title="设置">⚙</button>
    {:else}
      <button class="icon-btn" onclick={closeSettings} title="放弃修改并返回"
        >×</button
      >
    {/if}
  </div>

  {#if !settingsOpen}
    <div class="history">
      {#if history.length === 0}
        <div class="empty">还没有同步过<br /><span class="hint">复制一段文本试试</span></div>
      {:else}
        {#each history as item (item.id)}
          <div
            role="button"
            tabindex="0"
            class="item"
            onclick={() => copyItem(item.text)}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ") copyItem(item.text);
            }}
            title="点击复制到剪切板"
          >
            <div class="item-text">{item.text}</div>
            <div class="item-meta">
              <span>{sourceLabel(item.source)}</span>
              <span class="dotsep">·</span>
              <span>{timeAgo(item.timestamp_ms)}</span>
            </div>
            <button
              class="del-btn"
              onclick={(e) => deleteItem(item.id, e)}
              title="删除此条"
              aria-label="删除">✕</button
            >
          </div>
        {/each}
      {/if}
    </div>
    <div class="footer">
      <button
        class="ghost"
        onclick={clearAll}
        disabled={history.length === 0}
        title="清除所有历史">清除</button
      >
      <span class="device">{form.device_name || "…"}</span>
    </div>
  {:else}
    <div class="settings">
      <label>
        <span>本机端口</span>
        <input type="number" min="1024" max="65535" bind:value={form.port} />
      </label>
      <label>
        <span>密码</span>
        <div class="pwd-row">
          <input
            class="pwd-input"
            type={showPassword ? "text" : "password"}
            bind:value={form.password}
            placeholder="小组共享密码"
          />
          <button
            type="button"
            class="mini-btn"
            onclick={() => (showPassword = !showPassword)}
            title={showPassword ? "隐藏密码" : "显示密码"}
            aria-label="toggle password visibility">{showPassword ? "🙈" : "👁"}</button
          >
          <button
            type="button"
            class="mini-btn"
            onclick={generatePassword}
            title="随机生成 8 位密码"
            aria-label="generate password">🎲</button
          >
        </div>
      </label>
      <label>
        <span>设备名</span>
        <input type="text" bind:value={form.device_name} />
      </label>
      <label>
        <span>加入目标</span>
        <input
          type="text"
          bind:value={form.peer_hint}
          placeholder="ip:port（可选，首次必填）"
        />
      </label>
      <button class="primary" onclick={saveConfig}>保存</button>
    </div>
  {/if}
</div>

<style>
  /* 保证所有子元素使用 border-box，避免 width:100% + padding 撑破容器 */
  :global(*, *::before, *::after) {
    box-sizing: border-box;
  }
  .window {
    width: 100vw;
    height: 100vh;
    padding: 8px 10px 8px 10px;
    background: rgba(28, 28, 32, 0.88);
    color: #f3f4f6;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    font-size: 13px;
    border-radius: 10px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    display: flex;
    flex-direction: column;
    gap: 6px;
    user-select: none;
    overflow: hidden;
  }
  .header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 2px 4px 2px;
    cursor: grab;
  }
  .header:active {
    cursor: grabbing;
  }
  .dot {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .status {
    flex: 1;
    font-weight: 500;
  }
  .icon-btn {
    background: transparent;
    color: #d1d5db;
    border: none;
    cursor: pointer;
    padding: 2px 6px;
    font-size: 14px;
    border-radius: 4px;
  }
  .icon-btn:hover {
    background: rgba(255, 255, 255, 0.08);
  }
  .pill {
    background: #2563eb;
    color: white;
    border: none;
    border-radius: 999px;
    padding: 2px 10px;
    font-size: 11px;
    cursor: pointer;
    line-height: 1.4;
  }
  .pill:hover:not(:disabled) {
    background: #1d4ed8;
  }
  .pill:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .ghost-pill {
    background: transparent;
    color: #d1d5db;
    border: 1px solid rgba(255, 255, 255, 0.2);
  }
  .ghost-pill:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
  }
  .history {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 2px;
  }
  .history::-webkit-scrollbar {
    width: 6px;
  }
  .history::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.1);
    border-radius: 3px;
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    color: #6b7280;
    font-size: 12px;
    line-height: 1.6;
    text-align: center;
  }
  .empty .hint {
    font-size: 11px;
    opacity: 0.7;
  }
  .item {
    display: block;
    position: relative;
    width: 100%;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.04);
    border-radius: 6px;
    padding: 6px 28px 6px 8px;
    color: inherit;
    text-align: left;
    font: inherit;
    cursor: pointer;
  }
  .item:hover {
    background: rgba(255, 255, 255, 0.08);
    border-color: rgba(255, 255, 255, 0.12);
  }
  .item-text {
    font-size: 12px;
    color: #e5e7eb;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .item-meta {
    margin-top: 2px;
    font-size: 10px;
    color: #9ca3af;
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .item-meta .dotsep {
    opacity: 0.6;
  }
  .del-btn {
    position: absolute;
    top: 50%;
    right: 6px;
    transform: translateY(-50%);
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: none;
    background: rgba(255, 255, 255, 0.08);
    color: #d1d5db;
    cursor: pointer;
    font-size: 11px;
    line-height: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0.55;
    transition: opacity 0.15s, background 0.15s, color 0.15s;
  }
  .item:hover .del-btn {
    opacity: 1;
  }
  .del-btn:hover {
    background: rgba(239, 68, 68, 0.3);
    color: #fecaca;
    opacity: 1;
  }
  .footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding: 2px 2px 0 2px;
  }
  .ghost {
    background: transparent;
    color: #d1d5db;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 4px;
    padding: 3px 10px;
    font-size: 11px;
    cursor: pointer;
  }
  .ghost:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
  }
  .ghost:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  .device {
    font-size: 11px;
    color: #9ca3af;
  }
  .settings {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 2px;
    overflow-y: auto;
  }
  .settings label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 11px;
    color: #9ca3af;
  }
  .settings input {
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    padding: 5px 7px;
    color: #f3f4f6;
    font-size: 12px;
    outline: none;
  }
  .settings input:focus {
    border-color: rgba(96, 165, 250, 0.5);
  }
  .pwd-row {
    display: flex;
    gap: 4px;
    align-items: stretch;
  }
  .pwd-input {
    flex: 1;
    min-width: 0;
  }
  .mini-btn {
    flex-shrink: 0;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    color: #e5e7eb;
    font-size: 13px;
    line-height: 1;
    padding: 0 8px;
    cursor: pointer;
  }
  .mini-btn:hover {
    background: rgba(255, 255, 255, 0.12);
  }
  .primary {
    margin-top: 4px;
    background: #2563eb;
    color: white;
    border: none;
    border-radius: 4px;
    padding: 6px 12px;
    font-size: 12px;
    cursor: pointer;
  }
  .primary:hover {
    background: #1d4ed8;
  }
</style>
