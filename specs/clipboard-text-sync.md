---
status: SPEC_REVIEWED
owner: product-strategist
related_adrs: [ADR-003, ADR-011]
related_specs: [00-product-overview, group-discovery, e2e-encryption, group-approval]
created: 2026-05-06
updated: 2026-05-08
revised: 2026-05-08 — ADR-003 锁定 clipboard 模块切分 (clipboard/poll.rs + encode.rs) + PeerState.last_seen_seq_by_kind dedup + boundary NetworkError 422 解密失败映射
priority: P0
---

# clipboard-text-sync — 跨设备文本剪切板自动同步（MVP 核心闭环收尾）

## 1. 问题（为什么做）

剪切板同步是 Sync Copy 的**第一性功能**——所有其它能力（图片/文件/历史/审批/加密）都是为支撑它而存在的（00 总览 第 1 节）。具体场景：用户在 Mac 上 `Cmd+C` 一段文字 → 不到 1 秒 Windows 上能 `Ctrl+V` 直接粘出。这是衡量 v2 "MVP 是否能用" 的唯一终极标准（00 总览 第 4 节 项目级验收 #3）。本 feature 是 P0 闭环里最后一块，依赖 `group-discovery + e2e-encryption + group-approval` 三块全部就位才有意义。

工程挑战不止"能传"，还有：环路防止（A 收到 B 推过来的文本写入 A 自己剪切板，A 的轮询不能再推回 B）、去重（系统剪切板偶发同内容多次刷新）、异源去重（截图写入 image 时系统可能同时把 metadata 写入 text，要避免误传）、超长文本拒绝（上限 1 MB）。

## 2. 用户故事

- As a multi-machine developer, I want text I copied on machine A to appear in machine B's clipboard within 1 second, so that I can paste it with the system shortcut on B as if I'd copied it locally.
- As a user, I do not want my own clipboard to bounce back and forth in an infinite loop when receiving a remote copy, so that paste behavior is stable and predictable.
- As a user copying long text (millions of characters / a binary blob accidentally as text), I want the app to skip syncing it (silently) rather than blow up the network or memory.

## 3. 范围

**in scope**：
- 独立 std::thread 持有 `arboard::Clipboard`（Tauri 主线程不能 own arboard），用 `mpsc::Sender<ClipboardCmd>` 接收来自其它任务的写指令
- 1 秒间隔轮询本机剪切板：调 `clipboard.get_text()` → 与上次记录的 `last_text: Option<String>` 比较 → 不同且非空且 ≤ 1 MB 则触发：
  - `state.history.push_text(text, Source::Local)` 进历史 + emit `history-updated` Tauri 事件
  - `tauri::async_runtime::spawn(network::client::broadcast_text(state, text))` 异步发给所有 peer
- `ClipboardCmd::SetTextSuppress(text)` 写入路径：`clipboard.set_text(&text)` → 立即更新 `last_text = Some(text)` 防止下一次轮询误判为本地新复制（环路防止）
- 接收路径（在 axum `/clipboard` handler 里，文本 kind）：
  - 校 origin_device_id 在 peers 表 → 否则 403
  - `state.seen_seq_and_update(origin, seq)` 去重 → 重复则 200 OK 静默丢
  - 取 `peer_keys[origin]` → AES-GCM decrypt（`e2e-encryption` 提供）
  - `state.history.push_text(plaintext, Source::Remote{device_name})` 进历史
  - 把 `ClipboardCmd::SetTextSuppress(plaintext)` 发给剪切板线程
  - emit `history-updated` 事件
- 文本上限 `MAX_TEXT_BYTES`：暂定 1 000 000（1 MB），待 ADR 决定是否提到 5 MB（见 第 7 节 [P0] [架构师]）；超过即跳过不传
- 协议字段：复用 `e2e-encryption` 与 `group-discovery` 共建的 `ClipboardReq { origin_device_id, origin_device_name, seq, nonce, ciphertext, kind: "text" }`，`image_width / image_height` 字段对 text 为空
- 截图与文本互斥：每秒先看图片（属 `clipboard-image-sync`，P1）再看文本——避免截图写入时系统也写了 metadata 文本造成双广播。本 P0 阶段仅有 text 一种 kind；轮询逻辑保持"先 image 后 text"的结构以便 P1 接入。**P0 阶段 image branch 的 stub 范围**：定义函数签名 `try_handle_image(clipboard) -> bool`（返 true 表示捕获到图片，本轮跳过 text）；P0 实现里函数体直接返 `false` 不做 `clipboard.get_image()` 调用（避免 P0 引入 image 解码依赖），text 分支正常工作；P1 接入时仅替换函数体不改其它代码

