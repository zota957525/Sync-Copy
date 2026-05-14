---
status: SPEC_REVIEWED
owner: product-strategist
related_adrs: [ADR-003]
related_specs: [00-product-overview, group-discovery, group-approval, clipboard-text-sync, e2e-encryption]
created: 2026-05-06
updated: 2026-05-08
revised: 2026-05-08 — ADR-003 第 3.2 节 决议复用 /clipboard 端点 + body 加 is_snapshot flag（不开 /clipboard/snapshot 新端点）；触发节点 = 握手入口节点（A 单点 push 给 C，避免 N 倍流量）
priority: P1
---

# clipboard-snapshot-sync — 新成员加入时自动同步当前剪切板内容（入组即可粘贴）

## 1. 问题（为什么做）

v0 的入组体验有一个**割裂**：A 与 B 已经组队 30 分钟，A 半小时前 `Cmd+C` 了一段命令；现在用户走到 C 前，C 加入小组 → 审批通过 → C 浮窗变绿 `小组 · 3 台` → 用户在 C 上 `Ctrl+V` → 粘出来的是 C 自己 30 分钟前的剪切板内容（与组无关），不是用户想要的那段命令。**用户必须回到 A 再 `Cmd+C` 一次**才能让组内剪切板"刷新"到 C，整个流程从"无缝入组"退化为"入组后还要起身走回去"。

这违反了 00 总览 第 1 节 的核心承诺——"两台机器像一台机器"。"像一台"意味着新设备一接入就拥有"组的当前态"，而不是"未来事件的订阅者"。

本 feature 定义入组瞬间的"snapshot 同步"：当 C 完成握手 + 审批通过、加入 peers 表后，组内某一台已加入设备**自动把组的最新剪切板内容**（暂定最新 1 条）发给 C，让 C 进入"立刻有东西可粘"的状态——而不需要用户走回 A 再敲一次 `Cmd+C`。

本 feature 是 P1 完整体验改进，依赖整条 P0 链路全部就位（group-discovery + group-approval + clipboard-text-sync + e2e-encryption）才有意义——它不属于 MVP 闭环，但是把"入组即用"这一隐式承诺补完的关键一刀。

## 2. 用户故事

- As a new device just approved into a group, I want my floating window's history to instantly hold the most recent clipboard item from the group, so that I can see what's "already there" without asking another device to re-copy.
- As a user adding device C to an established A+B group, I do not want to walk back to A to press Cmd+C again just so C has something useful, so that onboarding is one approval click rather than "approval click + walk back + re-copy".
- As a user with a sensitive item already in C's local clipboard before joining, I do not want the snapshot to overwrite my system clipboard silently, so that joining a group does not feel like an invasive operation on what I just copied locally.
- As an operator, when no one in the group has copied anything yet, I want C's join to succeed normally with an empty history, not error out, so that "nothing to snapshot" is a valid state.

## 3. 范围

**in scope**：

- **触发时机**：握手成功 + 审批通过 + ECDH 完成 + 双方 `peers` / `peer_keys` 已写入之后，由"已在组内的某台设备"主动 push 一条 snapshot 给新成员
  - 在 v0 `handle_handshake` 的 `Some(true)` 分支末尾（或对应位置的 client 端 `join_group` 拿到 200 OK 之后），spawn 一个 fire-and-forget 任务执行 snapshot 推送
  - **方向**暂定 push（已在组的 A 推给新加入的 C），但具体是 push 还是 pull 待 ADR 决议（见 第 7 节 [P0] [架构师]）
- **同步数据范围**：暂定**只发组内最新 1 条剪切板内容**（最近 push 进任意一台 history 的 head 项）
  - 上限锁定 ≤ 1 条留给 ADR 是否扩展到 N 条评估（见 第 7 节 [P0] [架构师]）
  - 媒体类型：本 P1 阶段**仅 text**（继承 `clipboard-text-sync` 的协议）；image / file 类型 snapshot 不在本 spec 范围
