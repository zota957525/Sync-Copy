---
status: SPEC_REVIEWED
owner: product-strategist
related_adrs: [ADR-003, ADR-009, ADR-010]
related_specs: [00-product-overview, group-discovery, group-leave-notify, floating-window]
created: 2026-05-06
updated: 2026-05-08
revised: 2026-05-08 — ADR-003 第 3.7 节 锁定隐形掉线参数 N=3 / M=2 / keepalive=20s + last_successful_sync_at 仅在广播 200 OK 时写 + client_pool per-peer Client 强制重连实现
priority: P1
revision_history:
  - version: v1
    date: 2026-05-06
    notes: 初版 SPEC_DRAFTED；priority=P2；仅靠 PING_INTERVAL=10s + FAIL_LIMIT=2 兜底
  - version: v2
    date: 2026-05-08
    notes: 用户 v0 实战反馈"隐形掉线"bug（_assumptions A_BUG_HIDDEN_DEAD / lessons-learned 第 4.1 段）— 长时间运行后部分设备表面在线但实际同步失败，重启程序兜底。priority P2 → P1。第 1 节加问题描述；第 4 节加 3 条 AC（强制重连 / 被动健康自检 / 上次成功同步时间显示）；第 7 节加 P1 给架构师定阈值
  - version: v3
    date: 2026-05-08
    notes: ADR-003 第 3.7 节 答 第 7 节 [P1] 5-6 个待决参数；UI 锚点视觉细节留 P2-3.c UX 段补
---

# peer-heartbeat — 周期 ping 被动检测离线 peer 并自动剔除

## 1. 问题（为什么做）

`group-leave-notify` 解决"用户正常退出"，但**异常**情况无法靠它兜底：拔网线、断电、关合上盖子、进程崩溃、Wi-Fi 切换、笔记本切换网络都会让某 peer 凭空消失。如果不主动检测，本机会一直把已死设备列在 peers 表里——用户看到 `小组 · 3 台` 实际只有 2 台，而且每次复制还会向死 peer 发请求超时浪费 5s。

`peer-heartbeat` 是**被动兜底**层：每 10s 给所有 peer 发一次 GET `/ping` → 连续 2 次失败 → 把它从 peers 表 + peer_keys 中剔除 + 状态更新。容忍单次抖动（避免 Wi-Fi 短暂闪断误剔除）；周期与超时是经验值（00 总览 第 5.3 节 已锁定）。心跳与 leave 形成双层防御：好情况（leave 到达）≤ 1 秒收敛，坏情况（leave 丢）≤ 20 秒收敛。

### 1.1 v0 实战 bug：隐形掉线（2026-05-08 用户反馈）

v0 长时间运行后用户反复观察到一类**表面正常但实际死透**的失败模式（已记入 `docs/handoff-lessons-learned.md` 第 4.1 段 + `_assumptions.md` 校对项 A_BUG_HIDDEN_DEAD）：

- **现象**：peer 列表仍显示对方在线，浮窗顶部"小组 · N 台"状态点仍绿，但本机复制内容**对端无任何反应**；甚至从对端复制本端也收不到。
- **根因怀疑**：① TCP 连接处于"半死"状态（一端进程已僵或网络层路由静默丢包，但 OS 端口仍占用），心跳层 GET `/ping` 因 keepalive 不够激进而依然返 200（甚至底层 socket 已 reset 只是没及时上报）；② 对端进程已 hang 住但 axum runtime 未 panic；③ 加密会话密钥某种条件下失同步但密钥层无自检；总之单靠 v0 现有的 `/ping → "pong"` 心跳判活路径过浅。
- **用户唯一兜底**：重启本端进程 → 触发 re-handshake → 恢复。这种"关一下又能用"的体验对单人多机工具尤其糟糕（用户在两台机间来回切，往往不知道是哪一端卡住）。

