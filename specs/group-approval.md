---
status: SPEC_REVIEWED
owner: product-strategist
related_adrs: [ADR-003]
related_specs: [00-product-overview, group-discovery, e2e-encryption]
created: 2026-05-06
updated: 2026-05-08
revised: 2026-05-08 — ADR-003 第 3.5 节 quit_app 唯一退出路径 + 第 3.6 节 状态码 408 NetworkError 映射；DoS 限流 + device_name 字符集 留 ADR-008 安全审阅
priority: P0
---

# group-approval — 分布式审批弹框 + first-responder-wins + 跨 peer dismiss

## 1. 问题（为什么做）

Sync Copy 的"身份认证"模型是**审批弹框背后的人类用户**，不是密码、证书或 PKI（00 总览 第 5.1 节 已锁死）。设计挑战：当 C 申请加入已有 A+B 小组时，**不能强制**用户必须坐在 A 旁——任意一台已经加入的设备的用户点同意都应生效。这就要求审批是分布式的：

1. C 向 A 发起握手 → A 本机弹框 + A 把请求 forward 给 B 也弹框
2. **任一**设备先按"同意"或"拒绝"（first-responder-wins）→ 决定回流到 A
3. A 把决定 broadcast dismiss 给所有人，关掉所有弹框
4. A 返回握手响应给 C

这套范式在 v0 经 4 次迭代才稳定（00 总览 第 5.2.6 节），是 P0 必须可用、且必须把"分布式状态收敛"的不变式写进 spec 的核心 feature。

## 2. 用户故事

- As an A user already in a group with B, when C tries to join via either A or B, I want a single approval popup on **all** online devices simultaneously, so that whoever is currently at a computer can grant access.
- As any user pressing "Approve" on the popup, I want the popup on every other device to close immediately so they don't see a stale request, and the joining device to be granted in within ~1 second.
- As a user, I want a 30-second countdown on the popup, so that abandoned requests don't pile up indefinitely; if no one responds, the joining device gets a clean timeout error.

## 3. 范围

**in scope**：
- A 收到 C 的握手 → 生成 `request_id` (UUID) → 插入 `pending_approvals: HashMap<request_id, oneshot::Sender<bool>>`
- A 本机 emit `handshake-pending` 事件 → 浮窗弹审批框（毛玻璃蒙层 + 中心卡片 + 申请方设备名 + 30s 倒计时 + `[拒绝][同意]` 双按钮）
- A 并行 broadcast `/peers/approval/forward` 给所有已知 peer → 各 peer 把 `request_id` 插入 `forwarded_approvals: HashMap<request_id, ForwardedApprovalInfo>` + emit `handshake-pending` 事件 → 自己也弹审批框
- 任一节点（含 A 本机或某 peer）的用户点决定：
  - 本机即 A：直接 `tx.send(decision)` 喂给 `pending_approvals[request_id]`
  - 本机非 A：POST `/peers/approval/decide` 把决定送给 A，A 端 handler `tx.send(decision)`
- A 拿到决定（`oneshot::Receiver` 收到值或 30s 超时）后：
  - 立即清理本机 `pending_approvals[request_id]`
  - emit `handshake-dismissed` 事件 → 本机弹框消失
  - broadcast `/peers/approval/dismiss` → 所有 peer 清理 `forwarded_approvals` + emit `handshake-dismissed` → 弹框消失
  - 决定 = approve → 派生 AES 密钥、加 peers、加 `approved_device_ids`、broadcast trust（trust 属 `group-trust-gossip` P2，本 feature 仅做局部约定）→ 返回 200 握手响应
  - 决定 = reject → 加 `banned_device_ids`、broadcast ban（同上 P2）→ 返回 403
  - 决定 = timeout → 返回 408
- 30 秒倒计时由前端从 `handshake-pending` 事件里的 timestamp 自行驱动（弹框组件每秒 tick）
- 多请求并发处理：同时弹多个框时，标题区显示 `还有 N 个待处理`
- 黑/白名单短路：若 `request_id` 对应的 subject_device_id 已在我方 `banned_device_ids` 或 `approved_device_ids`，handle_approval_forward 静默 OK 不弹框（决定会自动生效）