- **传输协议**：复用 `e2e-encryption` 现有的 AES-256-GCM 加密通道与 `ClipboardReq` DTO 骨架，不引入新加密方案
  - 是否复用 `/clipboard` 端点（加 `is_snapshot: bool` 标记）还是开新端点 `/clipboard/snapshot` 待 ADR 决议（见 第 7 节 [P0] [架构师]）
  - AAD 是否绑入 "snapshot" kind 防止把 snapshot 报文重放成普通 clipboard 投放：复用 `e2e-encryption` 第 7 节 [P0] [安全] 的 AAD 决议路径，不在本 spec 单独开新决议
- **接收侧行为**（C 上的 handler）：
  - 校验 origin_device_id 在 peers 表（与 `/clipboard` 同样规则）→ 否则 403
  - 解密（与 `/clipboard` 同协议）→ 失败丢弃 + log
  - **写入 history**（`history.push_text(plaintext, Source::Remote{device_name})`）→ emit `history-updated`
  - **不写入系统剪切板**：snapshot 进 history 但不调 `ClipboardCmd::SetTextSuppress`，避免突袭式覆盖 C 加入前用户本机的剪切板内容（见 第 4 节 验收 #3）；用户若想用，单击历史条目复用——这正是 `history-list` 第 3 节 已定义的入口
- **去重 / 防重放保护**：
  - C 端在收到 snapshot 时检查"我是否已经处理过同 origin + seq"（继承 `seen_seq_and_update` 机制）→ 已见即静默 200 OK
  - C 端检查"我自己 history 是否已含同 content_hash"→ 已含即静默 200 OK（避免 push 来的 snapshot 与稍后正常 broadcast 重复入栈）
- **触发节点选择**：哪台设备负责发 snapshot 给 C？暂定**与 C 完成握手的"入口节点"** A 单点负责（A 是握手 200 OK 的发起方，逻辑上 A 知道"C 已成功加入"的最早时刻；其它 peer 通过 gossip 被动得知，等它们意识到时 A 早已 push 完）
  - 这一选择避免 N 个组员各自向 C push 同一份 snapshot 引起的 N 倍流量与去重压力
  - 决议属架构师 ADR（见 第 7 节 [P0] [架构师]）
- **空 history 兜底**：A 自己的 history head 为空（A 还没复制过任何东西，且没收到过其它 peer 的内容）时，A 直接不发 snapshot；C 加入后浮窗 history 仍为空，状态显示 `小组 · N 台` 正常
- **重复加入 / 重新握手**：已知 peer 重新握手（v0 `handle_handshake` 的 `known` 短路分支）不触发 snapshot——已知 peer 自己 history 已有内容，不需要重灌
- **审批被拒 / 超时不触发**：snapshot 仅在 `Some(true)` 分支或 client 端拿到 200 OK 之后才触发；403 / 408 路径不触发

**out of scope**（v2 这个 feature 不做）：

- **全量 history 同步**：不发最新 N>1 条（避免 N 与 50 上限的争议、避免一次大流量打到刚加入的 C；留 v3 评估）
- **图片 / 文件 snapshot**：本 P1 仅 text；image snapshot 涉及大流量入组瞬间打过去，文件 snapshot 涉及保存路径协商，复杂度都不属本 feature
- **双向 snapshot**：C 加入时 C 本地 history 已有内容，不反向 push 给 A、B；C 加入前的本地 history 留在 C 本地（这是"加入小组"的语义而非"双向合并历史"）
- **新成员的剪切板写入**（即把 snapshot 内容写到 C 的系统剪切板，让 C 的 `Ctrl+V` 直接粘出 snapshot）：违反"加入小组不应突袭系统剪切板"的安全 / 隐私底线（见 第 7 节 [P0] [安全]）；snapshot 仅进 history，用户单击复用
- **持久化 snapshot**：00 总览 第 3 节 已锁定历史不持久化；snapshot 与普通 history 同等待遇，重启即清
- **跨 peer 一致性保证**：A 推给 C 的"组内最新一条"在 push 那一刻成立；A 推完后立刻 B 复制了新东西并广播给 C，可能在 C 看到顺序为 [B 新, A snapshot]——这是正确的时序展示，不需要额外保证

## 4. 验收标准（Definition of Done）