v2 必须做的**三条解决方向**（见 第 4 节 新增 AC 与 第 7 节 [P1] [架构师]）：① 心跳超时 N 次后**强制重建底层 TCP 连接**而不只是从 peer 列表移除；② 增加被动健康自检 — 本地剪切板变化广播失败 ≥ M 次时主动 ping 全组并刷新连接；③ UI 层显式暴露**上次成功同步时间**（不是"上次 ping 成功"），让用户在表面绿但实际死透时一眼识破。

## 2. 用户故事

- As a user with peer A on a flaky Wi-Fi, I want a single failed ping not to immediately remove A (it might come back), so that brief network glitches don't cause noisy "1 台 → 2 台 → 1 台" flapping.
- As a user with peer A whose laptop went to sleep / lost battery / crashed, I want it removed within ~20 seconds so my "小组 · N 台" count is honest, and so future broadcasts don't pointlessly time out trying to reach it.
- As a user removed from a group by heartbeat (because my own Wi-Fi died for >20s and came back), I want re-joining to be a normal handshake again, so that the system safely recovers without leaking trust.

## 3. 范围

**in scope**：
- HTTP 端点 `GET /ping` → 返 `"pong"` 200 OK（无 body 校验、不需要 origin 认证——仅探活）
- 后台 task `network::health::spawn(state, app)` 在应用启动时（auto_listen_on_startup 后）启动一次：
  - 独立 reqwest::Client：`no_proxy() + timeout(2s) + connect_timeout(1s)`
  - 维护 `fail_counts: HashMap<device_id, u32>` 累计失败次数
  - loop：`tokio::time::sleep(PING_INTERVAL=10s)` → `peers.snapshot()` 取当前 peer 列表
    - peer 表为空 → `fail_counts.clear()` + continue
    - 清理 `fail_counts` 中已不在 peer 表的 entry（避免 ban / leave 后残留）
    - 对每个 peer：`GET http://{addr}/ping` →
      - 2xx 成功 → `fail_counts.remove(device_id)` 重置
      - 失败（任何错 / 非 2xx 都算失败）→ 计数 +1；`>= FAIL_LIMIT=2` → `peers.remove + peer_keys.remove + fail_counts.remove + removed_any = true`
    - `removed_any` 时 `update_status_connected + emit status-updated`
- 常量定义（v0 已收敛，v2 暂沿用；是否暴露为用户可配设置项见 第 7 节 [P2] [架构师]）：
  - `PING_INTERVAL = Duration::from_secs(10)` —— 每 10 秒一轮
  - `PING_TIMEOUT = Duration::from_secs(2)` —— 单次 ping 总超时
  - `connect_timeout = Duration::from_secs(1)` —— 连接阶段超时
  - `FAIL_LIMIT = 2` —— 连续 2 次失败才剔除（容忍单次抖动）
- 启动顺序：`AppState 创建 → start_server_if_needed（含 axum 注册 /ping 端点）→ auto_listen_on_startup → health::spawn` —— 心跳 task 跑在 axum runtime 同一 tokio 运行时
- 心跳 task **不涉及加密**（/ping 是明文 GET，body 仅 `"pong"`）—— 探活信息无敏感性

**out of scope**：
- ping 的负载（payload）—— v0 仅 GET，不带 body；v2 同
- 心跳延迟统计 / 显示给用户（"延迟 5ms" 之类）—— 不是 v2 产品需求
- 自适应心跳频率（peer 多时降到 30s）—— 当前 N 上限 8 设备，10s × 8 = 8 RPS 可接受
- 心跳路径加密（仅 origin 标识需保密时才加密；v2 探活无 origin 信息）
- 单次 ping 失败的 fast-retry（10s 内不重试，等下一轮）
- 心跳与 NAT keepalive 复用（无 NAT，纯 LAN）
- 探测 RTT 用作链路质量评分（v2 无此需求）
- 心跳 task 的关闭：进程退出时 tokio runtime 一同退出（不 graceful，可接受）

