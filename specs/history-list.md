---
status: SPEC_DRAFTED
owner: product-strategist
related_adrs: []
related_specs: [00-product-overview, floating-window, clipboard-text-sync, clipboard-image-sync, file-transfer-drag]
created: 2026-05-06
updated: 2026-05-06
revised: 2026-05-06 — P2-3 UX 段由 ux-designer 填写
priority: P1
---

# history-list — 浮窗历史列表（最近 50 条 / 单击复制 / 单条删除 / 清空）

## 1. 问题（为什么做）

剪切板的天然弱点是"只有一条"——刚复制的 URL 一覆盖就找不到了。Sync Copy 因为本来就是个"看着剪切板的 daemon"，自然能多记几条形成时间倒序的历史。这件事**不需要任何额外协议**——本机已经有所有进过同步的内容（无论 Source::Local 还是 Source::Remote），只需要 UI 把它们呈现出来 + 让用户单击复用 + 单条删除 + 全部清空。

本 feature 是浮窗的**主体内容**——除了顶部状态栏与底部 IP 栏外，浮窗中央 90% 面积是这个列表。

## 2. 用户故事

- As a user who copied 3 things in the last 5 minutes, I want to scroll through the history list and click the older one to re-copy it, so that "I forgot to keep that earlier copy" stops being a problem.
- As a user with a sensitive item in history (a password I just sent), I want a one-click delete on that single row, so that it disappears immediately and ideally also from my other devices.
- As a user, I want to see at a glance "where this came from"—my own machine vs which other device—and "how long ago", so that I can tell my own copies from a colleague's pushes.
- As a user, I want clicking a file row to reveal the file in Finder/Explorer, not "copy" it (files do not go on clipboard).

## 3. 范围

**in scope**：
- 浮窗 main view 中央占满区域（高度 = 容器高 - 顶部状态栏 ~36px - 底部 footer ~24px - brand line ~16px），内部纵向 scroll
- 数据源：`get_history` Tauri 命令返回 `Vec<HistoryItem>`（结构见下），首屏挂载时拉一次 + 监听 `history-updated` 事件每次刷新
- `HistoryItem` 结构（与后端 `history.rs::HistoryPayload` 对应）：
  - `id: String` (UUID)
  - `timestamp_ms: u64`
  - `source: { kind: "local" } | { kind: "remote", device_name: String }`
  - `content_hash: Option<String>`
  - payload tagged enum：`text { text }` / `image { width, height, data_url }` / `file { filename, size, saved_path?, file_status: "sent"|"received"|"failed", error? }`
- 渲染规则：
  - **文本条目**：白色文字，1-2 行（CSS line-clamp 2）
  - **图片条目**：缩略图（max-height 80px 等比缩放）+ 角标 `${width}×${height}`
  - **文件条目**：📎 + filename + 副标题 `${formattedSize} · ${statusBadge}`，状态徽章 `已保存 / 已发送 / 保存失败：<error>`
  - 每条底部 meta 行：`<source-label> · <relative-time>`
    - source 本机 → `本机`；source remote → `来自 <device_name>`
    - 时间相对：`刚刚 / N 分钟前 / N 小时前 / N 天前`
- 单击行为：
  - text → `recopy_history_item(id)` 命令把文本写回系统剪切板（用 ClipboardCmd::SetTextSuppress 防触发广播）+ 短暂 flash chip `已复制` 1.2s
  - image → `recopy_history_item(id)` 把 PNG 解码 + 写回剪切板（ClipboardCmd::SetImageSuppress）+ flash
  - file → `reveal_file(path)`：mac `open -R`、win `explorer /select,`，不复制；若 saved_path 为空（发送失败 / 接收方未保存）则不响应或显示 banner `路径不可用`