**out of scope**（v2 这个 feature 不做）：
- trust / ban 列表跨机器传播（属 `group-trust-gossip`，P2）—— 本 spec 只约定 A 自己加 `approved_device_ids` / `banned_device_ids` 内存集合，不广播
- 信任名单的持久化（00 总览 第 3 节 已锁定不持久化）
- 弹框跨 OS 用系统级原生通知（macOS UserNotifications / Win Toast）——v0 用浮窗内覆盖层，v2 默认沿用，第 7 节 [UX] 留风险
- 文件接收审批（结构相似但属于 `file-transfer-drag`，P1）

## 4. 验收标准（Definition of Done）

- [ ] A、B 已连接（`小组 · 2 台`）。在 C 上点加入填 A 的地址 → A 与 B 浮窗**同时**出现审批弹框，显示申请方设备名 + 30s 倒计时
- [ ] B 上点"同意" → A 与 B 的弹框 ≤ 1 秒内同时消失，C 浮窗变成 `小组 · 3 台`
- [ ] 在 D 申请加入时 A 与 B 同时弹框 → A 上点"拒绝" → B 弹框消失，D 收到 403 提示`对方拒绝了你的加入请求`
- [ ] 30s 内无人响应 → A 与 B 弹框同步消失，申请方收到 408 提示
- [ ] 同时有 C、D 两个未知设备申请加入 → A 上弹两个框（或一个含 `还有 1 个待处理` 标记），任一处理另一个不被影响
- [ ] 已经被某成员拒绝过的 device_id 重新申请（前提是 ban 还在某节点的内存里）→ 那个节点不弹框，握手在该节点直接 403（前提：已加 trust gossip 后；本 P0 仅本机校验）
- [ ] A 与 B 的两个用户**同时**按下不同按钮（A 同意、B 拒绝）→ first responder wins，C 收到的最终结果与 A 收到的 oneshot 值一致（即唯一一个最先到达 oneshot 的决定生效）；不会出现 C 同时收到 200 和 403。注：按下按钮后但 oneshot 尚未确认前的本地 UI 反馈（如按钮置灰 / 显示"已发送…"）属 第 6 节 UX 设计范围（见 第 7 节 [P1] [UX]）
- [ ] forward 单跳约束：A 把审批 forward 给 B，B 收到 `/peers/approval/forward` 后**不会**把同一 request_id 再次 forward 给其它 peer（B 不是握手入口节点）；以单元测试或日志断言形式验证（即 B 端不调用 broadcast_approval_forward）

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的
`legacy-prototype` 分支 `src-tauri/src/network/server.rs` 的 `handle_handshake`（87-235 行）含完整审批流程：UUID `request_id` → `pending_approvals.lock().insert(rid, tx)` → emit `handshake-pending` 本机事件 → spawn `client::broadcast_approval_forward` 给所有 peer → `tokio::time::timeout(APPROVAL_TIMEOUT=30s, rx)` 等决定 → 不论结果先 broadcast dismiss + emit `handshake-dismissed` + remove from pending_approvals → 按结果走 200/403/408 + （approve 时）broadcast_trust。`handle_approval_forward / decide / dismiss`（474-580 行）在每个非 A 节点上：forward → 校 origin 是已知 peer + dedupe seq + 黑/白名单短路 → 插 `forwarded_approvals` + emit 弹框；decide → 校 origin + 找本地 `pending_approvals[rid]` → `tx.send` ；dismiss → 校 origin + remove `forwarded_approvals` + emit 关框。`AppState` 里两组 map：`pending_approvals: Mutex<HashMap<String, oneshot::Sender<bool>>>`（仅 A 用）+ `forwarded_approvals: Mutex<HashMap<String, ForwardedApprovalInfo>>`（其它 peer 用）。

