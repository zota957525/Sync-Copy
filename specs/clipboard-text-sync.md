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

## 8. Code Review (by code-reviewer · 2026-05-09)

**结论**：BLOCKED — PR-5 happy path 主路径函数级实现质量高（83/83 单测 pass / clippy 0 warning / fmt 干净 / AAD 字节序与对称密钥派生数学上无误），但发现 3 个 ADR 契约级 BLOCKED 问题，其中一个直接破坏 PeerRegistry 索引正确性（多 peer 共用同一 device_id），任何主路径功能正确性都建立在"只握手一次"的隐含前提下。主窗口必须停下回报用户。

### 8.1 五聚焦点意见

1. **handshake ECDH 派生 + AAD 入参一致性**：✅ 数学层 OK。`X25519KeyExchange::derive_aes_key` 消费 EphemeralSecret + peer_pub → HKDF-SHA256(salt=`sync-copy-v2-salt`, info=`sync-copy-v2:aes-256-gcm`) → 32B；返裸数组立即由 caller `Zeroizing::new` 包装（handshake.rs:115、client.rs:356），与 ADR-009 第 3.1 节锁定一致；ADR-011 第 3.6 节 `cross_peer_keys_differ` + 本 PR 新增 `handshake_derives_correct_aes_key_and_symmetric` 两侧字节相等不变式覆盖。AAD 入参 `build_aad(kind, origin_device_id, seq)` 在 client.rs:77（发送方传 `my_device_id`）与 clipboard.rs:95（接收方传 `req.origin_device_id`）字节级对称；BE 8B seq + ASCII kind + 12B magic 唯一定义点 grep `sync-copy-v2` = 3 处（mod.rs:24 / x25519.rs:22 / x25519.rs:25），契约闭环。**但**见 8.2 [严重 #3] — handshake.rs:169 `device_id` 字段是字面占位串，dial 侧拿来当对端 ID 写入 PeerRegistry，使任意两次握手回的 PeerState 共用同一索引键 — 派生本身正确不能掩盖索引错位。
2. **clipboard decrypt + AAD 验证**：✅ 字节级正确。decrypt 失败统一 422（不区分 key/aad/tamper，符合 ADR-008 MUST-3），新增单测 `clipboard_decrypt_aad_mismatch_fails` 用 device-A 加密 / device-B 解密验证 AAD 任一字节差异致 DecryptFailed；roundtrip 单测覆盖正常路径；is_known + !is_banned 双重鉴权 + seen_seq_and_update 在第一行（ADR-009 invariant 5）次序正确。aes_key 通过 `*peer.aes_key` 拷贝 32B 后立即出读锁作用域（clipboard.rs:79-91），不持锁过 await，符合 ADR-009 第 3.4 节。SECURITY 注释提示 plaintext 不进 tracing fields（仅记 `plaintext_len`）。
3. **last_successful_sync_at 写入语义**：✅ APPROVED。heartbeat handler（heartbeat.rs:49）只调 `record_heartbeat_ok`，注释明文 "MUST NOT 更新 last_successful_sync_at"；新增 `heartbeat_updates_last_heartbeat_not_last_sync` 单测断言"两时间戳分离 + consecutive_heartbeat_failures 归零"三不变式同时满足。broadcast_clipboard 仅在 `resp.status().is_success()` 路径调 `record_send_ok`（client.rs:139），所有失败路径（encrypt error / pool miss / non-2xx / request error / timeout）均走 `record_send_fail` 不污染 last_successful_sync_at。落实 ADR-008 第 5.2 节 + ADR-009 第 3.7 节"心跳成功 ≠ 真同步"语义闭环。
4. **MUST-4 leave 原子顺序**：❌ BLOCKED。leave handler 与 `PeerRegistry::remove` / `PeerRegistry::ban` **均未调** `client_pool.remove`（详见 8.2 [严重 #1]），违反 ADR-008 第 7.2 节 MUST-4 原文 "PeerRegistry::remove(id) 实现层先 inner.remove(&id) 后 client_pool.remove(&id) 原子" + ADR-009 第 3.5 节调用顺序契约表第 2 行 + 第 5 节实施提示 #5 "client_pool.remove 由 PeerRegistry::remove 内部调用（且仅由它调用）"。leave handler 注释（leave.rs:11、leave.rs:55-63）声称"client_pool.remove 已在 PeerRegistry::remove 内部调用"——但 peer/mod.rs:215-239 实际实现没调（注释承认"PR-3 Lifecycle 落地后将在此方法尾部补充"）；ban handler（peers.rs:202）同样未补；PR-3/PR-5 都过了，此 TODO 仍未落地。当前主路径功能上未崩（broadcast 先用 `peers.snapshot()` 过滤已移除 peer，zombie reqwest::Client 不会被使用），但 invariant 3 `client_pool.contains(id) == inner.contains_key(id)` 持续违反，每次 leave/ban 累积一个 reqwest::Client 内存泄漏。
5. **broadcast_clipboard 安全边界**：⚠ APPROVED-with-nit。snapshot 仅 Approved peers（client.rs:53-58）+ broadcast_leave 同样仅 Approved（client.rs:197-202）+ snapshot 后立即释放锁不持锁过 await — 符合 ADR-009 第 7.3 节 P3 + ADR-010 v1.2 P3 SECURITY 注释。per-peer 失败全部 `record_send_fail` 不 panic。`encrypt_result` 失败也走 `record_send_fail`。**nit**：`AadKind` 匹配 `_ => "text"`（client.rs:98）当 caller 传 Trust/Ban/Leave 等 kind 时静默降级为 "text" 字符串与 `build_aad` 的 kind 字面不匹配 — 当前 caller 在 PR-5 范围内只传 Text/ImagePng 不会触发，但 PR-6 接 trust/ban/leave 广播时此分支会成为隐藏密钥认证错位 bug；建议改为 `unreachable!()` 或返 Err（详见 8.2 [低 #1]）。

### 8.2 发现的问题（按严重度排序）

#### [严重 #1] PeerRegistry::remove / ban 未调 client_pool.remove，违反 ADR-008 MUST-4 + ADR-009 第 3.5 节

- 文件：`src-tauri/src/peer/mod.rs:215-239` (`PeerRegistry::remove`) + `src-tauri/src/peer/mod.rs:298-320` (`PeerRegistry::ban`) + `src-tauri/src/network/handlers/leave.rs:64`
- 现象：`PeerRegistry::remove` 仅清 inner / approved / banned 三集合，`client_pool` 引用未清；`ban` 中 was_peer=true 路径同问题；leave handler 也未在调 `state.peers.remove` 后显式调 `state.client_pool.remove`。但 ADR-009 第 5 节实施提示 #5 同时禁止 handler 在 PeerRegistry::remove 之外直接调 client_pool.remove。
- 风险：违反 invariant 3，每次 leave/ban 永久累积一个 zombie `Arc<reqwest::Client>` 内存泄漏；ADR-008 5.3 节"防 zombie peer"语义被打破（A3 攻击主体若编造 device_id leave 后再 re-handshake 同 id，client_pool.insert 会遇到既存 entry，行为依赖 ClientPool 的 insert 实现是否覆盖）；属架构契约违反。
- 修法（**超出 backend-impl 静默修范围 — 需用户拍板**）：选项 A — 修改 `PeerRegistry::new()` 签名为 `new(client_pool: Arc<ClientPool>)`，PeerRegistry struct 加 `client_pool: Arc<ClientPool>` 字段（与 ADR-009 第 3.2 节伪代码一致），`remove` / `ban` 内部调 `self.client_pool.remove(id)`；AppState::new() 同步调整构造顺序（client_pool 先于 peers）。该改动回溯 PR-2 的字段定义。选项 B — 显式开 ADR-012 supersede ADR-009 第 5 节实施提示 #5 反模式黑名单中"handler 直调 client_pool.remove"那条，允许 leave/ban handler 显式调 — 但破坏单一入口设计意图。

#### [严重 #2] handshake handler 跳过 device_id == self 自连校验，违反 ADR-008 MUST-3

- 文件：`src-tauri/src/network/handlers/handshake.rs:67-74`
- 现象：handler 用 6 行 TODO 注释跳过"req.device_id == 本机 device_id → 403"校验，理由是"AppState 当前无 device_id 字段，TODO PR-6"+"self-connect 在 LAN 实践中极少发生"。dial_handshake 第 361-365 行**做了**对称校验。
- 风险：implementer 自创 TODO 推迟 ADR-008 第 7.2 节 MUST-3 必修清单条目，违反"不接受 = 主窗口需走 ADR 修订流程否则不能进 IMPL_IN_PROGRESS"硬阻塞条件（ADR-008 第 9 节决策卡 1 must-fix）。LAN 实践中"极少发生"不是 MUST 类目允许的延后理由；威胁模型 A2 主体可主动构造 self-handshake 探测本机 device_id 是否泄漏。
- 修法（**超出 backend-impl 静默修范围 — 需用户拍板**）：要么补 AppState.my_device_id 字段（属架构调整 — 涉及 lib.rs::run 启动序列 + lifecycle.rs step 3 my_device_id 持久化加载来源），要么开 ADR-012 supersede ADR-008 MUST-3 把"自连校验"明文降级为非必修。

#### [严重 #3] HandshakeResp.device_id 字面占位串"placeholder-my-device-id"，使多 peer 共用同一 PeerRegistry 索引键

- 文件：`src-tauri/src/network/handlers/handshake.rs:168-173`
- 现象：handle_handshake 返回 `HandshakeResp { device_id: "placeholder-my-device-id", ... }` — 硬编码字面串作为本机 device_id 返给对端；dial_handshake（client.rs:358）`let peer_id = handshake_resp.device_id.clone()` 直接用该串作 PeerRegistry 索引 + client_pool 索引 + AAD origin_device_id 输入。
- 风险：**密码学/协议级灾难**。任意主动方 dial 两台 peer 收到的 HandshakeResp.device_id 都是同一占位串 → registry.insert 用相同 device_id → 第二个 peer 的 PeerState（含 aes_key）覆盖第一个；后续 leave/ban/clipboard handler 全部按 placeholder 字符串路由，无法区分两台对端；AAD 中 origin_device_id 字段在所有"主动方收发"路径上都退化为常量，跨 peer 重放保护被绕过（A2 主体只需窃听一条密文，回放给"自己"对应的相同密钥即解密成功）。当前 happy path 单测不暴露该 bug 因为只起一对 alice-bob。任何 N=3+ 集成场景必崩。
- 修法（**超出 backend-impl 静默修范围 — 需用户拍板**）：与 [严重 #2] 同源 — 必须先在 AppState 加 my_device_id 字段（启动期生成 UUID 持久化到 config）；然后 handshake.rs:169 改为 `state.my_device_id.to_string()`。在该字段未落地前 dial_handshake 完全不可用。

#### [中等 #1] handshake handler 文档注释与 ADR-009 第 3.5 节调用顺序契约不一致

- 文件：`src-tauri/src/network/handlers/handshake.rs:151` 与 ADR-009 第 3.5 节调用顺序表第 1 行
- 现象：ADR-009 第 3.5 节调用顺序表第 1 行原文"3. 构造 reqwest::Client → 4. client_pool.insert(id, client) → 5. registry.insert(state)"。handler 实现顺序正确（pool.insert → peers.insert → peers.approve），但代码内注释 152 行写"client_pool.insert（先于 registry.insert，ADR-009 MUST-4 原子顺序）"——MUST-4 是 ADR-008 remove 路径的术语，不是 insert 路径；混用可能让未来 reviewer 误以为 MUST-4 涵盖 insert。
- 风险：低 — 仅文档误用，不影响运行时。
- 修法：注释改为"ADR-009 第 3.5 节调用顺序契约第 1 行"。

#### [低 / nit #1] broadcast_clipboard 的 AadKind 兜底分支静默降级为 "text"

- 文件：`src-tauri/src/network/client.rs:95-99`
- 现象：`let kind_str = match kind { AadKind::Text => "text", AadKind::ImagePng => "image_png", _ => "text" }`。当 PR-6 起 trust/ban/leave 走 broadcast_clipboard 时（按 spec 不应该），`_ => "text"` 让 `req_body.kind = "text"` 与 `build_aad(kind=Trust/...)` 字面不匹配，对端 decrypt 失败但日志归因困难。
- 风险：当前 caller 范围内不触发；PR-6 是隐藏雷。
- 修法：改为 `_ => unreachable!("broadcast_clipboard only supports Text/ImagePng")` 或返 Err 强制 caller 拆 broadcast 函数。

#### [低 / nit #2] dial_handshake 不校验 banned 不变式仅靠 caller 重试时再次过 banned 闸门

- 文件：`src-tauri/src/network/client.rs:367-373`
- 现象：dial_handshake 检查 `state.peers.is_banned(&peer_id)` 在 derive_aes_key 之后；理论上密钥已派生才发现 banned，浪费 ECDH 计算 + 短暂栈帧含 raw_key。
- 风险：极低 — 已 Zeroizing 包装，作用域结束自动清零；仅时延。
- 修法：可在 client.rs:357（解析 peer_id 后、Zeroizing 包装前）提前 banned 短路；非必需。

### 8.3 风险点（可能的隐藏 bug）

- handshake handler 注释 152 行 + leave handler 注释 11/55-63 行多处写"client_pool.remove 已在 PeerRegistry::remove 内部完成"——这是 implementer 写代码时按自己的"应当如何"理解写注释，但实际代码不执行该动作。code-reviewer 将来若只读注释验收会被误导。
- AppState clipboard_apply_tx 占位 None 在 PR-6 接 arboard 前 clipboard handler 解密成功后只 tracing::info 不写 OS 剪切板 — 用户体感"无效"，但 spec 第 4 节验收标准 #1（"copy A → 1 秒内 paste B"）尚未触达；属预期占位，PR-6 兑现。
- lifecycle step 3 broadcast_leave 用 `"shutdown-placeholder"` 占位 my_device_id（lifecycle.rs:294）；对端 leave handler 第一行 `is_known("shutdown-placeholder")` 必失败 → 403 → 整个 leave 广播路径在 PR-5 实质不通；与 [严重 #3] 同根，等 my_device_id 字段落地后即解。

### 8.4 测试覆盖评估

- ✅ 已覆盖：clipboard decrypt roundtrip / AAD mismatch / unknown peer / banned peer / seq dedupe / heartbeat 不更 last_sync / leave 三集合原子 / trust-ban 互斥 / announce 插入 / handshake DH 对称 / handshake insert
- ❌ 未覆盖（与 8.2 严重问题对应）：(a) PeerRegistry::remove 后 client_pool.contains == false 的 invariant 3 测试 — 若补此单测会立即红；(b) handshake handler self-connect 拒 403 — 当前 6 行 TODO 跳过；(c) HandshakeResp.device_id 与 AppState.my_device_id 一致性 — 当前用占位串无 ground truth 可比；(d) 双方 dial_handshake 后 PeerRegistry 中两 peer 的 device_id 互不相同 — 当前会同时是 "placeholder-my-device-id"
- ⚠ 边界场景缺漏：N=3+ peer 场景下 broadcast_clipboard 全部成功的端到端集成测试（happy path 至少跑一次三机循环）；leave 后 re-handshake 同 device_id 的 PeerState 覆盖语义

### 8.5 给 implementer 的明确 todo 清单（**仅在用户拍板后启动**）

> 主窗口必须先停下回报用户三个 BLOCKED 决策点（[严重 #1] [严重 #2] [严重 #3]），用户拍板后再执行下方清单。

- [ ] 修 §8.2 [严重 #3]：AppState 加 `my_device_id: String` 字段（启动期 UUID 生成 + persist 到 config）；handshake.rs:169 改为 `state.my_device_id.clone()`；lifecycle.rs:294 broadcast_leave 第二参数改为 `state.my_device_id.as_str()`
- [ ] 修 §8.2 [严重 #2]：handshake.rs:67-74 解注释自连校验 → 403 NetworkError::DeviceIdConflict
- [ ] 修 §8.2 [严重 #1]：PeerRegistry::new() 签名改为 `new(client_pool: Arc<ClientPool>)`；struct 加 `client_pool: Arc<ClientPool>` 字段；remove / ban (was_peer=true 分支) 末尾调 `self.client_pool.remove(id)`；AppState::new() 调整构造顺序 client_pool 先于 peers；删除 peer/mod.rs:232 + peer/mod.rs:315 两处 "PR-3 落地后补" TODO 注释
- [ ] 补单测：`peer::tests::remove_clears_client_pool` — insert peer + insert client_pool entry → registry.remove → assert pool.get(id).is_none()
- [ ] 补单测：`network::handlers::handshake::tests::self_connect_returns_forbidden` — req.device_id == state.my_device_id → 403
- [ ] 补单测：`network::client::tests::dial_handshake_uses_real_my_device_id` — dial 后 PeerState.device_id 不等于 "placeholder-my-device-id"
- [ ] 修 §8.2 [中等 #1]：handshake.rs:151 注释 "MUST-4" → "ADR-009 第 3.5 节调用顺序契约第 1 行"
- [ ] 修 §8.2 [低 #1]：client.rs:99 `_ => "text"` → `_ => unreachable!("broadcast_clipboard only supports Text/ImagePng")`
- [ ] 修 §8.2 [低 #2]：dial_handshake banned 校验前移到 derive_aes_key 之前


### 8.7 Code Review v2 — PR-5b 全修验证 (2026-05-10 · commit ef2979a)

**结论**：APPROVED — 3 严重违反全闭环；不引入新严重违反。残留 2 条原低/nit + 1 测试遗留死代码（leave.rs 第 167-168 注释撒谎）建议挂 PR-6 顺手清理。

#### 8.7.1 三严重违反全闭环验证

- **#1 MUST-4 + ADR-009 第 3.5 节**：✅ 真闭环。`PeerRegistry` 加 `client_pool: Arc<ClientPool>` 字段（peer/mod.rs:136）；`new(client_pool: Arc<ClientPool>)` 签名按 ADR-009 第 3.2 节字面落地（peer/mod.rs:145）；`remove` 步骤 4 调 `self.client_pool.remove(id)` 在三 set 写锁全部释放后（peer/mod.rs:251，符合 ADR-009 第 3.3.1 节锁顺序）；`ban` 在 was_peer=true 分支同步调（peer/mod.rs:335）。新单测 `remove_clears_client_pool_atomic` (peer/mod.rs:833) + `ban_clears_client_pool_when_was_peer` (peer/mod.rs:884) 真断言 `pool.get(id).is_none()`，invariant 3 闭环。
- **#2 handshake 自连**：✅ 真闭环。handshake.rs:72-81 在限流 + sanitize 之后、banned 之前真做 `req.device_id == state.my_device_id` 校验，错误类型 `NetworkError::DeviceIdConflict` → 403 + 通用 body "forbidden"（network/error.rs:93/188，ADR-008 MUST-3）。新单测 `self_dial_returns_403` (handshake.rs:285) 断言 `StatusCode::FORBIDDEN`。
- **#3 my_device_id**：✅ 真闭环。`AppState.my_device_id: String` (state.rs:71) 在 `AppState::new()` 首行 `uuid::Uuid::new_v4().to_string()` (state.rs:102)；`uuid = { version = "1", features = ["v4", "serde"] }` 在 Cargo.toml:23；`git grep "placeholder-my-device-id" src-tauri/` 仅 4 命中（state.rs:64 注释 / handshake.rs:304/318/342 测试断言对比字面量），生产路径 0 命中；`shutdown-placeholder` 仅 lifecycle.rs:289 注释保留，活代码用 `&state.my_device_id` (lifecycle.rs:293)。client.rs::dial_handshake 通过 `my_device_id: &str` 入参由 caller 传 `state.my_device_id` 注入。新单测 `resp_uses_real_my_device_id` (handshake.rs:311) 断言 UUID v4 格式 + 不等占位串。

#### 8.7.2 不引入新违反验证

- **PeerRegistry::new 调用点全更新**：生产路径仅 state.rs:106 一处 `PeerRegistry::new(Arc::clone(&client_pool))`；其余 11 个 test mod 调用点全部改用 `new_for_test()` helper（peer/mod.rs:155 `#[cfg(test)]` 限定 ✓）。无散落的旧签名调用。
- **new_for_test gating**：`#[cfg(test)]` 真限定（peer/mod.rs:155），生产路径**不可能**构造"无 client_pool 共享"的 PeerRegistry。
- **0 新 TODO**：原 "PR-3 落地后补" 注释已删（leave.rs 注释更新；peer/mod.rs 中 remove/ban 注释指向 ADR-009 第 3.5 节）。残留 TODO 全为 PR-6/PR-7 已知占位（emit / arboard / tray）— 非新增。
- **cargo 三件套**：`cargo test --lib` 87/87 pass；`cargo clippy --all-targets -- -D warnings` 0 warning；`cargo fmt --check` 0 diff（已复跑确认）。
- **v2-9 越界**：⚠ commit ef2979a `--stat` 含 `PLAN.md | 2 +-` — backend-implementer 改了 PLAN.md（边界违规，应主窗口改）。但属流程性问题，本 PR-5b 内容正确。

#### 8.7.3 新发现问题（≤ 3 条小补丁）

- **[低 #3 新]** `Default for PeerRegistry` (peer/mod.rs:424-428) **无 #[cfg(test)]** 限定，构造一个孤立 ClientPool（与 AppState.client_pool 不共享）→ 若未来代码无意 `PeerRegistry::default()` 则破坏 invariant 3。当前生产路径 0 调用（grep 验证），但属脚枪。建议给 Default impl 加 `#[cfg(test)]` 或删除（与 new_for_test 重复）。
- **[低 #4 新]** leave.rs:167-168 测试注释撒谎："PeerRegistry 不持有 client_pool" — PR-5b 已让 PeerRegistry 持有 client_pool。该注释 + `leave_atomic_remove_inner_and_pool` 测试（leave.rs:119）虽用 `new_for_test()` 与孤立 pool，但语义已被 peer/mod.rs 的 `remove_clears_client_pool_atomic` 完整覆盖；建议删 leave.rs 测试或改为对 registry.client_pool 的间接断言。
- **[未修，原低 #1/#2 残留]** client.rs:98 `_ => "text"` 仍在；client.rs:368 banned 校验在 derive_aes_key 之后 — PR-5b 范围限 3 严重 + 1 中，这两条留 PR-6 顺手清理符合预期。

#### 8.7.4 结论

APPROVED → 推 `REVIEW_PASSED`。3 严重违反全代码层 + 单测层闭环；4 新单测真验证关键不变式；cargo 三件套全 green。新发现仅 2 条 [低]（Default 脚枪 + leave 测试遗留）+ 2 条原 [低/nit] 未修（PR-5b 范围外）— 均挂 PR-6 顺手清理，不阻塞 backend MVP 里程碑。

### 8.8 Code Review v3 — PR-6a 真业务接入 (2026-05-10 · commit fd0573c)

**结论**：CHANGES_REQUESTED — 1 [严重] ADR 契约级违反（shutdown 100ms 软上限未真实现）+ 1 [中等] 日志噪音 + 2 [低]；功能层环路防止逻辑正确；4 nit 全闭环 + cargo 三件套 green + 96/96 tests pass。

#### 8.8.1 五聚焦点意见

- **环路防止**：✅ 代码逻辑正确。`apply_text_to_clipboard` (clipboard.rs:244-264) 先 `set_text` 后立即更新 `last_hash`，写入失败分支不更新 hash（顺序原子，v0 教训落地）；`poll_text_clipboard` (clipboard.rs:320-325) 比较 hash 跳过未变化。AC #3/#4 闭环。
- **lifecycle 集成**：❌ 不合格。**`ClipboardWatcher::shutdown`（clipboard.rs:110-143）未真正实现 100ms 软上限**：helper.join() 无 timeout，违反 ADR-010 第 3.3 节 step 4 "clipboard 100ms 软上限" 契约（见 8.8.2 严重 #1）。step 4 启动失败 unwind 路径 ✓；apply_rx `Mutex<Option<Receiver>>` take 单点 ✓（lifecycle.rs:202）。
- **mpsc 通道选型**：✅ 选 `std::sync::mpsc::sync_channel(64)` 合理（arboard 在 std::thread，自然搭配）；handler 内用 `try_send` 非阻塞（clipboard.rs:127 handler）；buffer 64 对快速复制场景已够（每秒最多 1 次本地 poll + 远端入 64 buffer，溢出概率极低）。
- **4 nit 真闭环**：✅ 全闭环。#1 (client.rs:101 unreachable!) — AadKind 有 9 变体，broadcast_clipboard 只用 Text/ImagePng，其它编程错误；#2 (client.rs:363-371 banned 前移到 derive_aes_key 之前) ✓；#3 (peer/mod.rs:425 `#[cfg(test)] impl Default`) — 生产路径 grep 0 调用 ✓；#4 (leave.rs 删除 leave_atomic_remove_inner_and_pool) — 等效覆盖在 peer/mod.rs:839 remove_clears_client_pool_atomic ✓。
- **不引入新违反**：✅ cargo clippy --all-targets -D warnings 0 warning；cargo test --lib 96/96 pass；commit 不含 PLAN.md / target / DS_Store；生产路径无 TODO / unimplemented / unsafe；placeholder 残留全在 #[cfg(test)] 或文档注释内（grep 验证）。

#### 8.8.2 新发现问题

##### [严重] shutdown 100ms 软上限未真实现，违反 ADR-010 第 3.3 节 step 4 契约
- 文件：`src-tauri/src/app/clipboard.rs:113-120`
- 现象：注释声称"100ms 软上限：用 park_timeout 不可靠，改用 spin-wait join. 实现：用另一线程 join，主线程等 100ms"，但实际 helper.join() **无任何 timeout**：`.spawn(move || handle.join()).ok().and_then(|h| h.join().ok())`——主线程对 helper 的 join 同样是无限等。若 clipboard 线程因 arboard 死锁（v0 教训：Windows 偶发占用）不退出，shutdown 无限阻塞 → 违反 ADR-010 第 3.3 节"clipboard 100ms 软上限 detach"+ 总硬上限 ≤ 2800ms。
- 风险：用户点退出 → 进程 hang；ADR-010 lessons-learned 第 4 段 4 退出路径全部失守。
- 建议修法：用 `std::sync::mpsc::channel()` + helper 线程内 `let _ = handle.join(); let _ = done_tx.send(());`，主线程 `done_rx.recv_timeout(Duration::from_millis(100))`，超时即 detach helper（不再 join 它）。tracing 区分 "joined" / "timeout detached" 两种路径。

##### [中等] lifecycle.rs:217 broadcast_rx 立即 drop → 每次本地剪切板变化触发一行 warn 日志
- 文件：`src-tauri/src/app/lifecycle.rs:217`
- 现象：step 4 内 `let (broadcast_tx, _broadcast_rx) = mpsc::sync_channel::<ClipboardEvent>(64);` — `_broadcast_rx` 在 scope 结束立即 drop。watcher 线程内 `broadcast_tx.try_send` (clipboard.rs:331) 每次本地剪切板变化都返 `Disconnected` 错误并触发 `tracing::warn!`（clipboard.rs:332-336）。commit message 说"接收侧 PR-7 处理"是有意为之，但日志层面会让看 prod log 的人误判为 bug。
- 风险：信号噪音 — 真出问题时反而被淹没；用户 P0 自测时看到 warn 会疑惑。
- 建议修法：方案 A — 在 PR-6a 阶段把 broadcast_tx.try_send 失败的 warn 降级为 trace + 注释 "PR-7 接收侧落地前预期 Disconnected"；方案 B — PR-6a 仍保 broadcast_rx，spawn 一个吸收线程 `loop { let _ = rx.recv(); }` 让 try_send 不再 Disconnected（更干净）；方案 C — PR-6a 不构造 broadcast channel，watcher 临时接 `Option<SyncSender>` None（最少改动）。建议方案 A（最小补丁）。

##### [低] handler 文档注释 stale
- 文件：`src-tauri/src/network/handlers/clipboard.rs:38`
- 现象：函数 doc-comment 第 7 步仍写 `"发到 clipboard_apply_tx（TODO PR-6 接 arboard；当前 None 占位 + tracing::info）"`，但 clipboard_apply_tx 已不是 Option，且 PR-6a 已真接。Sync Copy SDLC 强调"文档同步代码（v4-1）"，遗漏属低危但需补。
- 建议修法：把第 7 步更新为 `"发到 state.clipboard_apply_tx.try_send（PR-6 真接 arboard 线程；非阻塞，channel 满或 watcher 退出时 warn 不返错）"`。

##### [低] clipboard 单测全为"逻辑 simulate"未覆盖真函数路径
- 文件：`src-tauri/src/app/clipboard.rs:415-628`
- 现象：watcher_skips_empty / watcher_skips_oversize / watcher_skips_unchanged / watcher_broadcasts_on_change / apply_writes_local_no_loop 5 个核心单测都**不调** `poll_text_clipboard` / `apply_text_to_clipboard`，而是把分支逻辑在测试体内重写一遍（"simulate: if text.is_empty() { return; }"）。后果：若 implementer 在真函数中写错（如忘记更新 last_hash），单测仍 pass — 单测沦为"复读代码注释"。
- 建议修法：apply_text_to_clipboard 拆出纯 hash 计算 + 状态更新的内部 helper（不依赖 `&mut arboard::Clipboard`），让单测真调该 helper；或加 trait `ClipboardOps { fn set_text / fn get_text }` 让单测注入 mock。当前实现单测**编译通过即认为绿**，覆盖度名义高、信号低。
- 不阻塞 PR-6a，但建议挂 PR-6b 重构。

#### 8.8.3 结论

CHANGES_REQUESTED → 主窗口编排闭环。

- [严重] shutdown 100ms 软上限 必须修（ADR-010 契约级违反，新策略仍走"派 backend-impl 静默落 → 静默通过"，**不需要回报用户**）
- [中等] broadcast_rx drop warn 噪音 顺手降级（同补丁）
- 2 条 [低] 可挂 PR-6b（文档注释 stale + 单测真覆盖）

环路防止 5 个 AC 真闭环（spec 第 4 节 AC #3/#4/#5/#6/#7 + v0 lessons learned 第 4.2 节都对齐）；4 nit 全清；不引入新违反；cargo 三件套 green — 整体方向正确，仅 lifecycle.shutdown timeout 实现有 bug。

#### 8.8.4 过度工程自查

本轮 review 报告共 ~75 行（含本节）；问题列 4 条（1 严重 + 1 中等 + 2 低），未超 12 条阈；单条最长 5 行（严重 #1 含建议修法），未超 15 行；ADR/spec 引用 ~10 处，未超 20 处；todo 清单 4 条，未超 8 条阈。**5% 可省略**（[低] #4 单测 simulate 议题可挂 PR-6b 而非本轮 report）。

### 8.9 Code Review v4 — PR-6a' 4 补丁验证 (2026-05-10 · commit 994e16a)

**结论**：APPROVED — 4 补丁全代码层闭环；99/99 tests pass；release build watcher_shutdown 实测 0.01s ≪ 100ms 软上限；0 TODO/FIXME 残留；不引入新违反。

#### 8.9.1 4 补丁真闭环验证

- [严重 #1] 100ms 真实现：✅
  - `ClipboardWatcher` 新增 `done_rx: Option<mpsc::Receiver<()>>` 字段（clipboard.rs:82）；`start()` 建 `(done_tx, done_rx) = mpsc::channel()`（clipboard.rs:104）；thread 闭包尾部 `let _ = done_tx.send(())`（clipboard.rs:111，在 `clipboard_thread_main` 之后，正常退出 + arboard init 失败 early return 路径均可达 — done_tx 在闭包 move-in 后随线程退出 drop，Disconnected 路径合理处理）
  - `shutdown()` 重写（clipboard.rs:132-177）：cancel.store → `recv_timeout(100ms)`；Ok → join handle 回收；Timeout → tracing::warn 落盘（含 deadline_ms=100 字段，ADR-010 第 3.7 节配套约束）+ detach；Disconnected → 视为已退出 + join handle 回收
  - 单测 `watcher_shutdown_under_100ms`（clipboard.rs:655-681）真断言 `elapsed ≤ 100ms`；headless CI arboard init 失败仍可 send done_tx → shutdown 仍 ≤ 100ms（CI 友好）；release build 实测 0.01s（远低于 100ms 阈，无 flaky 风险）
- [中 #2] noise 降级：✅
  - clipboard.rs:368 broadcast_tx try_send 失败 log `tracing::warn!` → `tracing::trace!`；注释明文 "PR-7 落地前 broadcast_rx 未消费，预期 Disconnected"（行 365-366 + 371）
  - lifecycle.rs:213-217 broadcast_rx drop 处加同步注释（"PR-7 真接收侧落地后替换此 channel"），与 clipboard.rs 互引一致
- [低 #3] doc-comment：✅
  - handlers/clipboard.rs:38 第 7 步 doc 从"TODO PR-6 接 arboard；当前 None 占位 + tracing::info"→"state.clipboard_apply_tx.try_send（PR-6 真接 arboard 线程；非阻塞，channel 满或 watcher 退出时 warn 不返错）"，与现网业务对齐
- [低 #4] 单测覆盖 AC：✅
  - `clipboard_handler_rejects_invalid_aad`（clipboard.rs:690-725）：真调 `AesGcmSealer::decrypt` + `build_aad`；构造正确加密 → 用 wrong origin / wrong seq 解密 → 断言 `is_err()`；正确 AAD 解密 → 断言 `is_ok()`；覆盖 AC #6"解密失败 → 拒绝"
  - `clipboard_thread_retries_on_arboard_busy`（clipboard.rs:735-794）：诚实声明 arboard 不可 mock；测 retry skip 语义（两次失败 → 无 broadcast）+ retry 成功语义（last_hash 更新 + broadcast 触发），逻辑覆盖 AC #7。**遗留**：仍是 simulate 而非真函数路径（PR-6a review 第 8.8.2 节 [低] #4 议题）— 未阻塞本 PR，仍挂 PR-6b 重构

#### 8.9.2 不引入新违反验证

- `cargo clippy --all-targets -- -D warnings` 0 warning（复跑）
- `cargo test --lib` 99/99 pass（96 + 3 新单测）
- `cargo test --lib --release watcher_shutdown_under_100ms` 单跑 0.01s pass — release build 仍远低于 100ms（无 flaky 风险）
- `git grep "TODO\|FIXME" src/app/clipboard.rs` 0 命中
- `git show 994e16a --stat | grep PLAN.md` 0 真文件命中（仅 commit message 字面引用，v2-9 守住）
- detach 残留风险：headless / 真 arboard 死锁时 thread leak 是 ADR-010 第 4.2 节"OS 进程退出清理"接受面；shutdown 主流程仍 ≤ 100ms 返回，调用方 quit_app 不阻塞
- panic 路径：thread 闭包 panic 不发 done_tx → shutdown 走 Timeout → tracing::warn + detach；语义与 ADR-010 第 3.3 节 step 4 "100ms 软上限 detach" 一致
- recv_timeout(100ms) 在 worker cancel 后语义：cancel 是 Relaxed atomic 标志，thread loop 顶端检查后立即 break，远快于 100ms tick；Disconnected 分支处理 done_tx 在 send 前 drop 的边界（虽罕见但显式覆盖）

#### 8.9.3 结论

**APPROVED** → PR-6a 整体闭环（PR-6a' 4 补丁 1:1 真闭环 + 0 新违反）。

- PR-6a review 第 8.8.2 节 [严重] #1 ADR-010 第 3.3 节 step 4 100ms 软上限契约级违反 — **真修**
- PR-6a review [中等] / 2 条 [低] 全闭环
- backend MVP 里程碑：PR-1~5b（前置）+ PR-6a + PR-6a' = clipboard 真业务接入 + lifecycle shutdown 契约闭环；PR-6b（heartbeat worker）可推进
- 唯一遗留：[低] #4 单测 simulate 议题挂 PR-6b 重构 ClipboardOps trait（架构师域）；不阻塞

**过度工程自查**：本段 ~45 行（略超 40 行预算 5 行，因 [严重] #1 验证细节多列了 3 行 location + 1 行单测 CI 友好性 + 1 行 detach 接受面引用 — 都是契约级证据，保留）。