**out of scope**（v2 这个 feature 不做）：
- 图片同步（属 `clipboard-image-sync`，P1）
- 文件同步（属 `file-transfer-drag`，P1）
- 富文本 / HTML / RTF（00 总览 第 3 节 已锁死仅纯文本）
- 历史 UI 渲染（属 `history-list`，P1；本 feature 仅写 history、emit 事件）
- 历史跨机器同步删除（属 `history-sync-delete`，P2）
- 切到 OS 事件驱动 API（macOS NSPasteboard `changeCount` / Windows `AddClipboardFormatListener`）—— 见 第 7 节 [架构师]，v2 是否切换属架构师在 ADR 里决策

## 4. 验收标准（Definition of Done）

- [ ] A、B 已 `小组 · 2 台` 状态。在 A 上 `Cmd+C` 一段 ASCII 文本 → 1 秒内 B 浮窗历史顶部出现该条 + B 系统剪切板内容更新；B 上任意应用 `Ctrl+V` 粘出原文
- [ ] 在 A 上复制中文 / emoji / 多行文本 → B 上一致（UTF-8 透明）
- [ ] B 收到 A 推过来的文本写入 B 剪切板后，B 的轮询不再把同样内容反推给 A（无环路）
- [ ] 在 A 上连续两次复制完全相同的文本 → 仅第一次广播，第二次因 `last_text` 未变跳过
- [ ] 在 A 上复制超过 1 MB 的字符串 → 不广播、不进历史、不报错（静默跳过 + debug log）
- [ ] 在 A 上复制空字符串 / 全空白 → 不广播
- [ ] B 收到密文但解密失败（人为破坏密钥）→ 不写入剪切板、不进历史、log 报错；不影响后续正常文本接收
- [ ] 同一会话内 A 重启应用后再次复制文本 → 与 B 重新握手并能继续同步（继承 e2e-encryption 的协议）

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的
`legacy-prototype` 分支 `src-tauri/src/clipboard.rs`（197 行）：`spawn(app, state) -> mpsc::Sender<ClipboardCmd>` 启动独立线程；`run` 函数 loop 每 80ms tick：(1) `rx.try_recv` 处理写指令（SetTextSuppress / SetImageSuppress）；(2) 每 1 秒 poll：先看图片（`get_image()`）→ 没图片才看文本（`get_text()`）。文本路径：非空 + ≤ 1MB + 与 `last_text` 不同 → `history.push_text` + `broadcast_text`。`MAX_TEXT_BYTES = 1_000_000`。`network/protocol.rs` 的 `ClipboardReq` 已含 kind 字段（默认 "text"）。`network/client.rs` 的 `broadcast_text` 对每个 peer 取 aes_key、encrypt、POST `/clipboard`。`network/server.rs` 的 `handle_clipboard`（284-365 行）做 origin 校验 + seq dedupe + decrypt + push_text + ClipboardCmd 写入。`history.rs` 的 `push_text` 算 SHA-256 作 content_hash 用于跨机器同步删除（P2 场景）。

### 5.2 v0 暴露的具体坑
- **"先 image 后 text"的反直觉规则**：截图时系统会把图片+一段 metadata 文本同时塞进剪切板；如果先看 text 会先广播一段无意义的 metadata（如某些 Mac 截图工具写的文件路径）。v0 注释里有但易回归
- **`SetImageSuppress` 必须把 `last_text = None`**：图片进剪切板时系统可能清空文本——下一次轮询如果看到空文本会误判为"本地新复制空字符串"。这是隐式不变式，v0 注释有但 spec 没文档化（00 总览 第 5.2.1 节 已点名为"v2 必须改"）
- **每秒轮询是粗糙方案**：耗电 + 延迟下限 1s。v0 选定理由：跨平台简单、arboard 不支持事件监听。OS 原生 API（macOS NSPasteboard `changeCount` / Win `AddClipboardFormatListener`）可解，但跨平台抽象成本高。v2 是否切换属架构师 ADR 决策（00 总览 第 5.4 节 已点名）
- **环路防止靠 `last_text` 单值**：足够简单，但有边缘 case：B 收到 A 推过来的 text 后立即又被 B 用户复制了相同文本 → B 的 last_text 已是该值不会广播 → A 永远拿不到 B 的"新"复制。v0 接受这个 trade-off
- **文本 1 MB 上限**：`/clipboard` 的 axum body 上限是 8 MB（5 MB 文件 + base64 膨胀）—— text 1 MB 远低于此但仍是硬编码常量，没有 ADR 论证
- **整段文本进 AES-GCM 单条加密**：1 MB 仍可在内存中一次性加密；但若用户将来把上限提到 10 MB 需考虑分块
- **content_hash 算 SHA-256(plaintext)**：注意是明文哈希，跨机器一致；但理论上让攻击者抓多包能验证"这两条是否同明文"——属 metadata 泄露但无关键信息（同 LAN 内攻击者已能拿到密文长度和发送频率）