### 5.2 v0 暴露的具体坑
- **双 map 配对清理是隐式不变式**：`pending_approvals` 在 A 上、`forwarded_approvals` 在其它 peer 上，必须配对插入 / 配对清理；任何一个泄漏都会导致内存增长 + 弹框残留。**v0 这条规则只在作者头脑里**。
- **forward 之后 A 才 spawn dismiss**：理论上 A 决定时刻 dismiss 在前 / forward 在后会出现"先收到 dismiss、再收到 forward"的乱序——v0 没有 seq dedupe 防御逻辑乱序，只是经验上"approval 先 forward 再 dismiss" 实践没出问题
- **multi-hop forward 不存在**：A → B forward 后，B 不再继续 forward 给 C（B 不是握手入口，C 也不该重复弹框）。v0 隐式约定 forward 仅一跳，但代码层不强制（如果 B 启发式重新 forward 会死循环）
- **approval timeout = 30s 是硬编码**：用户可能希望更短（紧急加入）或更长（远程协作让对方走过去）—— v0 不可配。**约束**：approval_timeout 与申请方握手客户端 timeout（`group-discovery` 第 3 节 锁定 35s = 30s 审批 + 5s 网络余量）必须**双端同步**——任一端改值另一端必须同步改，否则配对错位（申请方先 timeout 而审批方仍在等）
- **first-responder-wins 的实现靠 oneshot::Sender 单次性**：`pending_approvals` map 只允许一次成功 send；后到的 send 直接 drop。v0 这条逻辑正确，但 spec 必须明文记录
- **黑/白名单短路在 forward 时检查**：`approved_device_ids` 命中即不弹（避免重复打扰），但当时 A 还没收到 trust gossip 就开始 forward 的场景下不命中——这是边缘 case，v0 没专门处理

### 5.3 v2 应继承
- 分布式审批 + first-responder-wins + auto-dismiss
- `request_id` UUID 标识请求
- `pending_approvals` (在握手入口节点) + `forwarded_approvals` (在其它节点) 双 map 设计
- 30 秒超时
- 同 device_id 在 banned/approved 集合的短路（在 forward 检查时）
- HTTP 端点：`/peers/approval/forward` + `/peers/approval/decide` + `/peers/approval/dismiss`
- 协议 DTO 字段：`origin_device_id, seq, request_id, subject_device_id, subject_device_name, accept`

### 5.4 v2 应挑战
- **配对清理不变式必须文档化**：在 ADR 里明文列出"`pending_approvals[rid]` 的生命周期 = 收到握手时 insert，决定到达 / 超时 / dismiss 时 remove；任何 panic / 异常路径必须保证 remove"
- **multi-hop forward 防御**：在 forward handler 里检测"我是不是握手入口节点"——若不是仍然 forward 是否是 bug？需架构师在 ADR 里强制单跳约束
- **审批超时可配**：是否把 30s 写到配置（默认 30，允许 10-60 调整）属架构师 ADR 决议；**前提约束**：若可配，必须强制申请方握手 client timeout = approval_timeout + 5s（双端同步），见 第 7 节 [P1] [架构师]
- **审批弹框 UX 改进**：是否需要"申请方 IP 地址"显式展示在弹框里以让用户验真（防止恶意 device_name 伪装）？v0 仅显示 device_name
- **lock 顺序**：`pending_approvals.lock` + `forwarded_approvals.lock` 不能嵌套（v0 没嵌套但易踩）
- **broadcast forward 失败的处理**：若 `/peers/approval/forward` 给某 peer 网络失败，v0 仅 log，不通知用户——是否在状态栏显示"部分 peer 未收到审批"？

## 6. UX 设计

> 本节由 ux-designer 在 2026-05-06 填写。颜色/字号/字体/间距等视觉语言定义见 floating-window.md 第 6.5 节，本节不重复定义。

### 6.1 信息架构

审批弹框是一个**覆盖在 main view 之上**的模态层，遮盖历史列表，浮窗其余结构（顶部状态栏、底部 footer）仍可见。信息按优先级：

1. 申请方设备名（最关键，用户靠它判断"是否认识这台设备"）
2. 申请方 IP 地址（辅助验证，见 6.4 决策说明）
3. 30 秒倒计时（行动时限）
4. 同意 / 拒绝双按钮
5. 并发请求数提示（当 > 1 个请求时）