- [ ] A、B 已 `小组 · 2 台`。在 A 上 `Cmd+C` `"hello"` → 1 分钟（A 与 B 期间无其它操作）后 C 加入并被审批通过 → C 浮窗在变绿 `小组 · 3 台` 之后**1 秒内**：history 顶部出现一条 `hello`，标 `来自 <A 的设备名> · 刚刚`
- [ ] 同上场景 C 加入瞬间：C 的**系统剪切板内容不变**（保持 C 加入前的内容，例如 C 在加入前剪切板里有 `local-stuff`，加入后立刻 `Ctrl+V` 粘出的仍是 `local-stuff`，不是 `hello`）
- [ ] A、B 已 `小组 · 2 台` 但**A 与 B 都从未复制过任何内容**（双方 history 为空）→ C 加入并被审批通过 → C 浮窗变绿，C history 仍为空，**不报错**、不弹任何提示
- [ ] A、B 已 `小组 · 2 台`，A 复制 `"hello"` 后 1 秒内 C 加入 → C history 应**只有一条** `hello`（去重生效，A 的"snapshot push" 与 A 的"正常 broadcast"对 C 来说不重复入栈）
- [ ] A、B、C 已 `小组 · 3 台`，C 因网络抖动短暂离线后 5 秒内重新握手回来（已知 peer 路径）→ C history 中**不出现重复的 snapshot 条目**（重新握手不触发 snapshot）
- [ ] C 加入审批被**拒绝**（403）或 **超时**（408）→ C 的 history 不出现任何 snapshot 条目（拒绝路径不触发 snapshot push）
- [ ] A push snapshot 时 C 已经退出 / 网络不可达 → A 端日志显示一条 warn `snapshot push to <C> failed` 但 A 自己不崩、A 与 B 之间正常剪切板同步不受影响（fire-and-forget）
- [ ] 在 A 上 history 中最新一条是**图片**（image_png kind）的场景下 → C 加入 → A 不发 snapshot（因本 P1 仅 text）；C history 为空。后续 P 阶段加入 image snapshot 时本验收标准会更新

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的

`legacy-prototype` 分支**没有此功能**。`src-tauri/src/network/server.rs::handle_handshake` 在 `Some(true)`（审批通过）分支的最后只做：

1. 派生 AES key、写入 `peer_keys`
2. 写入 `peers` 表
3. 写入 `approved_device_ids`（白名单）
4. broadcast trust gossip
5. 返回 `HandshakeResp { peers, pubkey, ... }`

`src-tauri/src/commands.rs::join_group`（client 端）在拿到 200 OK 后做：

1. 写入本机 `peers` / `peer_keys`
2. spawn gossip handshakes（向 resp.peers 中的未知 peer 发起握手扩展 mesh）

**两个端点路径都没有任何"把当前剪切板内容推过来"的逻辑**。新加入的 C 唯一拿到内容的途径是：等组内某成员**未来某次新的 Cmd+C** 触发 `network::client::broadcast_text` 才有内容到达。

### 5.2 v0 暴露的具体坑

- **入组瞬间 history 空是体验割裂**：用户原话"新连上的设备，自动同步当前复制功能里存在的内容"——v0 没做，用户每次加入新设备都要回到老设备再敲一次 `Cmd+C`
- **没有任何 snapshot 协议设计**：v0 的 `/clipboard` 端点只服务"事件型推送"（A 复制 → 立刻广播），没有"状态型同步"（C 加入 → 拉当前态）的概念。v2 引入这个语义需要在协议层做新选择
- **gossip mesh 扩展时同样有此问题**：C 通过 A 加入后再 gossip 接到 B，B 也不会主动给 C 推任何东西。v0 隐式假设"新成员靠未来事件累积态"，对短会话用户不友好
- **没有"组态"概念**：v0 的 history 是每台机器独立维护的本地态，"组里现在最新一条是什么"不是一个一阶概念。v2 引入 snapshot 后，"snapshot 取自哪台 peer 的 head"这一选择需要 ADR 明确（暂选握手入口节点，见 第 7 节）
- **去重机制可能漏 snapshot 与 broadcast 的赛跑**：A 复制 → 同时（1）broadcast 给 C（2）触发 snapshot push 给 C，两条路径都到达 C 后 history 可能进两次。v0 不存在此 bug 因为根本没有 snapshot；v2 必须在 spec 阶段就把"去重"约束写进 第 3 节 in scope（已写）+ 验收（已写 #4）