- 单条删除按钮（行右上角 ✕，hover 时显现）→ `delete_history_item(id)` → 本机历史移除 + emit history-updated + 跨机同步删除（见 `history-sync-delete` P2）
- 列表空态：`还没有同步过\n复制一段文本试试` 居中提示（每条 source/timestamp/payload 三行结构）
- 列表上限：暂定 50 条，超出时最旧条目从尾部弹出（`MAX_HISTORY = 50` 在 `history.rs` 已实现）；待 ADR 决定是否暴露设置项让用户调（见 第 7 节 [P2] [架构师]）
- 同 content_hash 去重：新条目 push 前若 head 是同 hash 跳过；非 head 但存在同 hash 时移除旧的再 push 新的（保证最新位置）

**out of scope**：
- 历史持久化（00 总览 第 3 节 锁定不持久化；进程退出即清）
- 关键字搜索（v2 不做；50 条上限下用户肉眼可扫）
- 历史分组（按设备 / 按类型 tab 切换）—— v0 单一时间线，v2 沿用
- 历史导出 / 备份
- 跨机同步删除细节（属 `history-sync-delete`，本 spec 仅约定**调用** `delete_history_item` 后会触发 broadcast，细节由 P2 spec 定义）
- 全量清空 UI（按钮在 `settings-panel`，本 spec 仅约定后端 `clear_history` 命令的存在）
- 富文本预览 / Markdown 渲染（text 条目纯文本展示）
- 编辑历史条目（只读 + 删）

## 4. 验收标准（Definition of Done）

- [ ] 在 A 上复制 3 段文本 → A 浮窗历史从顶到底依次列出 3 条 + 每条显示"本机 · 刚刚"
- [ ] B 已加入小组：A 复制后 1 秒内 B 浮窗历史顶部出现新条目，标 "来自 A · 刚刚"
- [ ] 单击 A 上某条旧文本 → 系统剪切板内容变为该文本 + UI 短暂出现 `已复制` chip 1-2s
- [ ] 单击 A 上某图片条目 → 系统剪切板内含图片，可在 Preview / Paint 粘出
- [ ] 单击 A 上某 file 条目（已保存）→ 系统文件管理器打开该文件父目录并选中文件
- [ ] 单击 file 条目状态 = failed 或 saved_path = None → 不动作或显示 banner `路径不可用`
- [ ] 鼠标悬停某条 → 行右上角出现 ✕ 删除按钮；点击 ✕ → 该行 50ms 内消失 + history-updated 触发
- [ ] 历史超过 50 条时新条目入栈 → 最旧一条从尾部弹出（`MAX_HISTORY = 50`）
- [ ] 在 A 上复制完全相同的文本两次 → 历史只有一条且位于头部
- [ ] 列表空态展示 `还没有同步过\n复制一段文本试试` 居中提示
- [ ] 50 条全部为图片（每张 5 MB）时浮窗滚动流畅（≤ 16ms/frame，前提：data_url 渲染优化由架构师在 ADR 决策；本验收仅 smoke）

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的
`legacy-prototype` 分支 `src-tauri/src/history.rs`：`History` struct 持 `RwLock<VecDeque<HistoryItem>>`，`MAX_HISTORY = 50`；`push_text/push_image/push_file` 算 SHA-256 → 调 `insert` 函数：`if head.hash == new.hash → return None`（去重 + 不动）；否则 `retain` 移除其它同 hash 旧条 + `push_front` + 截尾到 50。`commands.rs::get_history -> Vec<HistoryItem>`、`delete_history_item(state, app, id)` → `history.remove(&id)` + emit `history-updated` + 若有 content_hash 异步 `broadcast_delete`、`clear_history(state, app)` → `history.clear()` + emit + 异步 `broadcast_clear_history`、`recopy_history_item(state, id) -> Result<(), String>` → `history.snapshot().find(id)` → match payload：text 发 SetTextSuppress、image base64 解 + 发 SetImageSuppress、file 返错 `文件条目不支持复制到剪切板`。前端 `+page.svelte`（约 600-720 行）模板：`{#each history as item (item.id)}` → `class:item-image=...` 三态 → 各自 markup（`item-text` / `item-img-wrap` + `item-img-dim` / `item-file` 含 file-icon + file-info + status 徽章）→ `item-meta` 显示 source + timeAgo + flash chip → `del-btn` 行右上角 ✕。`flashId` $state 在 `copyItem` 触发后设 1.2s 高亮。`sourceLabel(source)` / `timeAgo(ts)` / `formatSize(bytes)` 工具函数。空态用 `.empty` div。