## 4. 验收标准（Definition of Done）

- [ ] A、B 两机已 `小组 · 2 台`。在 B 上拔网线（或杀进程）→ A 浮窗在 ≤ 25 秒内变为 `小组 · 1 台`（含 10s 第一次 ping 失败 + 10s 第二次 ping 失败 + 处理时间）
- [ ] A、B 已连接，B 路由器抖动一次（ping 失败一次后立即恢复）→ A 不剔除 B（容忍单次失败，B 在 fail_counts 计数 1 → 第二轮 ok 重置为 0）
- [ ] A、B、C 三机连接。C 进程崩溃 → A、B 各自在 ≤ 25 秒内独立把 C 从 peers 表移除（A 与 B 心跳独立，无协同）
- [ ] A 把 B 剔除后，B 网络恢复且重启进程 → B 主动加入填 A 的 IP → A 上 trust gossip 命中（如 A 与 B 之前在 approved_device_ids）→ 不弹审批直接连上
- [ ] 心跳 task 启动时 peers 表为空 → 不报错；后续 peer 加入 10s 内开始被 ping
- [ ] 心跳 ping 路径不被代理劫持（client 用 `no_proxy()`）
- [ ] 心跳路径被 ban 后立即剔除（`group-trust-gossip` 中 ban 触发的即时 remove）的 peer 不会出现在下一轮 ping 列表（fail_counts 清理逻辑生效）
- [ ] N = 8 设备时单轮 ping 完成时间 < `PING_INTERVAL`（10s），不导致下一轮被推迟。具体实现是串行 for-loop 还是 `try_join_all` 并行属架构师 ADR 决议（见 第 7 节 [P0] [架构师]）；本 spec 不强制实现方式，但强制"单轮不超 PING_INTERVAL"行为
- [ ] **隐形掉线兜底 #1（强制重连）**：连续 N 次心跳超时（N 由架构师决定，建议 3）后，**强制重建底层 TCP 连接**（关闭旧 reqwest connection pool 中该 peer 的连接复用 + 触发一次 re-handshake 而非仅从 peer 列表移除）；触发后 5 秒内日志能观察到"forced TCP rebuild for {device_id}"轨迹
- [ ] **隐形掉线兜底 #2（被动健康自检）**：本地剪切板变化（文本 / 图片 / 文件）广播给某 peer 失败 ≥ M 次（M 由架构师决定，建议 2）时，触发**被动健康自检** — 主动 ping 全组并刷新与该 peer 的连接；这条 AC 验证：A 复制内容 → A → B 因 B 端僵死失败 → 第 2 次失败后 A 立即触发 health 自检 → 在下个 PING_INTERVAL 周期前完成对 B 的强制重连或剔除（不是等 N × PING_INTERVAL 被动检测）
- [ ] **隐形掉线兜底 #3（UI 上次成功同步时间）**：浮窗 / 浮球状态区显示"上次成功同步：{相对时间}"字段（如"刚刚" / "3 分钟前" / "12 分钟前"）。"成功同步"定义 = 本机最近一次成功**广播被对端 200 OK 确认**或最近一次成功**接收对端推送**。当某 peer 表面在线（peer 表中存在）但"上次成功同步"≥ 5 分钟时，UI 提供视觉提示（如状态点变黄 / hover 提示"长时间无同步"），让用户能在表面绿但实际死透时一眼识破 — 这是用户层"重启之前先看一眼"的诊断锚点

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的
`legacy-prototype` 分支 `src-tauri/src/network/health.rs`（74 行，最干净的模块之一）：
```
PING_INTERVAL = 10s
PING_TIMEOUT = 2s
FAIL_LIMIT = 2
```
`spawn(state, app)`：
1. `tauri::async_runtime::spawn` 一个 long-lived task
2. `reqwest::Client::builder().no_proxy().timeout(2s).connect_timeout(1s).build()` 构造心跳专用 client（独立于剪切板 / handshake / file 各自的 client）
3. `fail_counts: HashMap<String, u32>` 维护每 peer 累计失败
4. loop：
   - `tokio::time::sleep(PING_INTERVAL=10s)`
   - `peers.snapshot()` 取列表；空 → clear fail_counts + continue
   - `alive_ids: HashSet<...>` = peer ids；`fail_counts.retain(|k, _| alive_ids.contains(k))` 清理已离开的
   - **串行** for each peer：`GET http://{peer.addr}/ping` → 2xx → remove fail_count；fail → entry().and_modify(|c| *c += 1).or_insert(1) → if >= FAIL_LIMIT → remove peer / peer_keys / fail_count + `removed_any = true`
   - `removed_any` → `update_status_connected + emit status-updated`