### 5.3 v2 应继承

- `e2e-encryption` 的 AES-256-GCM 加密通道与 `ClipboardReq` DTO 骨架（snapshot 走同一加密路径）
- `seen_seq_and_update` 的 origin + seq 去重机制（snapshot 与正常 clipboard 共用）
- `peers` + `peer_keys` 表的写入时机（snapshot 必须在这之后触发）
- `history.push_text` + `history-updated` 事件链（snapshot 接收侧仍走这条路径）
- `Source::Remote{device_name}` 标识（snapshot 在 history 里也是 remote，与正常 broadcast 视觉一致）

### 5.4 v2 应挑战

- **触发时机**：握手 200 OK 后、ECDH 完成后、peers 写入后——具体放在 `handle_handshake` 还是 client `join_group` 拿到 resp 之后？决定 push / pull 方向、决定哪一端是触发者（见 第 7 节 [P0] [架构师]）
- **数据量**：1 条 / 5 条 / 50 条全部？影响协议 payload 大小、影响 C 加入瞬间的网络流量、影响隐私（snapshot 内容暴露给"刚通过审批的新设备"是否需要更细的访问粒度）—— 见 第 7 节 [P0] [架构师]
- **方向**：A push 给 C / C 主动 pull from A？前者简单 + 一次 RTT；后者更"显式"，C 主动行为减少"自动同步"的隐式感（见 第 7 节 [P0] [架构师]）
- **端点选择**：复用 `/clipboard` 加 `is_snapshot` flag / 开新端点 `/clipboard/snapshot`？前者协议字段少、复用代码多；后者职责更清、便于 AAD 绑入 "snapshot" kind 防止重放为普通 clipboard 投放（见 第 7 节 [P0] [架构师]）
- **触发节点**：仅"入口节点"（与 C 直接握手成功的那台）单点 push / 全员各自 push（去重靠 content_hash + seq）？前者 1 倍流量但单点失败 = 没人发；后者 N 倍流量但天然冗余（见 第 7 节 [P0] [架构师]）
- **接收侧不写系统剪切板**的不变式必须明文写入 ADR：避免后续被"那要不要也写一下系统剪切板让用户更爽"的修改打破

## 6. UX 设计

> 本节由 ux-designer 在 2026-05-06 填写（P2-3.b）。颜色/字号/字体/间距等视觉语言定义见 floating-window.md 第 6.5 节，本节不重复定义。本 feature 无独立视图；UX 集中在三个联动时刻：入组成功瞬间、snapshot 写入 history 后、snapshot 失败静默处理。

### 6.1 信息架构

本 feature 不新增任何 UI 面板，它的 UX 体现在**已有 UI 元素的状态变化序列**：

1. 顶部状态栏（状态点 + 文字）：入组时从"等待审批"变为 `小组 · N 台`（已由 group-approval spec 定义）
2. history 列表顶部：snapshot 写入后出现新条目（与普通 remote 条目视觉一致）
3. toast 提示（入组成功后 1 秒内，叠加在 history 列表上方）：`"已同步 1 条记录"`（轻量感知信号）
4. 无内容时（A history 为空）：不出现 toast，history 保持空态（history-list 第 6.6 节 空态规则）

### 6.2 关键流程图（文字版）

主路径（C 加入，history 有内容）：

1. C 发起握手 → A 与 B 收到审批弹框 → 某台同意（group-approval 第 6.2 节 已定义）
2. 审批通过 → A 的 handler 把 snapshot 包发给 C（fire-and-forget）
3. C 端收到 snapshot → 解密 → 写入 history（`Source::Remote{A的设备名}`）→ emit `history-updated`
4. C 浮窗顶部状态栏同时：状态点变绿，文字变 `小组 · N 台`
5. C history 列表顶部出现 snapshot 条目
6. C 浮窗在 history 区域上方出现 toast：`"已同步 1 条记录"` → 2.5s 后自消失

主路径（C 加入，history 为空）：

1. 审批通过 → A 检查自己 history head = nil → 不发 snapshot
2. C 浮窗状态栏变绿 `小组 · N 台`，history 仍为空态（空态文案：见 history-list 第 6.6 节）
3. 不出现 toast（无可同步内容，不应发出空信号）

