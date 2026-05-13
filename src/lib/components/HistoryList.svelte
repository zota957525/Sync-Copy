<script lang="ts">
  /**
   * HistoryList — 历史列表组件
   *
   * 渲染规则（history-list.md 第 6.3 节 wireframe）：
   *   - 文本条目：13px 主色，line-clamp 2
   *   - 图片条目：缩略图 max-h 80px + 尺寸角标
   *   - 文件条目：📎 + 文件名 + 副标题(size + 状态徽章) + meta 行
   *   - 每条 meta 行：来源标签 + 相对时间
   *   - hover 显现 ✕ 删除按钮
   *   - 单击行主体 → recopy(text/image) / revealItemInDir(file)
   *
   * 颜色：history-list.md 第 6.5 节字典 + floating-window.md 第 6.5 节
   */
  import { historyStore, delHistoryItem, recopyItem } from "$lib/stores/history.svelte";
  import type { HistoryItem } from "$lib/types";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import {
    COLOR_TEXT_PRIMARY,
    COLOR_TEXT_SECONDARY,
    COLOR_TEXT_DANGER,
    COLOR_TEXT_SUCCESS,
    FONT_SIZE_DEFAULT,
    FONT_SIZE_SECONDARY,
  } from "$lib/style/tokens";

  // ---------------------------------------------------------------------------
  // 工具函数
  // ---------------------------------------------------------------------------

  function timeAgo(ts: number): string {
    const diffMs = Date.now() - ts;
    const diffSec = Math.floor(diffMs / 1000);
    if (diffSec < 60) return "刚刚";
    const diffMin = Math.floor(diffSec / 60);
    if (diffMin < 60) return `${diffMin} 分钟前`;
    const diffHr = Math.floor(diffMin / 60);
    if (diffHr < 24) return `${diffHr} 小时前`;
    const diffDay = Math.floor(diffHr / 24);
    return `${diffDay} 天前`;
  }

  function sourceLabel(item: HistoryItem): string {
    return item.source.kind === "local" ? "本机" : `来自 ${item.source.device_name}`;
  }

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function fileStatusLabel(status: string): string {
    switch (status) {
      case "received": return "已保存";
      case "sent":     return "已发送";
      case "failed":   return "保存失败";
      default:         return status;
    }
  }

  // ---------------------------------------------------------------------------
  // 交互状态：flash id（已复制 chip）/ 错误 banner
  // ---------------------------------------------------------------------------

  let flashId = $state<string | null>(null);
  let flashError = $state<string | null>(null);  // 用于"路径不可用"banner
  let flashErrorId = $state<string | null>(null);

  let flashTimer: ReturnType<typeof setTimeout> | null = null;
  let errorTimer: ReturnType<typeof setTimeout> | null = null;

  function showFlash(id: string): void {
    flashId = id;
    if (flashTimer) clearTimeout(flashTimer);
    flashTimer = setTimeout(() => { flashId = null; }, 1200);
  }

  function showErrorBanner(id: string): void {
    flashErrorId = id;
    flashError = "路径不可用";
    if (errorTimer) clearTimeout(errorTimer);
    errorTimer = setTimeout(() => { flashError = null; flashErrorId = null; }, 1500);
  }

  // ---------------------------------------------------------------------------
  // 行点击处理
  // ---------------------------------------------------------------------------

  async function handleRowClick(item: HistoryItem): Promise<void> {
    if (item.payload.type === "file") {
      if (!item.payload.saved_path) {
        showErrorBanner(item.id);
        return;
      }
      try {
        await revealItemInDir(item.payload.saved_path);
      } catch {
        showErrorBanner(item.id);
      }
      return;
    }
    // text / image → recopy
    const ok = await recopyItem(item.id);
    if (ok) {
      showFlash(item.id);
    } else {
      flashErrorId = item.id;
      flashError = "复制失败";
      if (errorTimer) clearTimeout(errorTimer);
      errorTimer = setTimeout(() => { flashError = null; flashErrorId = null; }, 1200);
    }
  }

  async function handleDelete(e: MouseEvent, id: string): Promise<void> {
    e.stopPropagation();
    await delHistoryItem(id);
  }
</script>