### 5.2 v0 暴露的具体坑
- **图片 data_url 内嵌 base64 在前端 50 张大图时占用 ≈ 250 MB DOM 内存**，没有缩略 cache 或 lazy 渲染（00 总览 第 5.4 节 已点名）
- **去重规则在 push 时按 head hash 跳过 + 非 head 同 hash 移除再 push**：相当于"复用同内容时把旧条提到顶部"——但 timestamp 仍是新的，可能让人误以为旧条被删了。v0 用户没投诉这点
- **content_hash = SHA-256 算的是明文**（text 是字符串字节、image 是 PNG 字节、file 是文件字节）—— 跨机器一致是优点，但同 LAN 抓包者可对照"两条消息是否同明文"。安全 trade-off 已在 `clipboard-text-sync` 第 5.2 节 + `clipboard-image-sync` 第 5.2 节 列出
- **删除按钮无确认**：误点即删 + 触发跨机广播；用户偶尔抱怨。v0 选了"无摩擦优于无误删"
- **flash 反馈用 ID 单值 `flashId` $state**：同时 flash 多条不可能（罕见场景）
- **空态文案不区分**：`还没有同步过` —— 用户首次启动 vs 用户清空过历史区分不出来
- **timeAgo 的"刚刚"阈值**：v0 < 60s 都是"刚刚"；用户期待"几秒前"更精细的不强烈
- **file 条目 saved_path 空 + click → 无视觉反馈**（v0 alert，体验差）
- **历史与 routes/+page.svelte 中 1483 行混在一起**：渲染 + 状态 + 工具函数全在一个文件 → 单组件抽出（HistoryList.svelte）是 v2 强制要求

### 5.3 v2 应继承
- VecDeque<HistoryItem> + RwLock + MAX_HISTORY = 50
- HistoryPayload 三态 enum：text / image / file
- content_hash = SHA-256 + push 时去重（head match → skip；非 head 同 hash → retain 移除 + 重 push_front）
- get_history / delete_history_item / clear_history / recopy_history_item 四个 Tauri 命令
- history-updated 事件
- 单击 text/image → recopy；file → reveal
- 行 hover 显示 ✕ 删除按钮 + flash chip 反馈
- formatSize / timeAgo / sourceLabel 工具

### 5.4 v2 应挑战
- **HistoryList 必须独立 Svelte 组件**（00 总览 第 5.4 节 + floating-window 第 5.4 节 已点名禁止单文件堆砌）
- **图片缩略 vs 全尺寸 data_url**：是否在历史里存 thumbnail（小尺寸）+ 单击查看大图？或图片落盘到 cache 目录用路径引用？50 张 5 MB 图常驻内存的对策必须在 ADR 写明
- **删除二次确认**：是否对"敏感"条目（如 password 看起来）加确认？或全部加 cmd+click 跳过确认的快捷？v0 无确认有人投诉
- **空态文案分场景**：首次启动 vs 用户清空（清空后给"已清空"提示替代默认空态）
- **file 条目 click 路径不可用反馈**：banner / 灰化 / icon 区分；v0 alert 不友好
- **跨机同步删除的 UX 是否"删了就在所有机器删了"对用户够透明**？是否需要本机删除前给一个 "也在其它设备删除" checkbox？v0 强制全机删除——决策属 `history-sync-delete` 与 UX 共商

## 6. UX 设计

> 本节由 ux-designer 在 2026-05-06 填写。颜色/字号/字体/间距等视觉语言定义见 floating-window.md 第 6.5 节，本节不重复定义。

### 6.1 信息架构

历史列表是浮窗主体，占据容器中央全部可用高度（约 344px = 420 - 36 - 24 - 16px）。每条 HistoryItem 展示的信息按重要性排列：