### 5.3 v2 应继承
- 独立 std::thread 持有 arboard + mpsc 命令模式
- 1 秒轮询间隔 + 80ms tick（让命令处理足够及时）
- "先 image 后 text" 顺序
- `last_text: Option<String>` 单值环路防止
- `SetTextSuppress` / `SetImageSuppress` 写入路径
- 1 MB 文本上限
- `ClipboardReq.kind = "text"` 协议字段
- content_hash = SHA-256(plaintext) for cross-machine dedupe

### 5.4 v2 应挑战
- **OS 事件驱动**：架构师在 ADR 论证是否切到 NSPasteboard `changeCount` (Mac, ~50ms 延迟) + `AddClipboardFormatListener` (Win)，与 1s 轮询的对比；若改，arboard 需替换 / 抽象层加一层（见 第 7 节 [P1]）
- **`SetImageSuppress` 重置 `last_text` 的不变式必须明文写进 ADR**（00 总览 第 5.2.1 节 教训；见 第 7 节 [P0]）
- **clipboard.rs 单文件 197 行**还可接受，但 image 解码 / png 编码逻辑后续 P1 加进来后会膨胀——是否提前拆 `clipboard/poll.rs` + `clipboard/encode.rs`？
- **是否在解密失败时主动 toast 给用户**（v0 仅 log）——某些场景（peer 被 ban 后老消息进来）用户应感知（见 第 7 节 [P1] [UX]）
- **超长文本上限**：见 第 7 节 [P0] [架构师]——1 MB vs 5 MB 决定后由 ADR 锁死

## 6. UX 段（占位）

> 待 ux-designer 在后续阶段填写。建议覆盖：
> - 文本同步成功后的视觉反馈（浮窗历史顶部高亮闪烁？toast？还是默默更新让用户自己感知？）
> - 收到超长文本被跳过时是否给用户提示（v0 仅 log）
> - 解密失败 / 网络失败的 UI 表现

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 4 条] [P1 3 条] [P2 0 条]

- [P0] [架构师] `MAX_TEXT_BYTES` 上限决议：1 MB（v0 沿用，简单）vs 5 MB（更宽容代码片段）。协议层 axum body 8 MB 上限对应 base64 膨胀后约 6 MB ciphertext，理论可容纳 5 MB 明文 text；本 spec 第 3 节 暂定 1 MB 待 ADR 锁死
- [P0] [架构师] `last_text = None` 在 `SetImageSuppress` 时重置是隐式不变式（00 总览 第 5.2.1 节 教训）—— 是否抽象为更显式的 `state machine`（如 `LastClipboardKind { None, Text(s), Image(hash) }`）？必须 ADR 明文化
- [P0] [安全] content_hash = SHA-256(plaintext) 暴露"两条消息是否同明文"的 metadata，是否改为 HMAC(key, plaintext) 让 hash 变成 per-pair？trade-off：跨 peer 同步删除（P2）就需要每对 peer 各算一次 hash
- [P0] [架构师] arboard 在 Linux Wayland 上不稳（00 总览 第 3 节 已锁定不支持 Linux）—— v2 编译时是否做 `cfg` 隔离避免误启用？
- [P1] [架构师] OS 事件驱动 vs 每秒轮询的 trade-off：跨平台抽象层成本 vs 电池/响应度收益。需 ADR 明文论证选哪条 + 否决路径
- [P1] [UX] 解密失败 / 跳过时的用户可见性策略
- [P1] [架构师] `broadcast_text` fire-and-forget，但若所有 peer 都失败用户 0 反馈——是否在状态栏显示"上次同步：3 台中 1 台失败"？

## 8. Review 段（占位）

> code-reviewer / tech-architect / security-reviewer 后续填写。本 feature 涉及网络协议层的 `/clipboard` 端点，必须经 security-reviewer ACK（CLAUDE.md 第 9 节）。