主路径（网络抖动后 C 重连——已知 peer 路径）：

1. C 短暂离线 → 重新握手（已知 peer 路径）→ 不触发 snapshot
2. C 的 history 内容保持原样（重连无变化，无 toast）
3. 状态栏闪烁后恢复正常颜色——这是 group-approval / group-discovery 的既有反馈，snapshot 不参与

异常路径：

- A 发 snapshot 时 C 网络不可达：A 日志一条 warn，A 与 B 正常运行，不显示任何 UI 反馈（fire-and-forget 语义）
- C 收到 snapshot 但解密失败：静默丢弃 + 记录日志（diagnostic-logging），C 的 history 保持空态，无 toast
- C 已通过去重检测拒绝重复 snapshot：静默 200 OK，history 不重复，无 toast（已有内容不再叠加）

### 6.3 ASCII wireframe（必填）

C 入组成功瞬间（审批通过后约 1 秒内，C 浮窗状态）：

```
┌────────────────────────────────┐
│  ● 小组 · 3 台  [加入]   − ⚙  │← 状态点变绿，N=3，已连接状态
├────────────────────────────────┤
│                                │
│  ┌──── 已同步 1 条记录 ──────┐  │← toast，见下方详细 wireframe
│  └────────────────────────── ┘  │
│                                │
│ ┌────────────────────────── ✕ ┐ │
│ │ hello world                  │ │← snapshot 条目，与普通 remote 条目完全一致
│ │ 来自 工作 Mac · 刚刚         │ │← 12px #9ca3af meta 行
│ └────────────────────────────  ┘ │
│                                │
│   [更早的本机历史，若有]        │
│                                │
├────────────────────────────────┤
│  192.168.1.77:5858    我的笔记本 │
├────────────────────────────────┤
│   Made with Claude · by Tao   │
└────────────────────────────────┘
```

toast 详细结构（叠加在 history 列表区域顶部，不影响列表布局）：

```
┌────────────────────────────────┐
│  [历史列表区域]                 │
│                                │
│  ┌──────────────────────────┐  │← toast，绝对定位，浮在列表上方
│  │   ✓  已同步 1 条记录      │  │← 12px #22c55e 成功绿，左对齐
│  └──────────────────────────┘  │← 背景 rgba(34,197,94,0.12)，8px 圆角
│                                │   水平居中，2.5s 后 opacity 淡出消失
```

A history 为空时 C 入组后的状态（无 toast，无 snapshot 条目）：

```
┌────────────────────────────────┐
│  ● 小组 · 3 台  [加入]   − ⚙  │
├────────────────────────────────┤
│                                │
│                                │
│       还没有同步过              │← 空态，history-list 第 6.6 节 定义
│       复制一段文本试试           │
│                                │
│                                │
├────────────────────────────────┤
│  192.168.1.77:5858    我的笔记本 │
└────────────────────────────────┘
```

### 6.4 交互细节

点击区域划分：

- toast：不可点击（纯感知信号，无操作入口）
- snapshot 写入的 history 条目：单击行为与普通 remote 条目完全一致（单击复制，见 history-list 第 6.4 节）
- toast 不提供"×"关闭按钮（2.5s 短暂显示，手动关闭意义不大；保持视觉简洁）

鼠标悬停反馈：

- toast：无 hover 状态（不可交互）
- snapshot 条目：完全继承 history-list 第 6.4 节 的行悬停规则（背景微亮 + ✕ 按钮出现）

snapshot 条目视觉策略（关键决策）：

- 选择与普通 Remote 条目视觉完全一致，不加特殊标记（如 `snapshot` 角标、入组图标）
- 理由 1：v0 教训之一是视觉负担过重（00 总览 第 5.4 节）；snapshot 条目在大多数情况下只有 1 条，用时间戳和来源标签（"来自 X · 刚刚"）足以让用户判断它是刚入组时灌进来的
- 理由 2：从用户心智模型看，snapshot 与"实时收到的 remote 条目"本质相同——都是另一台机器的剪切板内容，对 C 来说它就是"A 上最新的内容"；在 UI 里刻意区分反而引入困惑（"snapshot 是什么"）
- 理由 3：代码层 snapshot 条目已经用 `Source::Remote{device_name}` 标记，与普通 broadcast 一致，不需要前端单独处理