`network/server.rs::handle_ping`（一行）：`async fn handle_ping() -> &'static str { "pong" }`，`/ping` 用 `axum::routing::get` 注册（区别于其它 POST 端点）。心跳 client 与 reqwest builder 区别于 `client.rs::build_client()`（5s/3s）—— 心跳更快超时（peer 真死时不该等久），但这两套 client 共存在于代码里没有 ADR 论证。

### 5.2 v0 暴露的具体坑
- **串行 for each peer**：N=8 时单轮最差 8 × 2s = 16s 超过 PING_INTERVAL=10s；下一轮就被推迟。v0 N 实战 ≤ 3，未触发；v2 N 上限假设需 ADR 明定（00 总览 第 5.4 节 + group-discovery 第 7 节 已点名）
- **失败原因不区分**：`Err(_) => false` 把所有失败合并（DNS 解析失败 / 连接拒绝 / TLS 错误 / 超时）—— 调试时只能猜
- **fail_counts 用 String 索引**：与 `last_seen_seq` / `peer_keys` 等同样模式；多份 HashMap 在 AppState / health 各持一份，未抽象统一 PeerStatusRegistry（00 总览 第 5.4 节 待挑战的 AppState 上帝结构问题之一）
- **PING_INTERVAL = 10s 是经验值**：未论证选 5s vs 10s vs 30s 的 trade-off；用户感知离线滞后 20s 仍是 v2 体验底线
- **重启 peer 但 device_id 不变**：例如 B 重启进程但 IP 不变 → B 的 axum 重新监听 `/ping` → A 的下一轮 ping 收到 200 OK → fail_count 重置 → A 不会主动重新协商密钥（密钥已丢，下次发剪切板时 broadcast 加密 key 无 → 跳过）。这是边缘 case：B 重启后必须主动握手才能继续接收（`group-discovery` 已实现 re-handshake 路径）
- **task 没有取消机制**：进程退出时 OS 杀；如果 v2 改 server 重启逻辑（用户改端口），心跳 task 不受影响（ping 用的是 peer.addr，不是本机端口）
- **`removed_any = true` 在所有 peer 处理完后才 emit `status-updated`**：批量 batched 而非每移除一个就发——避免事件抖动（好）
- **emit `status-updated` 仅在剔除时**：peer 一直健康时不发事件；前端浮窗状态文字不会闪——这是好的设计

### 5.3 v2 应继承
- 单文件 `network/health.rs` 干净独立
- `PING_INTERVAL = 10s` + `PING_TIMEOUT = 2s` + `connect_timeout = 1s` + `FAIL_LIMIT = 2` 经验值
- `fail_counts: HashMap<device_id, u32>` 累计失败计数 + 每轮 retain 清理已离开
- 容忍单次抖动（FAIL_LIMIT = 2 的核心价值）
- 独立 reqwest::Client（与剪切板 / handshake client 隔离）
- `/ping → "pong"` 极简端点 + GET 方法
- emit `status-updated` 仅在剔除时（避免事件抖动）

