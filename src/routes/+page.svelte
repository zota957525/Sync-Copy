<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
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
    timestamp_ms: number;
    source: Source;
  } & (
    | { kind: "text"; text: string }
    | { kind: "image"; width: number; height: number; data_url: string }
  );

  type ConfigView = {
    port: number;
    device_name: string;
    peer_hint: string | null;
    device_id: string;
  };

  type View = "main" | "settings" | "join";

  let status = $state<StatusKind>({ kind: "idle" });
  let history = $state<HistoryItem[]>([]);
  let view = $state<View>("main");
  let localIp = $state<string | null>(null);
  let joining = $state(false);
  let joinTarget = $state("");
  let banner = $state<string | null>(null);
  let unlistenFns: UnlistenFn[] = [];

  type PendingApproval = {
    request_id: string;
    device_id: string;
    device_name: string;
  };
  let pendingApprovals = $state<PendingApproval[]>([]);
  let approvalCountdown = $state(30);

  type PendingFile = {
    request_id: string;
    filename: string;
    size: number;
    origin_device_name: string;
  };
  let pendingFiles = $state<PendingFile[]>([]);
  let dragOver = $state(false);
  let sendingFiles = $state(false);

  function formatSize(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / 1024 / 1024).toFixed(1)} MB`;
  }

  let form = $state<ConfigView>({
    port: 5858,
    device_name: "",
    peer_hint: null,
    device_id: "",
  });

  // ---- data loaders ----
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

  async function loadLocalIp() {
    try {
      localIp = (await invoke("get_local_ip")) as string | null;
    } catch (e) {
      console.warn("get_local_ip failed", e);
      localIp = null;
    }
  }

  // ---- settings view actions ----
  async function openSettings() {
    await loadConfig();
    await loadLocalIp();
    view = "settings";
  }

  async function closeSettings() {
    await loadConfig();
    view = "main";
  }

  async function saveConfig() {
    try {
      const update = {
        port: form.port,
        device_name: form.device_name,
      };
      form = (await invoke("set_config", { update })) as ConfigView;
      view = "main";
    } catch (e) {
      banner = "保存失败: " + e;
    }
  }

  async function quitApp() {
    try {
      await invoke("quit_app");
    } catch (e) {
      console.warn("quit_app", e);
    }
  }

  // ---- join view actions ----
  async function openJoin() {
    await loadConfig();
    // 用上次成功的 peer_hint 作为默认值
    joinTarget = form.peer_hint || "";
    view = "join";
  }

  function closeJoin() {
    view = "main";
  }

  async function submitJoin() {
    if (!joinTarget.trim()) {
      banner = "请输入对方机器地址";
      return;
    }
    banner = null;
    joining = true;
    try {
      await invoke("join_group", { target: joinTarget.trim() });
      view = "main";
    } catch (e) {
      banner = String(e);
    } finally {
      joining = false;
      await refreshStatus();
    }
  }

  async function goOnline() {
    banner = null;
    joining = true;
    try {
      // 空 target = 仅上线，等别人连我
      await invoke("join_group", { target: "" });
    } catch (e) {
      banner = String(e);
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

  async function respondApproval(request_id: string, accept: boolean) {
    try {
      await invoke("respond_handshake", { requestId: request_id, accept });
    } catch (e) {
      console.warn("respond_handshake failed", e);
    }
    pendingApprovals = pendingApprovals.filter((p) => p.request_id !== request_id);
  }

  async function respondFile(request_id: string, accept: boolean) {
    try {
      await invoke("respond_file_save", { requestId: request_id, accept });
    } catch (e) {
      console.warn("respond_file_save failed", e);
    }
    pendingFiles = pendingFiles.filter((p) => p.request_id !== request_id);
  }

  async function handleDroppedFiles(paths: string[]) {
    if (paths.length === 0) return;
    sendingFiles = true;
    try {
      const report = (await invoke("send_files", { paths })) as string;
      banner = report;
    } catch (e) {
      banner = "发送失败: " + e;
    } finally {
      sendingFiles = false;
    }
  }

  // ---- history actions ----
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

  async function copyItem(item: HistoryItem) {
    try {
      // 走后端：文本和图片都能正确写回系统剪切板并抑制回传
      await invoke("recopy_history_item", { id: item.id });
    } catch (e) {
      console.warn("recopy failed", e);
      banner = "复制失败: " + e;
    }
  }

  // ---- misc ----
  async function copyLocalAddr() {
    if (!localIp) return;
    try {
      await navigator.clipboard.writeText(`${localIp}:${form.port}`);
      banner = "本机地址已复制";
      setTimeout(() => {
        if (banner === "本机地址已复制") banner = null;
      }, 1500);
    } catch (e) {
      console.warn("copy failed", e);
    }
  }

  function onHeaderMouseDown(ev: MouseEvent) {
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
    loadLocalIp();
    listen("history-updated", () => refreshHistory()).then((fn) =>
      unlistenFns.push(fn)
    );
    listen("status-updated", () => refreshStatus()).then((fn) =>
      unlistenFns.push(fn)
    );
    listen<PendingApproval>("handshake-pending", (e) => {
      pendingApprovals = [...pendingApprovals, e.payload];
    }).then((fn) => unlistenFns.push(fn));
    listen<PendingFile>("file-pending", (e) => {
      pendingFiles = [...pendingFiles, e.payload];
    }).then((fn) => unlistenFns.push(fn));
    listen<{ path: string; filename: string }>("file-saved", (e) => {
      banner = `已保存 ${e.payload.filename}`;
      setTimeout(() => {
        if (banner && banner.startsWith("已保存")) banner = null;
      }, 2500);
    }).then((fn) => unlistenFns.push(fn));

    // 拖文件到窗口 → 发送
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over" || event.payload.type === "enter") {
          dragOver = true;
        } else if (event.payload.type === "leave") {
          dragOver = false;
        } else if (event.payload.type === "drop") {
          dragOver = false;
          handleDroppedFiles(event.payload.paths);
        }
      })
      .then((fn) => unlistenFns.push(fn));

    return () => {
      unlistenFns.forEach((fn) => fn());
    };
  });

  // 审批倒计时：每当弹框队列头变化时重新计时
  $effect(() => {
    const current = pendingApprovals[0];
    if (!current) return;
    approvalCountdown = 30;
    const id = setInterval(() => {
      approvalCountdown = Math.max(0, approvalCountdown - 1);
    }, 1000);
    return () => clearInterval(id);
  });

  // ---- derived ----
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
        return `出错 · ${peerCount} 台`;
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

  // 是否可以主动发起握手加入（连接过程中不行）
  const canJoin = $derived.by(() => status.kind !== "connecting");
  // 服务器是否在跑（listening/connecting/connected 都算）
  const isOnline = $derived.by(
    () =>
      status.kind === "listening" ||
      status.kind === "connecting" ||
      status.kind === "connected"
  );
  // 是否需要手动上线（服务器没起 + 不是正在连）
  const needGoOnline = $derived.by(
    () => status.kind === "idle" || status.kind === "error"
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
  <!-- 顶部栏 -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="header"
    data-tauri-drag-region
    onmousedown={onHeaderMouseDown}
    ondblclick={() => getCurrentWindow().setFocus()}
  >
    <span class="dot" data-tauri-drag-region style="background:{statusColor}"></span>
    <span class="status" data-tauri-drag-region>{statusText}</span>
    {#if view === "main"}
      {#if needGoOnline}
        <button
          class="pill ghost-pill"
          onclick={goOnline}
          disabled={joining}
          title="只启动本机服务端，等别人连我">上线</button
        >
      {/if}
      {#if canJoin}
        <button
          class="pill"
          onclick={openJoin}
          disabled={joining}
          title="主动连对方机器">加入</button
        >
      {/if}
      <button class="icon-btn" onclick={openSettings} title="本机设置">⚙</button>
    {:else if view === "settings"}
      <button class="icon-btn" onclick={closeSettings} title="放弃修改并返回">×</button>
    {:else if view === "join"}
      <button class="icon-btn" onclick={closeJoin} title="取消加入">×</button>
    {/if}
  </div>

  <!-- 全局横幅错误 / 提示 -->
  {#if banner}
    <div class="banner">
      <span>{banner}</span>
      <button class="banner-close" onclick={() => (banner = null)} aria-label="关闭"
        >✕</button
      >
    </div>
  {/if}

  {#if view === "main"}
    <div class="history">
      {#if history.length === 0}
        <div class="empty">
          还没有同步过<br /><span class="hint">复制一段文本试试</span>
        </div>
      {:else}
        {#each history as item (item.id)}
          <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
          <div
            role="button"
            tabindex="0"
            class="item"
            class:item-image={item.kind === "image"}
            onclick={() => copyItem(item)}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ") copyItem(item);
            }}
            title="点击复制到剪切板"
          >
            {#if item.kind === "text"}
              <div class="item-text">{item.text}</div>
            {:else}
              <div class="item-img-wrap">
                <img class="item-img" src={item.data_url} alt="image" />
                <span class="item-img-dim">{item.width}×{item.height}</span>
              </div>
            {/if}
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
  {:else if view === "settings"}
    <div class="panel scrollable">
      <div class="section-title">本机</div>
      <label>
        <span>设备名</span>
        <input type="text" bind:value={form.device_name} />
      </label>
      <label>
        <span>端口</span>
        <input type="number" min="1024" max="65535" bind:value={form.port} />
      </label>

      <div class="divider"></div>

      <div class="readonly-row">
        <div class="label-small">本机地址（告诉其他设备连我）</div>
        <div class="readonly-box">
          <span class="mono">
            {localIp ? `${localIp}:${form.port}` : "获取中…"}
          </span>
          <button
            type="button"
            class="mini-btn"
            onclick={copyLocalAddr}
            disabled={!localIp}
            title="复制本机地址">📋</button
          >
        </div>
      </div>

      <button class="primary" onclick={saveConfig}>保存</button>
      <button class="danger" onclick={quitApp}>退出应用</button>
    </div>
  {:else if view === "join"}
    <div class="panel">
      <div class="section-title">加入小组</div>
      <label>
        <span>对方机器地址</span>
        <input
          type="text"
          bind:value={joinTarget}
          placeholder="ip:port，例如 192.168.1.10:5858"
          autofocus
        />
      </label>
      <div class="hint-row">密码相同才能连上；密码在「⚙ 本机设置」里</div>
      <div class="btn-row">
        <button class="ghost" onclick={closeJoin} disabled={joining}>取消</button>
        <button class="primary" onclick={submitJoin} disabled={joining}>
          {joining ? "等待对方同意…" : "加入"}
        </button>
      </div>
    </div>
  {/if}

  <!-- 握手审批覆盖层 -->
  {#if pendingApprovals.length > 0}
    {@const p = pendingApprovals[0]}
    <div class="approval-overlay">
      <div class="approval-card">
        <div class="approval-icon">📥</div>
        <div class="approval-title">有设备希望加入</div>
        <div class="approval-device">{p.device_name}</div>
        <div class="approval-hint">
          还剩 <span class="countdown">{approvalCountdown}</span> 秒未响应视为拒绝
        </div>
        <div class="approval-actions">
          <button class="ghost" onclick={() => respondApproval(p.request_id, false)}
            >拒绝</button
          >
          <button class="primary" onclick={() => respondApproval(p.request_id, true)}
            >同意</button
          >
        </div>
        {#if pendingApprovals.length > 1}
          <div class="approval-queue">还有 {pendingApprovals.length - 1} 个待处理</div>
        {/if}
      </div>
    </div>
  {:else if pendingFiles.length > 0}
    <!-- 收到文件：另一种覆盖层 -->
    {@const f = pendingFiles[0]}
    <div class="approval-overlay">
      <div class="approval-card">
        <div class="approval-icon">📎</div>
        <div class="approval-title">收到来自 {f.origin_device_name} 的文件</div>
        <div class="approval-device">{f.filename}</div>
        <div class="approval-hint">
          {formatSize(f.size)} · 将保存到 Downloads
        </div>
        <div class="approval-actions">
          <button class="ghost" onclick={() => respondFile(f.request_id, false)}
            >拒绝</button
          >
          <button class="primary" onclick={() => respondFile(f.request_id, true)}
            >保存</button
          >
        </div>
        {#if pendingFiles.length > 1}
          <div class="approval-queue">还有 {pendingFiles.length - 1} 个文件待处理</div>
        {/if}
      </div>
    </div>
  {/if}

  <!-- 拖放视觉提示 -->
  {#if dragOver}
    <div class="drop-overlay">
      <div class="drop-hint">
        <div class="drop-icon">📤</div>
        <div>松开即发送给所有设备</div>
        <div class="drop-sub">单文件 ≤ 5 MB</div>
      </div>
    </div>
  {/if}
  {#if sendingFiles}
    <div class="drop-overlay dim">
      <div class="drop-hint">
        <div class="drop-icon">⏳</div>
        <div>发送中…</div>
      </div>
    </div>
  {/if}
</div>

<style>
  :global(*, *::before, *::after) {
    box-sizing: border-box;
  }
  .window {
    width: 100vw;
    height: 100vh;
    padding: 8px 10px;
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
    gap: 6px;
    padding: 4px 2px;
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
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
    flex-shrink: 0;
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

  /* 全局 banner */
  .banner {
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid rgba(239, 68, 68, 0.3);
    color: #fecaca;
    padding: 6px 8px;
    border-radius: 6px;
    font-size: 11px;
    line-height: 1.4;
    display: flex;
    align-items: flex-start;
    gap: 6px;
    word-break: break-all;
  }
  .banner span {
    flex: 1;
  }
  .banner-close {
    background: transparent;
    color: #fca5a5;
    border: none;
    cursor: pointer;
    padding: 0 4px;
    font-size: 11px;
    flex-shrink: 0;
  }
  .banner-close:hover {
    color: #fecaca;
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
  .item-img-wrap {
    position: relative;
    max-height: 80px;
    overflow: hidden;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.25);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .item-img {
    max-height: 80px;
    max-width: 100%;
    object-fit: contain;
    display: block;
  }
  .item-img-dim {
    position: absolute;
    right: 4px;
    bottom: 4px;
    background: rgba(0, 0, 0, 0.65);
    color: #e5e7eb;
    padding: 1px 6px;
    border-radius: 3px;
    font-size: 10px;
    font-variant-numeric: tabular-nums;
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
    padding: 2px 2px 0;
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

  /* settings/join 通用面板 */
  .panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 2px;
  }
  .panel.scrollable {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
  .section-title {
    font-size: 11px;
    color: #9ca3af;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .panel label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 11px;
    color: #9ca3af;
  }
  .panel input {
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    padding: 5px 7px;
    color: #f3f4f6;
    font-size: 12px;
    outline: none;
  }
  .panel input:focus {
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
  .mini-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.12);
  }
  .mini-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .divider {
    height: 1px;
    background: rgba(255, 255, 255, 0.08);
    margin: 4px 0;
  }
  .readonly-row {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .label-small {
    font-size: 11px;
    color: #9ca3af;
  }
  .readonly-box {
    display: flex;
    gap: 4px;
    align-items: center;
    background: rgba(255, 255, 255, 0.03);
    border: 1px dashed rgba(255, 255, 255, 0.08);
    border-radius: 4px;
    padding: 5px 7px;
  }
  .mono {
    flex: 1;
    font-family: "SF Mono", Menlo, Consolas, monospace;
    font-size: 12px;
    color: #e5e7eb;
    user-select: text;
  }
  .hint-row {
    font-size: 10px;
    color: #9ca3af;
    line-height: 1.4;
  }
  .btn-row {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
  .primary {
    background: #2563eb;
    color: white;
    border: none;
    border-radius: 4px;
    padding: 6px 14px;
    font-size: 12px;
    cursor: pointer;
  }
  .primary:hover:not(:disabled) {
    background: #1d4ed8;
  }
  .primary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .danger {
    margin-top: 6px;
    background: #dc2626;
    color: white;
    border: none;
    border-radius: 4px;
    padding: 6px 14px;
    font-size: 12px;
    cursor: pointer;
  }
  .danger:hover {
    background: #b91c1c;
  }

  /* 握手审批覆盖层 */
  .approval-overlay {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    backdrop-filter: blur(4px);
    -webkit-backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 12px;
    z-index: 100;
  }
  .approval-card {
    width: 100%;
    background: rgba(40, 40, 46, 0.98);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 8px;
    padding: 14px 12px 12px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    box-shadow: 0 4px 24px rgba(0, 0, 0, 0.4);
  }
  .approval-icon {
    font-size: 28px;
    line-height: 1;
  }
  .approval-title {
    font-size: 13px;
    color: #d1d5db;
  }
  .approval-device {
    font-size: 15px;
    font-weight: 600;
    color: #fef3c7;
    margin: 2px 0;
    word-break: break-all;
    text-align: center;
  }
  .approval-hint {
    font-size: 10px;
    color: #9ca3af;
  }
  .approval-hint .countdown {
    font-weight: 600;
    color: #fde68a;
    font-variant-numeric: tabular-nums;
  }
  .approval-actions {
    display: flex;
    gap: 8px;
    margin-top: 10px;
    width: 100%;
  }
  .approval-actions button {
    flex: 1;
  }
  .approval-queue {
    margin-top: 6px;
    font-size: 10px;
    color: #9ca3af;
    font-style: italic;
  }

  /* 拖放覆盖层 */
  .drop-overlay {
    position: absolute;
    inset: 0;
    background: rgba(37, 99, 235, 0.25);
    border: 2px dashed #60a5fa;
    border-radius: 10px;
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 90;
    pointer-events: none;
  }
  .drop-overlay.dim {
    background: rgba(0, 0, 0, 0.5);
    border-style: solid;
    border-color: rgba(255, 255, 255, 0.15);
  }
  .drop-hint {
    text-align: center;
    color: #f3f4f6;
    font-size: 12px;
    line-height: 1.6;
  }
  .drop-icon {
    font-size: 32px;
    line-height: 1;
    margin-bottom: 4px;
  }
  .drop-sub {
    font-size: 10px;
    color: #d1d5db;
    opacity: 0.8;
  }
</style>