1. 内容预览（文本 / 缩略图 / 文件名）——最关键，用户靠它辨认
2. 来源标签（本机 / 来自 X）——次要，帮助区分自己的 vs 别人推来的
3. 时间（刚刚 / N 分钟前）——辅助，帮助定位
4. 删除按钮（hover 显现）——操作，不占常驻视觉空间

### 6.2 关键流程图（文字版）

主路径（复制条目）：

1. 用户在列表看到目标条目 → 单击行主体
2. 文本或图片条目：`recopy_history_item` → 写回系统剪切板 → 行内短暂显示"已复制" chip 1.2 秒
3. 文件条目：`reveal_file` → Finder / Explorer 高亮文件

主路径（删除单条）：

1. 用户鼠标悬停某行 → 右上角 ✕ 显现
2. 点击 ✕ → 50ms 内该行消失 → 触发 `history-updated`

异常路径：

- file 条目 saved_path 不可用时：单击行不触发 reveal，行内显示"路径不可用"banner（1.5s 自消失）
- 复制操作失败时：行内显示"复制失败"chip（红色，1.2s 自消失）
- 历史为空时：显示空态占位（见 6.6）

### 6.3 ASCII wireframe（必填）

列表整体（填满浮窗中央区域，内部可滚动）：

```
┌──────────────────────────────────┐
│ [历史列表，纵向 scroll]            │
│                                  │
│ ┌──────────────────────────── ✕ ┐│← hover 时 ✕ 显现
│ │ hello world，这是一段示例文…   ││← 文本：13px 主色，line-clamp 2
│ │ 本机 · 刚刚                    ││← meta 行：12px 次要色
│ └────────────────────────────── ┘│
│                                  │
│ ┌──────────────────────────── ✕ ┐│
│ │ [缩略图 max-h:80px] 1920×1080  ││← 图片：缩略图左 + 尺寸右下角标
│ │ 来自 工作 Mac · 3 分钟前        ││
│ └────────────────────────────── ┘│
│                                  │
│ ┌──────────────────────────── ✕ ┐│
│ │ 📎 report.pdf                   ││← 文件：图标 + 文件名 13px
│ │    2.0 MB · 已保存              ││← 副标题：12px，状态徽章
│ │ 来自 工作 Win · 30 分钟前        ││← meta 行
│ └────────────────────────────── ┘│
│                                  │
│ ┌──────────────────────────── ✕ ┐│
│ │ 📎 photo.png                    ││
│ │    4.8 MB · 已发送              ││
│ │ 本机 · 1 小时前                  ││
│ └────────────────────────────── ┘│
└──────────────────────────────────┘
```

文本条目详细结构：

```
┌─────────────────────────────── ✕ ┐
│ hello world，这是一段示例文字…    │← 13px #f3f4f6，line-clamp: 2
│ 本机 · 刚刚                       │← 12px #9ca3af，flex row
└───────────────────────────────── ┘
  ↑ 内边距：水平 8px，垂直 6px
  ↑ 条目高度：弹性（约 52-66px 随内容）
```

单击复制后的 flash 状态（行内）：

```
┌──────────────────────── [已复制 ✓] ┐← chip 出现在右上，替换 ✕，绿色
│ hello world，这是一段示例文字…      │  1.2s 后 chip 消失
│ 本机 · 刚刚                         │
└──────────────────────────────────── ┘
```

空态（无历史条目时）：

```
┌──────────────────────────────────┐
│                                  │
│                                  │
│       还没有同步过               │← 13px #9ca3af，居中
│       复制一段文本试试            │← 12px #9ca3af，居中
│                                  │
│                                  │
└──────────────────────────────────┘
```

### 6.4 交互细节

点击区域划分：

- 行主体（除 ✕ 按钮外）：整行可点击，触发 复制 / reveal 操作
- ✕ 按钮（右上角 20×20px 热区）：仅 hover 时显现，点击删除
- meta 行：不可点击，仅展示
- 文件条目的状态徽章（"已保存 / 已发送 / 保存失败"）：不可点击，仅展示