### 6.2 关键流程图（文字版）

主路径（审批通过）：

1. C 发起握手 → A 与 B 同时收到 `handshake-pending` 事件
2. A 与 B 的浮窗各自显示审批覆盖层（含 C 的设备名 + IP + 30s 倒计时）
3. B 点"同意" → B 发 `/peers/approval/decide` 给 A
4. A 收到决定 → broadcast dismiss → A 与 B 的弹框同时消失 → C 连入，浮窗变 `小组 · 3 台`

主路径（审批拒绝）：

1. 同上 1-2 步
2. A 点"拒绝" → 决定直接给本机 oneshot → A broadcast dismiss
3. A 与 B 弹框消失 → C 收到 403

主路径（超时）：

1. 同上 1-2 步
2. 30s 内无人响应 → 倒计时归零 → 弹框自动关闭（无需用户操作）→ C 收到 408

异常路径：

- 用户点决定后，按钮进入 disabled 状态 + 显示"已发送…"等待确认（防止双重点击）
- 网络抖动导致 dismiss 未到达：弹框会在本地 30s 自我超时后关闭（forwarded_approvals 自清理属架构师 ADR 议题）
- 多个并发请求：在同一覆盖层内同时展示（见 6.3 多请求 wireframe）

### 6.3 ASCII wireframe（必填）

单请求状态（主卡片，覆盖历史列表区域）：

```
┌────────────────────────────────┐
│  ● 小组 · 2 台  [加入]  −  ⚙  │← 顶部状态栏仍可见（非 modal）
├────────────────────────────────┤
│  ██████████████████████████████│← 半透明蒙层 rgba(0,0,0,0.5)
│  █                            █│
│  █  ╔══════════════════════╗  █│
│  █  ║  📥  有设备申请加入   ║  █│← 图标 + 标题，13px
│  █  ╠══════════════════════╣  █│
│  █  ║                      ║  █│
│  █  ║  工作 Mac             ║  █│← 设备名，14px #f3f4f6 加粗
│  █  ║  192.168.1.88        ║  █│← IP 地址，12px #9ca3af
│  █  ║                      ║  █│
│  █  ║  ⏱ 还剩 23 秒        ║  █│← 倒计时，12px，颜色随时间变化
│  █  ║                      ║  █│
│  █  ║  [拒绝]    [同意]    ║  █│← ghost + primary blue
│  █  ╚══════════════════════╝  █│
│  ██████████████████████████████│
├────────────────────────────────┤
│  192.168.1.50:5858    工作 Mac │← 底部 footer 仍可见
├────────────────────────────────┤
│   Made with Claude · by Tao   │
└────────────────────────────────┘
```

多请求并发（2 个请求）：

```
│  █  ╔══════════════════════╗  █│
│  █  ║  📥 有设备申请加入    ║  █│
│  █  ║  还有 1 个待处理      ║  █│← 11px #9ca3af，提示并发数
│  █  ╠══════════════════════╣  █│
│  █  ║  工作 Mac             ║  █│
│  █  ║  192.168.1.88        ║  █│
│  █  ║  ⏱ 还剩 23 秒        ║  █│
│  █  ║  [拒绝]    [同意]    ║  █│
│  █  ╚══════════════════════╝  █│
```

点击决定后（按钮 disabled 状态）：

```
│  █  ║  [拒绝]   [已发送 ✓]  ║  █│← 同意按钮变为"已发送"，disabled
│  █  ╚══════════════════════╝  █│
```

申请方浮窗（等待期）：

```
┌────────────────────────────────┐
│  ⏳ 等待审批                   │← 状态栏文字变为"等待审批"，蓝色状态点
├────────────────────────────────┤
│                                │
│      等待对方同意…              │← 中央提示，13px #9ca3af
│      [取消]                    │← ghost 小按钮，取消加入请求
│                                │
└────────────────────────────────┘
```

### 6.4 交互细节

点击区域：