<div class="list-container">
  {#if historyStore.items.length === 0}
    <div class="empty">
      <span class="empty-main" style:color={COLOR_TEXT_SECONDARY} style:font-size={FONT_SIZE_DEFAULT}>
        还没有同步过
      </span>
      <span class="empty-sub" style:color={COLOR_TEXT_SECONDARY} style:font-size={FONT_SIZE_SECONDARY}>
        复制一段文本试试
      </span>
    </div>
  {:else}
    <div class="list">
      {#each historyStore.items as item (item.id)}
        <div
          class="item"
          role="button"
          tabindex="0"
          onclick={() => handleRowClick(item)}
          onkeydown={(e) => { if (e.key === "Enter") handleRowClick(item); if (e.key === "Delete") delHistoryItem(item.id); }}
          style:cursor={item.payload.type === "file" && !item.payload.saved_path ? "default" : "pointer"}
        >
          <!-- 内容预览区 -->
          {#if item.payload.type === "text"}
            <p class="text-preview" style:color={COLOR_TEXT_PRIMARY} style:font-size={FONT_SIZE_DEFAULT}>
              {item.payload.text}
            </p>
          {:else if item.payload.type === "image"}
            <div class="image-wrap">
              <img
                src={item.payload.data_url}
                alt="图片"
                class="thumb"
                onerror={(e) => { (e.currentTarget as HTMLImageElement).style.display = "none"; }}
              />
              <span class="img-dim" style:color={COLOR_TEXT_SECONDARY} style:font-size="11px">
                {item.payload.width}×{item.payload.height}
              </span>
            </div>
          {:else if item.payload.type === "file"}
            <div class="file-wrap">
              <span class="file-icon">📎</span>
              <div class="file-info">
                <span class="file-name" style:color={COLOR_TEXT_PRIMARY} style:font-size={FONT_SIZE_DEFAULT}>
                  {item.payload.filename}
                </span>
                <span class="file-sub" style:font-size={FONT_SIZE_SECONDARY}>
                  <span style:color={COLOR_TEXT_SECONDARY}>{formatSize(item.payload.size)}</span>
                  <span class="status-badge"
                    style:color={
                      item.payload.file_status === "received" ? COLOR_TEXT_SUCCESS :
                      item.payload.file_status === "failed"   ? COLOR_TEXT_DANGER  :
                      COLOR_TEXT_SECONDARY
                    }
                  >
                    {fileStatusLabel(item.payload.file_status)}
                    {#if item.payload.file_status === "failed" && item.payload.error}：{item.payload.error}{/if}
                  </span>
                </span>
              </div>
            </div>
          {/if}

          <!-- meta 行 -->
          <div class="meta" style:color={COLOR_TEXT_SECONDARY} style:font-size={FONT_SIZE_SECONDARY}>
            {sourceLabel(item)} · {timeAgo(item.timestamp_ms)}
          </div>

          <!-- 右上角操作区：flash chip / 错误 banner / ✕ 按钮 -->
          <div class="action-area">
            {#if flashId === item.id}
              <span class="chip-copied">已复制 ✓</span>
            {:else if flashErrorId === item.id && flashError}
              <span class="chip-error">{flashError}</span>
            {:else}
              <button
                class="del-btn"
                onclick={(e) => handleDelete(e, item.id)}
                aria-label="删除"
                tabindex="-1"
              >✕</button>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .list-container {
    width: 100%;
    height: 100%;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }

  /* 空态 */
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 16px;
  }

  .empty-main,
  .empty-sub {
    display: block;
    text-align: center;
  }

  /* 列表 */
  .list {
    display: flex;
    flex-direction: column;
    padding: 4px 8px;
    gap: 4px;
  }

  /* 单条 */
  .item {
    position: relative;
    padding: 6px 8px;
    border-radius: 6px;
    transition: background 80ms ease;
  }

  .item:hover {
    background: rgba(255, 255, 255, 0.04);
  }

  .item:hover .del-btn {
    opacity: 1;
  }

  /* 文本预览 */
  .text-preview {
    margin: 0 0 2px 0;
    line-height: 1.4;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    word-break: break-all;
    padding-right: 24px; /* 留给操作区 */
  }

  /* 图片 */
  .image-wrap {
    position: relative;
    display: inline-flex;
    margin-bottom: 2px;
  }

  .thumb {
    max-height: 80px;
    max-width: 100%;
    object-fit: contain;
    border-radius: 4px;
    display: block;
  }

  .img-dim {
    position: absolute;
    bottom: 2px;
    right: 2px;
    background: rgba(0, 0, 0, 0.55);
    padding: 1px 4px;
    border-radius: 3px;
  }

  /* 文件 */
  .file-wrap {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin-bottom: 2px;
    padding-right: 24px; /* 留给操作区 */
  }

  .file-icon {
    font-size: 14px;
    line-height: 1;
    flex-shrink: 0;
    margin-top: 1px;
  }

  .file-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .file-name {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 180px;
    display: block;
  }

  .file-sub {
    display: flex;
    align-items: center;
    gap: 4px;
    line-height: 1.4;
  }

  .status-badge {
    font-size: 11px;
  }

  /* meta 行 */
  .meta {
    line-height: 1.4;
  }

  /* 操作区（右上角绝对定位） */
  .action-area {
    position: absolute;
    top: 6px;
    right: 6px;
    display: flex;
    align-items: center;
  }

  /* ✕ 删除按钮 */
  .del-btn {
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    color: #9ca3af;
    font-size: 11px;
    cursor: pointer;
    border-radius: 4px;
    opacity: 0;
    transition: opacity 120ms ease, color 80ms ease;
    padding: 0;
  }

  .del-btn:hover {
    color: #ef4444;
  }

  /* 已复制 chip */
  .chip-copied {
    display: inline-flex;
    align-items: center;
    padding: 1px 6px;
    border-radius: 8px;
    background: #22c55e;
    color: #fff;
    font-size: 11px;
    white-space: nowrap;
  }

  /* 错误 chip（路径不可用 / 复制失败） */
  .chip-error {
    display: inline-flex;
    align-items: center;
    padding: 1px 6px;
    border-radius: 8px;
    background: rgba(239, 68, 68, 0.15);
    color: #ef4444;
    font-size: 11px;
    white-space: nowrap;
  }
</style>