### 5.4 v2 应挑战
- **串行 → 并行 ping**：N 上限 8 时建议改 `try_join_all`（peers 并发 ping）确保单轮 ≤ PING_TIMEOUT 完成
- **失败原因细分**：DNS / 连接拒绝 / 超时 / TLS 等分类记入 tracing，便于调试用户 "为什么我莫名被剔除"
- **fail_counts 抽象到 PeerRegistry / 统一状态管理**：与 `last_seen_seq` / `peer_keys` / `approved_device_ids` 等共享一个 PeerState 概念（00 总览 第 5.4 节 AppState 上帝结构教训）
- **PING_INTERVAL 是否暴露设置**：用户在不稳网络下可能想 5s 检测，稳定 LAN 想 30s 省流——v0 硬编码
- **leave + heartbeat 重叠场景**：B leave 广播到 A → A 立即 remove；同 10s 内 A 心跳又给 B 发 ping → 此时 B 已不在 peer 表 ping 不会发出（fail_counts 已 retain 清理）。无问题，但 spec 必须文档化这条不变式
- **N 上限 ADR 明文**：≤ 8 vs ≤ 5？影响并发 ping 的工程必要性
- **`/ping` 是否应加 origin 校验**：当前任意 LAN 设备能 ping 暴露"这台跑了 Sync Copy"事实——属信息泄露但同 LAN 通常已知
- **健康 task 取消机制**：v2 是否在 quit_app 路径中显式 abort 该 task（v0 靠 OS 杀）

## 6. UX 段（占位）

> 原 v1 标记 N/A（纯后端）。v2 新增"隐形掉线兜底 #3"AC（第 4 节）后引入 UI 元素，需 ux-designer 在后续阶段填写：
> - **"上次成功同步时间"字段**展示位置（浮窗顶部状态栏 / 历史区头部 / hover tooltip）
> - **相对时间格式**（"刚刚" / "3 分钟前" / "12 分钟前" / "1 小时前"）的阈值与刷新频率
> - **降级提示视觉**（"上次成功同步" ≥ 5 分钟时状态点变黄 / 文字变橙 / 显示警告图标的具体设计）
> - 与 `floating-window` 状态栏 `小组 · N 台` 显示的视觉协调（不能让两个状态指示互相打架）
> - 浮球（minimized 状态）下是否也展示该字段，还是仅浮窗（expanded 状态）显示

无显式 UI 元素的部分（心跳/重连/自检纯后端逻辑）仍属 N/A。

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 2 条] [P1 4 条] [P2 3 条]

- [P0] [架构师] 串行 → 并行 ping：N 上限 8 时是否升级 `try_join_all`？v0 串行在 N ≤ 3 不触发问题；与 第 4 节 验收 #8 直接绑定
- [P0] [架构师] N 上限假设 ADR 明文（≤ 5 / ≤ 8 / 不限）—— 直接影响并发 ping 设计与 gossip mesh 设计（与 `group-discovery` 第 7 节 同议题）
- [P1] [架构师] **隐形掉线三参数 + 三机制定义**（_assumptions A_BUG_HIDDEN_DEAD 联动；与 第 4 节 新增 3 条 AC 直接绑定）：
  - **N**（心跳超时强制重连阈值）取值 — 建议 3，是否合理？过低会在弱网误重连，过高则隐形掉线收敛慢；与 FAIL_LIMIT=2（剔除阈值）的关系：N=3 意味"连续 3 次失败强制重连一次（不剔除），仍失败到第 N+FAIL_LIMIT 次再剔除"还是"FAIL_LIMIT 不变 = 2，但第 2 次失败时同时触发重连+剔除"？需明确状态机
  - **M**（广播失败触发被动健康自检阈值）取值 — 建议 2；与剪切板单次失败的 retry 关系（v0 在 client.rs 是否已有 retry 1-2 次？）；本机剪切板变化对 N 个 peer 的失败计数是 per-peer 还是全局？
  - **TCP keepalive 配置**：reqwest 0.12 是否启用 `tcp_keepalive(Duration::from_secs(K))`？K 取值；连接池中已存在的连接如何强制驱逐（drop reqwest::Client 重建 vs 显式 close 单个连接）
  - **被动健康自检的 ping 频率**与正常 PING_INTERVAL=10s 的关系：是临时插队一次 ping 全组，还是缩短下一轮 PING_INTERVAL？是否设防抖（同一 peer 30s 内只触发一次自检，避免广播失败风暴时刷屏）
  - **piggyback 机制**：是否考虑在加密剪切板广播 ACK 中携带 health 信号（成功 ACK = 隐式心跳），减少独立 ping 流量；属优化项，可不在 v2 P1 阶段决议
  - **"上次成功同步时间"的定义边界**：对端 200 OK = 成功？还是必须 decrypt 成功 + history push 成功才算？跨平台时钟漂移如何处理（用本机收到 ACK 的本地时间，还是依赖某种共识时间）