- 蒙层背景：**不可点击穿透**（蒙层期间用户无法操作历史列表）
- 卡片内"同意"按钮：primary blue，点击后立即 disabled + 文字变"已发送 ✓"
- 卡片内"拒绝"按钮：ghost，点击后立即 disabled + 文字变"已拒绝"
- 卡片外蒙层：不响应点击（不允许点蒙层关闭，必须明确选择）

鼠标悬停反馈：

- "同意"按钮：brightness 稍亮
- "拒绝"按钮：background `rgba(239,68,68,0.12)`（浅红 hover 提示危险）

倒计时视觉（关键决策）：

- 30-16 秒：`#9ca3af` 灰（中性，"还有时间"）
- 15-6 秒：`#f59e0b` 橙（提醒，"快了"）
- 5-0 秒：`#ef4444` 红（紧急，"快决定"）+ 文字加粗
- 不做数字闪烁动画（闪烁令人焦虑，颜色渐变足够）
- 不做进度条（空间不足，进度条占用卡片高度）

IP 地址展示（关键决策）：

- 在设备名下方显示申请方 IP，12px `#9ca3af`
- 理由：IP 是用户判断"这是我的设备还是陌生设备"的辅助信息；恶意设备名（如伪装成"我的 Mac"）可以被 IP 戳穿；成本低（IP 在握手请求里已有），不增加协议复杂度

申请方浮窗等待状态：

- 状态点变蓝，状态文字"等待对方同意…"
- 中央显示"等待对方同意…"提示文字
- 有"取消"小按钮（ghost），点击中断加入流程

键盘可达性：

- `Tab`：在"拒绝"和"同意"按钮间切换焦点
- `Enter`：触发当前聚焦的按钮
- `Esc`：不关闭弹框（必须明确选择，防误关）

### 6.5 状态与颜色字典

详见 floating-window.md 第 6.5 节。审批弹框特有颜色：

| 元素 | 颜色 | 说明 |
|---|---|---|
| 蒙层背景 | `rgba(0, 0, 0, 0.50)` | 半透明遮盖历史列表 |
| 卡片背景 | `rgba(28, 28, 32, 0.96)` | 比浮窗背景稍不透明 |
| 卡片边框 | `rgba(255,255,255,0.12)` | 比窗口边框稍亮 |
| 设备名文字 | `#f3f4f6` + font-weight 600 | 14px，加粗显示 |
| 申请方 IP | `#9ca3af` | 12px 次要色 |
| 倒计时（≥ 16s） | `#9ca3af` | 中性 |
| 倒计时（6-15s） | `#f59e0b` | 橙色警示 |
| 倒计时（0-5s） | `#ef4444` + 加粗 | 红色紧急 |
| "同意"按钮 | primary blue `#3b82f6` | 见全局字典 |
| "拒绝"按钮 | ghost `rgba(255,255,255,0.12)` | 见全局字典 |
| 并发数提示 | `#9ca3af` | 11px hint 级别 |

### 6.6 边界与例外

- 0 个并发请求：不显示覆盖层（正常 main view）
- 1 个请求：标准卡片（无"还有 N 个待处理"提示）
- 2-N 个并发请求：卡片标题区显示"还有 N 个待处理"（N = 总数 - 1），每次处理一个，完成后自动显示下一个（先进先出）
- 不同时展示多张卡片（避免浮窗被弹框淹没，one-at-a-time 更清晰）
- 弹框超时（30s）自动消失，用户无需操作
- 按钮 disabled 期间（已发送决定但弹框尚未收到 dismiss）：用户看到"已发送 ✓" + 等待，禁止重复点击
- 网络慢导致 dismiss 延迟到达：弹框继续显示 disabled 按钮状态，不会让用户再次点击；等待 dismiss 或倒计时结束
- 实测可能暴露的问题：浮窗内覆盖层在用户当前不在浮窗窗口时无法看到（用户在 IDE 里），30s 很容易超时；这是弹框在浮窗内（而非系统通知）的固有缺陷，v2 接受这一限制

### 6.7 给前端工程师的实现提示（可选）