toast 出现条件（关键决策）：

- 仅当 C 入组时 history 被实际写入了 1 条 snapshot 内容时才显示 toast
- history 为空（A 没有内容）：不显示 toast（无可同步内容，空信号即噪声）
- 解密失败静默丢弃：不显示 toast（静默失败，不惊扰用户）
- 这意味着 toast 是"入组确认 + 内容已就绪"的双重信号，用户看到 toast 就知道历史里有东西可用

toast 位置与时长：

- 位置：history 列表区域内顶部，绝对定位（不推移 history 条目），水平居中
- 时长：2.5 秒后 opacity 淡出消失（0.3s 过渡），不做入场动画（直接出现，淡出消失）
- 同时只有一个 toast（入组事件不会并发）

### 6.5 状态与颜色字典

详见 floating-window.md 第 6.5 节。本 feature 特有颜色：

| 元素 | 颜色 | 说明 |
|---|---|---|
| toast 背景 | `rgba(34,197,94,0.12)` | 成功绿 极浅底，与 history-list 第 6.5 节 "已复制"chip 同色系 |
| toast 文字 | `#22c55e` | 成功绿，12px |
| toast 勾号 `✓` | `#22c55e` | 成功绿，与文字同色 |
| snapshot 条目（所有元素）| 与 history-list 第 6.5 节 remote 条目完全一致 | 不重复定义 |

状态变化链：

| 阶段 | 顶部状态点颜色 | 顶部文字 | 触发条件 |
|---|---|---|---|
| 握手进行中 / 等待审批 | `#3b82f6` 蓝 | `等待对方同意…` | group-approval 第 6.5 节 已定义 |
| 审批通过，snapshot 写入完成 | `#22c55e` 绿 | `小组 · N 台` | 状态点同时变绿，toast 随后出现 |
| 审批通过，A history 为空 | `#22c55e` 绿 | `小组 · N 台` | 同上，无 toast |

### 6.6 边界与例外

- C 入组后，A 同时广播了新内容（broadcast 与 snapshot push 几乎同时到达 C）：C 的 history 通过 content_hash 去重机制处理，顶部条目最终只有一条。toast 是入组事件的一次性信号，不因广播再次触发
- C 同时被多台 peer 先后发来 snapshot（理论上不应发生，因为规定入口节点单点发）：content_hash 去重兜底，C 的 history 不出现重复条目；toast 只在第一条写入时触发（由 `history-updated` 事件驱动，去重后只 emit 一次）
- 浮窗在 toast 显示期间被用户点击 ⚙ 切换到 settings view：toast 随 main view 一起隐藏，不需要显式清除
- 浮窗在 toast 显示期间被折叠为悬浮球：toast 随浮窗隐藏，不需要显示在悬浮球上
- snapshot 内容为空字符串：history-list 层面不写入（空内容无意义），不显示 toast
- snapshot 解密失败（密钥不匹配等）：静默丢弃，不显示 toast，diagnostic-logging 记录一条 warn；C 的 history 保持空态（或已有内容不变）
- 实测可能暴露的问题：toast 出现时间点依赖后端处理完成时机（snapshot push 延迟可能导致 toast 在状态栏变绿后 2-3 秒才出现）；建议实测验证端到端延迟，必要时可把 toast 的触发时机从"snapshot 写入完成"改为"状态栏变绿同时预触发 + 1.5s 内若无 snapshot 写入则不显示"（属前端实现决策，不在此强制）

### 6.7 给前端工程师的实现提示（可选）

- toast 建议用 `position: absolute` 叠在 history 列表区域顶部（`top: 8px`，水平居中），由 `$state` 变量 `showSnapshotToast: boolean` 控制显隐；设置 `setTimeout(2500ms)` 后置为 false，再以 CSS `opacity` transition 淡出（不要直接隐藏，防止突变）
- toast 不需要队列机制（snapshot 只在入组时触发一次，不会并发多个 toast）
- snapshot 条目与普通 remote 条目共用同一个 Svelte 组件（HistoryItem），无需额外分支；`source.kind = "remote"` 已满足渲染条件

### 6.8 给主窗口和 PM 的反馈（UX 开放问题回应）