- [P1] [架构师] B 重启后 device_id 不变 + IP 不变的场景：A 心跳不会失败 → A 不会重新握手 → 密钥协商缺失 → 后续广播跳过 B；只能等 B 主动 re-handshake；是否在 health 检测到 "ping 成功但加密失败" 时主动触发 re-handshake（注意：本议题与"隐形掉线兜底 #2 被动健康自检"实质上是同一类"广播失败 → 主动诊断"机制，架构师 ADR 时应合并设计）
- [P1] [架构师] PeerRegistry / 统一状态管理：fail_counts 与 last_seen_seq / peer_keys / approved_device_ids / **last_successful_sync_at** 等是否抽出共享 PeerState（新增"上次成功同步时间"字段是另一条加入此 registry 的状态）
- [P1] [安全] `/ping` 端点是否需要 origin 校验？任何 LAN 设备能探活暴露"此机跑 Sync Copy"事实
- [P2] [架构师] PING_INTERVAL 用户可配 vs 硬编码：v0 硬编码 10s；用户在弱网下可能希望 5s。本 spec 第 3 节 暂沿用 10s 待 ADR 决议
- [P2] [架构师] 失败原因细分到 tracing（DNS / connect / timeout / TLS 各 log 一类）便于诊断
- [P2] [架构师] 健康 task 取消机制：v2 quit_app 路径是否显式 abort 该 task；v0 靠 OS 杀

## 8. Review 段（占位）

> code-reviewer / tech-architect / security-reviewer 后续填写。本 feature 涉及网络层周期任务与状态机，需 security-reviewer 评估 `/ping` 暴露的信息泄露风险（CLAUDE.md 第 9 节）。

## 8. Code Review (by code-reviewer · 2026-05-09 · PR-2 commit 69597a4)

**结论**：CHANGES_REQUESTED（2 条小补丁可直派 backend-implementer 静默落地；无严重违反 ADR 决议项；小补丁完成后即可推 REVIEW_PASSED）

### 8.1 Spec / ADR 一致性 / ADR-009 v1.2 4 补丁