- 倒计时建议用每秒 tick 的 `setInterval`，从 `handshake-pending` 事件的 timestamp 反推剩余秒数（而非从 30 倒数），保证多设备倒计时视觉同步
- 多请求的队列建议用数组（`pendingApprovals: ApprovalItem[]`），始终展示 index 0 的请求；处理完（dismiss 事件）后 shift 移除，自动展示下一个

### 6.8 给主窗口和 PM 的反馈（UX 开放问题回应）

**问题 1**：弹框是覆盖层（v0）还是系统级原生通知。

结论：选覆盖层（v0 方案）。理由：
- 系统级通知不支持内联倒计时视觉；first-responder-wins 机制需要弹框在收到 dismiss 后立刻消失——系统通知无法做到精确撤销
- 系统通知需要 macOS UserNotifications 权限申请，增加安装摩擦
- 浮窗内覆盖层可以完全掌控 dismiss 时机和视觉状态
- 已知缺陷：用户不在浮窗前台时看不到，30s 超时。接受这一限制，属产品设计选择。

**问题 2**：申请方 IP 地址是否显式展示。

结论：显示。IP 是低成本的安全辅助信息，帮助用户区分"自己的设备"（已知 IP 段）和"陌生设备"（未知 IP）。恶意设备名欺骗（如伪装成"我的 Mac"）被 IP 揭穿的可能性值得这一行文字。显示格式：`192.168.x.x`，12px 次要色。

**问题 3**：多个并发请求的视觉处理（堆叠 vs 单卡片含计数）。

结论：单卡片 + "还有 N 个待处理"提示，one-at-a-time 处理。堆叠多张卡片在 320px 窗口里视觉混乱，且用户需要在多张卡片间做决定时容易混淆哪张对应哪个设备。先进先出队列，处理完一个再展示下一个。

**问题 4**：30s 倒计时的视觉（数字闪 / 进度条 / 颜色渐变）。

结论：颜色三段渐变（灰 → 橙 → 红）+ 数字每秒更新，不做闪烁动画，不做进度条。颜色变化已经提供足够的"时间紧迫感"，闪烁令人焦虑，进度条占用卡片空间。

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 4 条] [P1 3 条] [P2 0 条]

- [P0] [架构师] forward 单跳约束如何在代码层强制？需 ADR 明文 + 实现层 assert（与 第 4 节 验收对应）
- [P0] [架构师] `pending_approvals` 与 `forwarded_approvals` 双 map 设计是否拆为独立 module（如 `approval/registry.rs`）以集中清理逻辑？v0 散在 server.rs + state.rs。配对清理不变式必须 ADR 明文
- [P0] [安全] 同 LAN 攻击者构造 fake handshake 触发审批弹框轰炸（DoS 用户）：是否限流？v0 无限流
- [P0] [安全] device_name 任意字符串，弹框中直接显示——是否限制长度（≤ 64）+ 过滤控制字符 / Unicode 反向覆盖字符防欺骗？（与 `group-discovery` 第 7 节 / `settings-panel` 第 7 节 同议题）
- [P1] [UX] 弹框是浮窗内覆盖（v0）还是系统级原生通知？前者要求浮窗在前台才看见，后者更醒目；前者也是 first-responder-wins 等待 oneshot 期间 UI 反馈状态的设计载体（第 4 节 验收 #7 注解）
- [P1] [架构师] 审批决定的事件链路（A 本机决定 → ts.send → handshake handler 解套 → broadcast dismiss）目前同步，但 broadcast dismiss 是 fire-and-forget；若网络抖动 dismiss 没到某节点，弹框会显示完整 30s 直到自己超时——是否在 forwarded_approvals 中也设 30s 自我超时？
- [P1] [架构师] approval_timeout 是否可配（用户在 settings 里改）？决议必须含**双端同步约束**：`group-discovery` 第 3 节 的握手 client timeout 必须 = approval_timeout + 5s 网络余量（任一端单独改值会 break 配对）

## 8. Review 段（占位）

> code-reviewer / tech-architect / security-reviewer 后续填写。本 feature 涉及网络协议与 trust 状态机，必须经 security-reviewer ACK（CLAUDE.md 第 9 节）。