鼠标悬停反馈：

- 行整体：背景微亮（`rgba(255,255,255,0.04)`），`cursor: pointer`
- ✕ 按钮出现（从 opacity 0 → 1，建议 CSS transition）：颜色 `#9ca3af`
- ✕ 悬停：变为 `#ef4444`（danger red），`cursor: pointer`

点击反馈：

- 文本 / 图片单击：行右上角显示"已复制 ✓" chip（绿色 `#22c55e` 背景，白字，8px 圆角），持续 1.2 秒后自动消失
- 文件单击：无 chip（reveal_file 是系统操作，没有本地视觉反馈）；若路径不可用则行内显示红色 banner "路径不可用"（1.5s 消失）
- ✕ 点击：行在约 50ms 内从列表消失（不做淡出动画，直接移除）

删除策略：

- 无二次确认弹框（与 v0 保持一致，"无摩擦优于无误删"）
- 理由：历史条目价值有限且可通过剪切板重新产生；减少确认步骤更符合"轻度伴随工具"的产品调性
- 潜在隐患：如果 P2 实现了跨机同步删除，用户一次点击可能删除所有设备的历史，届时再评估是否需要对"跨机删除"增加确认提示（属 history-sync-delete spec 的 UX 边界）

长文本处理：

- 文本条目：`-webkit-line-clamp: 2`，截断用 `…` 表示；悬停不展开全文（保持简洁）
- 文件名：单行 `text-overflow: ellipsis`，最长显示到约 180px 宽度

图片缩略策略：

- 显示缩略图（max-height: 80px，宽度等比），尺寸角标叠在右下角
- 单击复制回剪切板，不展开大图 modal（避免在小浮窗内嵌套大图查看器）
- 图片数据来源：`data_url`（base64）直接在 `<img>` src 中用；内存问题属架构师 ADR 决议

滚动条：

- 使用系统默认滚动条样式（macOS 自动隐藏细条，Win 标准滚动条）
- 不自定义滚动条外观（避免平台不一致）

状态颜色：见 floating-window.md 第 6.5 节。

键盘可达性：

- 列表整体可聚焦（`tabindex=0`），上下方向键在条目间移动
- Enter：在聚焦条目上触发单击操作（复制 / reveal）
- Delete 键：在聚焦条目上触发删除（等价于点 ✕）
- 优先级低于鼠标交互，作为可达性补充而非主要操作路径

### 6.5 状态与颜色字典

详见 floating-window.md 第 6.5 节。历史列表特有的颜色说明：

| 元素 | 颜色 | 说明 |
|---|---|---|
| 文本条目主体 | `#f3f4f6` | 13px 主文字色 |
| meta 行（来源 + 时间） | `#9ca3af` | 12px 次要色 |
| ✕ 按钮（常态） | `#9ca3af` | 仅 hover 可见 |
| ✕ 按钮（悬停） | `#ef4444` | danger red |
| "已复制" chip 背景 | `#22c55e` | 成功绿 |
| "路径不可用" banner 背景 | `rgba(239,68,68,0.15)` | 浅红背景 |
| "路径不可用" banner 文字 | `#ef4444` | danger red |
| 文件状态徽章"已保存" | `#22c55e` | 成功绿 |
| 文件状态徽章"已发送" | `#9ca3af` | 中性灰 |
| 文件状态徽章"保存失败" | `#ef4444` | danger red |
| 行悬停背景 | `rgba(255,255,255,0.04)` | 极浅白 |

### 6.6 边界与例外