1. MUST-4 remove 原子顺序：✅ `inner.remove → approved.remove → banned.remove` 严格按声明序在同一函数内完成（`mod.rs:214-238`）；client_pool 内嵌按 PR 范围正确推迟到 PR-3，注释（`mod.rs:19-21, 209-211, 231-232`）显式标注。
2. P4 锁顺序硬约束：✅ `approve()`（`mod.rs:265-280`）/ `ban()`（`mod.rs:297-319`）严格按 approved → banned 拿写锁；`ban()` 注释（`mod.rs:283-288, 303-305`）明确说明字面操作顺序与锁取得顺序的差异；模块顶部注释（`mod.rs:10-12, 115-117`）二次重申硬约束。⚠ **仅缺 lock_order_no_deadlock 单测**（详见 8.2 严重 #1）。
3. P1 snapshot/get SECURITY 注释：✅ `snapshot()`（`mod.rs:175-179`）+ `get()`（`mod.rs:166-170`）方法签名上方均有完整 SECURITY 段，覆盖 Zeroizing clone / Debug-print / tracing fields / 落盘 / 跨进程发送禁令。
4. P3 RateLimiter 未认证 device_id 安全：✅ struct 上方 SECURITY 段完备（`rate_limit.rs:53-59, 64-70`）；`check_handshake` 警告日志只记 `remote_ip + count`，不写 `device_id`（`rate_limit.rs:121-126, 151-156`）；per_pair / global 容量上限 + 过期 retain 策略已以"占位 + group-discovery feature ADR 接管"形式注明（`rate_limit.rs:20-32, 96-98`）。
5. AadKind Hash 孤儿 impl：✅ 选择本模块孤儿 impl（`mod.rs:417-422`）合理 — PR-1 crypto 已落定不动；用 `as_bytes()` 稳定字节做 hash 键正确；选择可接受、不阻塞。

### 8.2 必修条目落地

- MUST-2 zeroize import：✅ `aes_key: Zeroizing<[u8; 32]>`（`mod.rs:83`）+ `Cargo.toml zeroize` 依赖正确。
- MUST-4 remove 原子顺序：✅（见 8.1 第 1 项）。
- MUST-5 panic message：✅ 单测中 `expect("test addr parse failed")` / `expect("inserted peer should be found")` 等均为字面量，无运行时变量插值，符合 ADR-008 第 7.2 节 MUST-5 约定。
- ADR-009 v1.2 P1 / P2 / P3 / P4：✅ / ✅（PR-2 范围内 health.rs 反模式黑名单 P2 是 PR-3 范畴，本 PR 不涉及）/ ✅ / ✅。

### 8.3 发现的问题（按严重度排序）

#### [中等] 缺失 ADR-009 第 6.1 节 单测 #13 `lock_order_no_deadlock`
- 文件：`src-tauri/src/peer/mod.rs:428-736`（`#[cfg(test)] mod tests`）
- 现象：commit message 与 mod.rs:425 注释均自称落了 `lock_order_no_deadlock`，实际只有 10 个 `#[test]`，无任何并发 spawn/线程死锁测试。`Cargo.toml` 也未启用 `parking_lot/deadlock_detection` feature。
- 风险：ADR-009 第 4.3 节副作用 #1 + 第 6.1 节单测 #13 均明文要求 "dev profile 跑 approve/ban/remove 并发 100 次不死锁" 作为 P4 锁顺序硬约束的兜底证明。当前 P4 仅靠注释 + 串行单测覆盖；未来 implementer 误改 ban() 字面顺序 → release 卡死的回归无自动化拦截。
- 建议修法：补一条 `#[test] fn lock_order_no_deadlock()`：`Arc<PeerRegistry>` + `std::thread::spawn` 100 个线程随机调 `approve / ban / remove` 同一组 id；用 `std::sync::Barrier` 同步起跑；测试加 `#[timeout(...)]`（或循环结束后判断时长 < 5s）作为活性证明。可选：`Cargo.toml [dev-dependencies]` 加 `parking_lot = { version = "0.12", features = ["deadlock_detection"] }` 并在测试入口起 deadlock 检测线程。

#### [低] commit message 与代码不符（声称单测覆盖与实际不一致）
- 文件：commit `69597a4` body
- 现象：commit message 自称 "peer::tests 10 ... lock_order_no_deadlock 等"，实际 10 个测试名中无此项。
- 风险：未来 audit / blame 时误导，让 reviewer 与 PM 误以为已覆盖。
- 建议修法：补完 `lock_order_no_deadlock` 后该问题自然消解；无需单独修改历史 commit。