**问题（来自 spec 第 7 节 [P1] [UX]）：snapshot 接收后的用户感知：是否需要 toast 提示"已同步组内最新内容"，还是与普通 remote 条目视觉一致让用户自行从时间戳判断**

结论：两者都要，但形式轻量。

选择在 C 浮窗 history 区域内顶部显示一次性 toast（`"已同步 1 条记录"`，2.5s 自消失），且 snapshot 条目本身与普通 remote 条目视觉完全一致。

理由：

- toast 是入组"成功确认"信号的补充——用户看到状态栏变绿已知"连上了"，但不知道"history 里有没有内容"；toast 把这两个信息结合（"连上了 + 有东西"），避免用户自己滚动历史才能发现
- toast 轻量（非系统通知，不打断其它应用），2.5s 消失，不持久干扰
- snapshot 条目不加特殊标记，避免引入"snapshot 是什么"的认知负担；用"来自 X · 刚刚"的 meta 行足以暗示时序

**关于 history-list 第 6 节 视觉一致性自检：**

snapshot 条目完全复用 history-list 第 6.3 节 remote 条目的 wireframe 结构（文字行 + meta 行 + hover ✕），不引入任何新视觉元素；toast 使用 `#22c55e` 成功绿，与 history-list 第 6.5 节"已复制"chip 颜色系一致。

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 6 条] [P1 2 条] [P2 1 条]

- [P0] [架构师] 同步条数：1 / 5 / 50？决议直接修改 第 3 节 in scope。trade-off：1 条最简单 + 隐私范围最小 + 但用户可能"差一条"；50 条体验最完整 + 但隐私范围最大 + 入组瞬间流量大；5 条折中
- [P0] [架构师] 触发方向：push（A 主动发给 C）/ pull（C 拿到 200 OK 后调 A 的 `/snapshot` 接口取）？影响协议端点设计与"哪一端是发起方"的语义
- [P0] [架构师] 端点选择：复用 `/clipboard` 加 `is_snapshot: bool` 字段 / 新开 `/clipboard/snapshot` 端点？前者代码复用率高，后者职责洁净 + AAD 绑入 "snapshot" kind 更直观（见下条 [安全]）
- [P0] [架构师] 触发节点：仅"握手入口节点"单点 push（v2 暂定，避免 N 倍流量）/ 全员各自 push（去重靠 content_hash + seq）？暂定单点 + ADR 决议是否需要兜底（如入口节点 push 失败时其它 peer 接力）
- [P0] [安全] AAD 绑入 kind 防重放：snapshot 报文若用与普通 clipboard 同一 AES key + 同一 AAD 设计，攻击者抓 snapshot 报文可能在另一时刻重放成普通 clipboard 投放（虽然 seq dedupe 防重复 + LAN 信任假设，仍是密码学层缺陷）。需把 "snapshot" 字符串绑入 AAD（前提是 `e2e-encryption` 第 7 节 [P0] [安全] 的 AAD 决议路径选了"绑 kind"方向）
- [P0] [安全] 谁有资格收到 snapshot：仅审批通过的新成员（已加 peers + 写 peer_keys）。这条不变式必须 ADR 明文 —— 任何"未审批通过但 device_id 在某种半状态" 的成员不应触发 snapshot push
- [P1] [架构师] 单点 push 节点失败兜底：A 是入口节点但 A push 给 C 失败（网络抖动）—— 其它已加入 peer 是否在某个延迟（如 5s）后接力 push？还是"丢就丢，C 等下次新事件"？
- [P1] [UX] snapshot 接收后的用户感知：是否需要 toast 提示"已同步组内最新内容"，还是与普通 remote 条目视觉一致让用户自行从时间戳判断（"刚刚 · 来自 X" 已经足够暗示）
- [P2] [架构师] 未来扩展到 image / file snapshot 的协议预留：本 P1 锁定仅 text，但端点 / 字段设计是否预留 kind 字段以便 P 阶段无破坏性扩展？还是当时再演进协议版本

## 8. Review 段（占位）

> code-reviewer / tech-architect / security-reviewer 后续填写。本 feature 涉及加密通道复用与新协议字段，必须经 security-reviewer ACK（CLAUDE.md 第 9 节）；触发时机与端点选型需 tech-architect ACK。