- 历史 = 0 条时：显示空态（"还没有同步过 / 复制一段文本试试"），居中显示，12-13px `#9ca3af`；不显示插画（避免设计资产依赖）
- 历史 = 1 条时：正常显示，无特殊处理
- 历史 = 50 条时：列表可滚动；最旧条目在底部；新条目从顶部插入（不做"条目入场动画"，直接出现）
- 超过 50 条：最旧条目静默从底部弹出，不给用户提示（行为合理，用户不需要知道"有一条被扔掉了"）
- 图片条目缩略图加载失败：显示占位块（灰色矩形 + 图片图标），不崩溃
- 文件条目 saved_path 为空（发送方或保存失败）：行主体不响应点击；鼠标保持默认 cursor，不显示 pointer
- 列表内容更新（history-updated 事件）：直接刷新列表，不做 diff 动画（简单可靠）
- 实测可能暴露的问题：50 张大图同时渲染的内存问题需要在 ADR 和实测中验证；删除无二次确认在 P2 跨机删除后的 UX 可能需要重新评估

### 6.7 给前端工程师的实现提示（可选）

- ✕ 按钮的 hover 显现建议用 CSS parent-hover 联动（`.item:hover .delete-btn { opacity: 1 }`）而非 JS 状态控制
- "已复制" chip 和"路径不可用" banner 建议用行内绝对定位叠在条目右上角，避免影响行高和列表滚动位置
- 列表滚动容器应设置 `overflow-y: auto`（内容少时无滚动条，多时自动出现）

### 6.8 给主窗口和 PM 的反馈（UX 开放问题回应）

**问题 1**：文本 / 图片 / 文件三类条目的视觉差异。

结论：已在 6.3 wireframe 和 6.4 定义。核心差异：文本条目高度由内容决定（line-clamp 2，约 52-66px）；图片条目高度含缩略图（约 96-110px）；文件条目高度固定（三行：文件名 + 副标题 + meta，约 66px）。三类条目共用相同的边框、间距、hover 和 ✕ 按钮样式，视觉语言统一。

**问题 2**：长文本预览长度。

结论：`-webkit-line-clamp: 2`，约 2 行 × 13px 行高 = 显示约 60-80 个汉字或 100-130 个英文字符。不做 hover 展开（浮窗空间有限，全文展开会推移其它条目）。用户若需要全文，单击复制后在目标应用粘贴查看。

**问题 3**：删除按钮位置与确认弹框。

结论：删除按钮位于行右上角，hover 时显现（从 opacity 0 → 1），无二次确认。这个决策在 v0 已验证可接受（用户无投诉）。在 P2 引入跨机同步删除后，若用户反馈误删问题，届时评估是否对"跨机删除"增加确认提示。

**问题 4**：空列表占位文案。

结论：使用"还没有同步过 / 复制一段文本试试"，与 v0 一致。不区分"首次启动"和"用户清空"的空态（区分复杂度高、价值有限）——但注意：settings-panel 清空后应由 settings-panel 自己显示"已清空"反馈（在 banner 里），历史列表回到空态后只显示默认空态文案。

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 2 条] [P1 4 条] [P2 2 条]

- [P0] [架构师] HistoryList.svelte 独立组件的属性边界（接收 history props / 派发 click / delete / clear 事件）—— 与父组件 FloatingWindow 的契约
- [P0] [安全] 图片条目的 content_hash 跨机器一致性带来的 metadata 泄露——同 `clipboard-image-sync` 第 7 节 / `clipboard-text-sync` 第 7 节
- [P1] [架构师] 图片 data_url 内嵌的内存占用对策：缩略图 cache / 落盘 + 路径引用 / lazy 渲染——选哪条 + 是否 P1 上线
- [P1] [架构师] 跨机同步删除是 fire-and-forget 还是给本机删除返回的 Promise 等所有 peer 应用？UI 会不会因网络慢看到"本机删了但远端还在"
- [P1] [UX] 删除是否二次确认？v0 不确认有偶发误删
- [P1] [UX] 单击 text/image 后的反馈强度（chip 1.2s vs 行高亮 vs toast）
- [P2] [UX] file 条目 saved_path 不可用时的视觉处理
- [P2] [架构师] 50 条上限可配？某些用户希望保留更多。本 spec 第 3 节 暂定 50 待 ADR 决议

## 8. Review 段（占位）

> code-reviewer / tech-architect / ux-designer 后续填写。本 feature 是浮窗 UX 主体，UX 段必须由 ux-designer 完整填写后才能进入实现阶段。