#### [低 / nit] `allowed_decision_is_stable` 单测名与实际断言不符
- 文件：`src-tauri/src/peer/rate_limit.rs:264-274`
- 现象：测试名暗示"多次连续调用稳定"，实际只调 1 次 check_handshake 后断言 Allowed，未连续多次。
- 建议修法：要么改名为 `allowed_decision_first_call`，要么在测试体内补 2-3 次连续 Allowed 断言以匹配名字。

### 8.4 风险点

- **PR-3 client_pool 集成时的 ban() 路径补漏点**：当前 `ban()`（`mod.rs:297-319`）在 was_peer=true 分支只 `inner.remove`，未补 `client_pool.remove`，注释（`mod.rs:292, 314`）已留 TODO。PR-3 实施时**必须**在 ban() 与 remove() 两处都补上 `client_pool.remove`，否则 invariant 3（`client_pool.contains(id) == inner.contains_key(id)`）会破。建议 PR-3 backend-implementer 接到 ADR-010 后**第一行**就读本 review 8.4。
- **`seen_seq_and_update` 对未知 peer 返 false 是合理"安全侧"** — 但调用方若把 false 一律映射成"重复，200 OK 静默丢"，则陌生 peer 的合法首包也会被吞。需在 PR-3 / handshake handler 落地时确认调用顺序：先 `is_known` 校验或先走 handshake，不要直接把陌生 peer 的报文进 `seen_seq_and_update`。建议 ADR-010 / handshake handler PR 评审时复查。
- **clear() 内三把锁顺序写**（`mod.rs:244-251`）虽按声明序拿，但每次都是 acquire→release→acquire（write 锁不重叠）。当前实现没有 AB-BA 死锁面，但若未来改为"全部 hold 后 clear"需重新审。

### 8.5 给 implementer 的明确 todo 清单

- [ ] 补 `#[test] fn lock_order_no_deadlock()` 到 `src-tauri/src/peer/mod.rs::tests` — Arc<PeerRegistry> + spawn 100 线程随机调 approve/ban/remove 同一 id 集合，用 Barrier 同步起跑，断言 5s 内完成（活性证明）。可选启用 `parking_lot/deadlock_detection` dev feature。
- [ ] `src-tauri/src/peer/rate_limit.rs::tests::allowed_decision_is_stable` 改名或补充连续多次 Allowed 断言（与名字匹配）。

### 8.6 测试覆盖评估

- **当前 13 单测**（peer 10 + rate_limit 3）vs ADR-009 第 6.1 节最小集（≥ 7 条）：
  - #1 insert_then_get：✅ `insert_get_remove_basic`
  - #2 remove_clears_inner_and_pool：⚠ 部分（无 mock ClientPool 调用顺序断言；PR-3 接 client_pool 后补）
  - #3 approve_atomic：✅ `trust_mutual_exclusion`
  - #4 ban_atomic_was_peer：✅ `trust_transition_atomicity` + `remove_atomic_order` 联合覆盖
  - #5 ban_atomic_unknown：✅ `ban_unknown_peer_does_not_affect_inner`
  - #6 trust_overrides_ban：✅ `trust_transition_atomicity` 后半段
  - #7 ban_overrides_trust：✅ `trust_transition_atomicity` 前半段
  - #8 seen_seq_dedupe：✅ `seen_seq_and_update_dedupe`（含 unknown peer / 不同 kind 独立计数）
  - #9 record_send_ok_updates_last_sync：✅
  - #10 record_heartbeat_ok_does_not_update_last_sync：✅
  - #11 record_heartbeat_fail_increment：⚠ 缺（建议补；不阻塞）
  - #12 clear_all：⚠ 缺（建议补；不阻塞）
  - #13 lock_order_no_deadlock：❌ **缺**（见 8.3 中等）
  - #14 aes_key_zeroize_after_remove：⚠ 缺（ADR-009 第 6.1 节标注 best-effort 跨平台不强制；不阻塞）
- 已覆盖最小集 7/14；强制项满足（≥ 7），但 #13 是 P4 死锁硬约束的唯一活性证明，必须补。

