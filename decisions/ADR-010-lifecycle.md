---
id: ADR-010
feature_id: lifecycle
title: Lifecycle 启停步序 / 4 退出路径收敛 / panic hook 注册位置 / long-running task owner / shutdown grace period
status: ACCEPTED
owner: tech-architect
date: 2026-05-09
accepted_at: 2026-05-09
security_signoff: ADR-010 第 7 节追加签字（CHANGES_REQUESTED → 4 补丁已落 v1.2）2026-05-09
deciders: [tech-architect, main, user]
user_decision_summary: 4/4 决策卡片用户 2026-05-09 全选 A（采纳架构师推荐）；卡 1 grace period A 固定 deadline ≤ 2800ms；卡 2 P0 tray 例外 A 允许 + TODO；卡 3 panic hook A 注册在 lib.rs::run 最早入口；卡 4 runtime A 全用 Tauri 内置 async_runtime
user_meta_feedback: 用户反馈"决策疲劳 + 都是细节 + 我没这方面判断经验"。本 ADR 4 张卡片本不应上报用户（均为技术实现细节）；主窗口编排策略将调整为"技术细节走架构师 + sec 双签自动落地，仅产品方向 / 不可逆操作 / 范围变更上报用户"。详 docs/handoff-lessons-learned.md 第 5 段
related_specs:
  - peer-heartbeat
  - group-leave-notify
  - diagnostic-logging
  - tray-integration
  - settings-panel
related_adrs:
  - ADR-003
  - ADR-008
  - ADR-009
supersedes: []
superseded_by: []
revision_history:
  - version: v1
    date: 2026-05-09
    notes: 初版 — P2-1.b 第一批第二份。把 ADR-003 第 3.5 节"app/lifecycle.rs 集中管理 + 启动 7 步 / 关闭 7 步 + 退出全走 quit_app"决议落到代码契约 + 启停步序 + panic hook 注册位置 + 错误兜底 + 单元测试清单层面。落实 ADR-008 MUST-5（panic message 不含运行时变量）+ 第 6 节 fatal 三件套
  - version: v1.1
    date: 2026-05-09
    notes: 用户拍板全 A（1A 2A 3A 4A 采纳推荐）；status PROPOSED → ACCEPTED_PENDING_SECURITY_SIGNOFF；deciders 加 [main, user]。**用户元反馈：决策疲劳，要求主窗口降低技术细节决策卡片密度** — 触发主窗口编排策略调整（lessons-learned 第 5 段记账）
  - version: v1.2
    date: 2026-05-09
    notes: 落 security-reviewer 第 7.3 节 4 条补丁（P1 panic hook prev(info) 注释 + core dump 边界声明 / P2 P0 tray 例外 tracing::warn 强制 + 反模式黑名单 / P3 第 3.3 节 step 3 banned peer snapshot 信息泄露注释段 / P4 第 3.6 节 health worker Shutting 禁 replace + 反模式黑名单）；status ACCEPTED_PENDING_SECURITY_SIGNOFF → ACCEPTED；按 lessons-learned 第 5 段第 10 条新策略"文本级补丁主窗口直接 ACCEPTED 不再二次 sec 审"
depends_on_artifacts:
  - path: decisions/ADR-003-project-architecture-skeleton.md
    version: ACCEPTED 2026-05-08（第 3.5 节 + 第 3.6 节末尾 panic hook + 第 8 节 卡片 5 must-fix）
  - path: decisions/ADR-008-security-review-of-adr003.md
    version: ACCEPTED 2026-05-08（第 7.2 节 MUST-5 + 第 6 节 fatal 三件套 + 第 9 节实施提示 #5）
  - path: decisions/ADR-009-peer-registry.md
    version: ACCEPTED 2026-05-09（第 5 节 #5 启动顺序 + 第 7.3 节 P2 反模式）
  - path: specs/peer-heartbeat.md
    version: v3 2026-05-08（第 3 节 health worker 启停时机 + 强制重连）
  - path: specs/group-leave-notify.md
    version: 2026-05-08 SPEC_REVIEWED（第 3 节 关闭时 leave 时序 + 1.5s timeout）
  - path: specs/diagnostic-logging.md
    version: 2026-05-08 SPEC_REVIEWED（第 3 节 tracing-appender 在 lifecycle 第几步初始化）
  - path: specs/tray-integration.md
    version: 2026-05-08 SPEC_REVIEWED（第 3 节 P0 简化 + P2 升级 quit_app）
  - path: specs/settings-panel.md
    version: 2026-05-08 SPEC_REVIEWED（第 3 节 退出按钮调 quit_app）
---

# ADR-010 — Lifecycle 启停步序 / 退出路径收敛 / panic hook 注册位置

> 范围：把 ADR-003 第 3.5 节"`app/lifecycle.rs` 集中管理 + 启动 7 步 / 关闭 7 步 + 退出全走 quit_app"决议落到 **可签编 struct / Phase 状态机 + 步序细化（每步失败兜底）+ panic hook 注册位置 + long-running task runtime 归属表 + shutdown grace period**。本 ADR 不重新论证 lifecycle owner 集中化方向（ADR-003 已锁），仅就两个仍有候选的子点（grace period 处理 / tokio runtime 归属确认）列选项。

---

## 1. 上下文（Context）

### 1.1 触发本 ADR 的输入

- **ADR-003 第 3.5 节** 已决"选项 B"：`app/lifecycle.rs` 暴露 `Lifecycle::start` + `Lifecycle::shutdown`；启动 7 步 / 关闭 7 步 / runtime 归属表 / 4 退出路径全走 quit_app — 但**未细化**：(a) 每步失败时是否 unwind；(b) shutdown 各步 deadline；(c) 4 退出路径在 Tauri 2 API 层的具体挂载；(d) panic hook 注册位置与 hook 内安全约束
- **ADR-008 第 7.2 节 MUST-5** + **第 6 节 fatal 三件套**（tracing::error 入文件 + dialog 兜底 + process::abort 不静默）+ **第 9 节实施提示 #5**（panic hook 在 `Lifecycle::start` step 1 之前；hook 不依赖 Tauri runtime）
- **ADR-009 第 5 节实施提示 #5** 启动构造顺序（PeerRegistry / RateLimiter / client_pool）+ **第 7.3 节 P2 反模式**（health.rs replace 前校验 banned）— lifecycle.start 必须按此顺序串起
- **5 份相关 spec** 全部 SPEC_REVIEWED：peer-heartbeat（worker 启停 + 强制重连）/ group-leave-notify（leave 时序 + 1.5s timeout）/ diagnostic-logging（tracing-appender 初始化时机）/ tray-integration / settings-panel（4 退出路径之三、之四）—— 都引用本 lifecycle 决议

### 1.2 v0 散乱 spawn 的反面教材（仅引文件路径，不复制源码）

`legacy-prototype:src-tauri/src/main.rs`（Tauri builder）+ `network/server.rs`（`start_server_if_needed` 内 spawn）+ `network/health.rs`（独立 spawn）+ `clipboard.rs`（std::thread spawn）+ `commands.rs::quit_app`（5 步序列散在命令里）—— 4 个 task 在 4 个文件各自起，**无统一启动顺序记录**；`quit_app` 仅设置面板路径调用，**托盘 quit 直接 `app.exit(0)` 不发 leave**（00 总览 + tray-integration + settings-panel + group-leave-notify 4 份 spec 第 5.4 节同议题点名）。

### 1.3 现在不决的后果

- backend-implementer 拿 ADR-009 后"PeerRegistry 在 lifecycle 哪一步实例化"无源；axum bind 失败的回退顺序无文档 → 启动半程失败留 zombie task
- 4 退出路径在 Tauri 2 API 层挂载方式各做一套 → 维护者改一处易遗漏其它（v0 教训重现）
- ADR-008 MUST-5 panic hook 注册位置无"代码契约"兜底 → implementer 把 hook 注册在 `Lifecycle::start` 内部 → runtime 死时 hook 自身依赖 Tauri 反而崩

---

## 2. 选项考虑（Options Considered）

> ADR-003 第 3.5 节已锁定"`app/lifecycle.rs` 集中管理 + 启动 7 步 + 关闭 7 步 + 4 退出路径全走 quit_app"四件方向。本 ADR 仅就两个仍有候选的子点列选项：(a) **shutdown grace period 处理**；(b) **tokio runtime 归属确认**（虽 ADR-003 表已给方向，但实施层是否真的全用 Tauri 内置 runtime 仍需明文）。其余子节（启动 7 步 / 关闭 7 步 / 4 退出路径收敛 / panic hook 注册位置 / long-running task owner 表）是 ADR-003/008/009 已决方向的细化，无可选项，直接进第 3 节。

### 2.1 Shutdown grace period 处理

> 背景：关闭路径 step 3（leave 广播）+ step 4（health/clipboard worker cancel + join）+ step 5（HTTP server graceful shutdown）+ step 6（tracing flush）每步都涉及"等多久才放弃"。group-leave-notify spec 第 3 节锁定 leave 广播 1.5s 总超时；其它步骤需要明确策略。

#### 选项 A：固定 deadline 全表（每步硬编码超时）

- 怎么做：每步独立超时（leave 1500ms / worker 500ms / clipboard 100ms / server 500ms / log 200ms）；超时即 abort 该步进入下一步；总硬上限 ≤ 2800ms
- 优点：可预测；用户感知"按了退出 ≤ 2 秒进程消失"稳定；每步失败影响隔离；implementer 照表实现无判断逻辑
- 缺点：deadline 是经验值；某步合法慢于 deadline 时被强 abort 少量数据丢
- 实现复杂度：低

#### 选项 B：自适应 grace period（按 peer 数 / queue depth 动态调整）

- 怎么做：leave 超时 = `300ms × peer_count`；worker 超时 = `pending_msg × 10ms`；log 超时 = `buffered_bytes / throughput`
- 优点：理论"刚好够用"；不浪费 deadline
- 缺点：复杂度暴增；依赖 metric（worker_pending / appender_buffered）当前 crate 不暴露；调试"为什么这次 3 秒"难推理
- 实现复杂度：高
- 否决理由：N=8 场景下选项 A 已够用；复杂度收益不匹配

#### 选项 C：仅 best-effort，不等（fire-and-forget 全部）

- 怎么做：leave 只 spawn 不 join；worker 只 abort 不 join；server 只 send 不等；log flush 跳过；100ms 后 exit
- 优点：极快（≤ 100ms 退出）
- 缺点：leave reqwest task 在 100ms 内多半还在 connect（v0 经验：leave 200-800ms 到 peer）→ 组员靠心跳兜底等 25s → 违反 group-leave-notify AC #1；log flush 跳过让 quit 路径日志丢 → 违反 diagnostic-logging AC #2
- 实现复杂度：低
- 否决理由：直接违反 group-leave-notify + diagnostic-logging 两份 spec AC

### 2.2 Tokio runtime 归属确认

> 背景：ADR-003 第 3.5 节 runtime 归属表给了方向（剪切板 std::thread / server / health / 自检 全 Tauri tokio runtime / 日志 NonBlocking guard 内置线程），但实施层是否真的全用 Tauri 内置 runtime（即 `tauri::async_runtime`）仍需明文。

#### 选项 A：全部复用 Tauri 内置 runtime（即 `tauri::async_runtime`）

- 怎么做：HTTP server / health worker / leave broadcaster 全部用 `tauri::async_runtime::spawn`；剪切板单独 std::thread；tracing-appender 用 `non_blocking` 自带 guard 线程
- 优点：与 v0 lessons-learned 4.4 节"禁止 #[tokio::main]"经验一致；不引第二个 runtime；spawn API 单一
- 缺点：health worker 与 server / handler 共享 runtime；ADR-009 第 3.4 节"RwLock 临界区禁 I/O"已规避 handler 拖累 worker
- 实现复杂度：低；跨平台风险：无

#### 选项 B：独立 tokio runtime for backend tasks

- 怎么做：lifecycle.start 内显式 `tokio::runtime::Builder::new_multi_thread().build()` 起独立 runtime（`backend-rt`），server / health / leave 全在它上面；Tauri runtime 只跑 IPC handler
- 优点：网络/后台任务与 Tauri IPC 解耦；某 handler 卡死不影响 health worker
- 缺点：v0 lessons-learned 4.4 节明文禁止类似模式（与 #[tokio::main] 冲突近邻反模式）；两 runtime 间需 channel / Arc 共享；shutdown 顺序更复杂；与 ADR-003 第 3.5 节方向相悖
- 实现复杂度：中
- 否决理由：v0 教训反向方向；ADR-003 已锁

---

## 3. 决定（Decision）

### 3.1 Lifecycle struct + Phase 状态机

```rust
// app/lifecycle.rs

use std::time::Duration;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Booting,    // start() 进行中；尚未 emit app-ready
    Running,    // start() step 7 完成；正常服务期
    Shutting,   // shutdown() 进行中；不再接受新 IPC 命令（除 quit_app 重入幂等）
    Dead,       // shutdown() 完成；进程即将 exit
}

pub struct Lifecycle {
    phase: parking_lot::RwLock<Phase>,
    // long-running handles（启动时填充；关闭时按表 cancel/join）
    server_shutdown_tx: Option<oneshot::Sender<()>>,
    health_cancel: CancellationToken,
    clipboard_cmd_tx: Option<std::sync::mpsc::Sender<ClipboardCmd>>,
    clipboard_thread: Option<std::thread::JoinHandle<()>>,
    health_task: Option<tokio::task::JoinHandle<()>>,
    server_task: Option<tokio::task::JoinHandle<()>>,
    // tracing-appender NonBlocking guard — drop 时自动 flush
    log_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

impl Lifecycle {
    pub fn new() -> Self;

    /// 7 步启动；任一步失败按 3.2 节 unwind 已起步骤后返 Err
    pub async fn start(&mut self, app: &tauri::AppHandle, state: &AppState) -> Result<(), StartupError>;
    /// 7 步关闭；幂等（重入返 Duration::ZERO）；返总耗时（审计 ≤ 2.8s 上限）
    pub async fn shutdown(&mut self, state: &AppState) -> Duration;
    pub fn phase(&self) -> Phase;
}

#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    #[error("tracing init failed: {0}")] TracingInit(String),
    #[error("config load failed: {0}")]  ConfigLoad(String),
    #[error("port bind failed: {0}")]    PortBind(String),
    #[error("clipboard thread spawn failed: {0}")] ClipboardSpawn(String),
}
```

**Phase 状态转移**：

| from | to | 触发条件 |
|---|---|---|
| Booting | Running | start() step 7 emit app-ready 成功 |
| Booting | Dead | start() 任一步失败 → unwind 后转 Dead → 返 StartupError |
| Running | Shutting | quit_app 命令调用 shutdown() 进入 step 1 |
| Shutting | Dead | shutdown() step 7 process::exit 之前 |

**重入保护**：shutdown() 入口检查 `phase() == Shutting || Dead` 直接返 Duration::ZERO（幂等）；4 退出路径任两路同时触达 quit_app（用户连点托盘 + Cmd+Q）只第一次走完整 7 步。

### 3.2 启动 7 步细化（落实 ADR-003 第 3.5 节方向 + ADR-009 实施提示 #5 顺序）

| Step | 动作 | 失败兜底 | 是否 unwind 已起 |
|---|---|---|---|
| 1 | `tracing` init + `tracing-appender::rolling` daily file appender + `non_blocking` guard 持入 lifecycle | 文件目录不可写 → 降级仅 stderr + Phase 仍进 Running（diagnostic-logging spec 第 4 节 AC #8）；非 fatal | 不 unwind |
| 2 | `Config::load()` 同步 ≤ 50ms | 文件不存在用 Default + 写盘；写盘失败 tracing::warn 后用内存 default 继续 | 不 unwind |
| 3 | 实例化 `Arc<ClientPool>` + `Arc<PeerRegistry>` + `Arc<RateLimiter>` + `Arc<History>`，挂入 AppState（参考 ADR-009 第 3.6 节 AppState struct） | 不可能失败（纯内存构造） | n/a |
| 4 | `clipboard::spawn` — std::thread 持 arboard，建 mpsc<ClipboardCmd> | std::thread::spawn 失败（极罕见 OS 资源耗尽）→ 返 ClipboardSpawn → unwind step 1（drop log_guard）| **unwind step 1** |
| 5 | `network::server::start` — axum bind 配置端口起 listen | TCP bind 失败（端口占用）→ 返 PortBind → unwind step 4（mpsc Shutdown + std::thread join 100ms 软上限）→ unwind step 1 | **unwind step 4 + 1** |
| 6 | `network::health::spawn` — 心跳 worker + 健康自检合并 task；持 `health_cancel: CancellationToken` 的 child token | spawn 不会失败（spawn 本身不返 Result）；如 worker 内首次 ping 异常仅 log warn 不阻塞 step 7 | n/a |
| 7 | emit `app-ready` Tauri 事件（前端开始调用命令） | emit 失败仅 log warn；Phase → Running | 不 unwind |

**Step 1 之前**（main / lib.rs 入口处）：注册 `std::panic::set_hook`（详见 3.5 节）。注册位置**不在 lifecycle.start 内**（runtime 死时 hook 不能依赖 Tauri）。

**Step 3 顺序（参 ADR-009）**：`Arc<ClientPool>::new()` → `Arc<PeerRegistry>::new(client_pool.clone())` → `Arc<RateLimiter>::new()` → `Arc<History>::new()`；4 个 Arc 一次塞入 `AppState`。

**Step 5 失败的 unwind**：drop clipboard_cmd_tx → 让 clipboard thread 收 mpsc disconnect 退出循环 → join 100ms 软上限；之后 drop log_guard 让 appender flush；之后返 PortBind err 给调用方。整个 unwind 过程在 ≤ 200ms 内完成。

### 3.3 关闭 7 步细化（落实 ADR-003 第 3.5 节方向 + group-leave-notify spec 第 3 节 1.5s timeout）

| Step | 动作 | Deadline | Deadline 超时处理 |
|---|---|---|---|
| 1 | `phase = Shutting` + emit `app-shutting-down` Tauri 事件（前端可显示"正在退出..."灰罩） | n/a（同步） | n/a |
| 2 | `health_cancel.cancel()` 让 health worker 退出 loop；同时 mpsc::send `ClipboardCmd::Shutdown` 让 clipboard 线程退出 | 0ms（仅信号；不等） | n/a |
| 3 | `tokio::time::timeout(1500ms, broadcast_leave(state))` — 对 peers.snapshot() 每 peer 起 spawn POST /peers/leave，外层 join_all 包 timeout | 1500ms | 超时即放弃；剩余 peer 由心跳剔除（spec 已锁，best-effort） |
| 4 | `health_task.await` + `clipboard_thread.join()` 串行收尾 | 500ms（health）+ 100ms（clipboard 软上限） | health 超 500ms abort handle；clipboard 超 100ms detach |
| 5 | `server_shutdown_tx.send(())` + `server_task.await` graceful shutdown axum | 500ms | 超时 abort handle |
| 6 | `peers.clear()`（ADR-009 第 3.3 节 clear 全部 → Unknown）；drop AppState 内 Arc<PeerRegistry> 让 PeerState 全部 drop（aes_key 触发 Zeroizing 清零） | 0ms（同步） | n/a |
| 7 | drop `log_guard` 让 tracing-appender flush 剩余 buffer；之后 `app.exit(0)` 走 Tauri AppHandle 干净退出 | 200ms（drop guard 内部超时） | 超时仍 exit；丢部分末尾日志（可接受） |

**总 deadline**：1500 + 500 + 100 + 500 + 200 = **2800ms 硬上限**；实际多数情况 leave 广播 ≤ 800ms / health 50ms / clipboard 10ms / server 50ms / log 5ms ≈ **~ 1 秒**（与 group-leave-notify spec 第 4 节 AC #1"≤ 2 秒退出"留 1s 余量）。

> **步 3 SECURITY 注释（ADR-008 A2/A3 威胁主体）**：leave 广播使用 `PeerRegistry::snapshot()` 当前快照；此时 banned peer 若刚被本机 ban 但 client_pool 尚未 remove（MUST-4 原子顺序内 ns 级窗口），可能误收到本机 leave 信号 → 让攻击者推断"本机正在退出"。group-leave-notify feature ADR 实现时必须在广播前过滤 `state.trust != Banned`（即仅向 Approved peer 发 leave）。窗口 ≤ 1500ms，信息价值 = "本机正在下线"，A2/A3 拿到该信号可在 1.5s 内提速攻击；属低危但需文档化。

**步 6 顺序锁**：clear 必须在 step 4 worker 都 join 之后做（否则 worker 还在用 PeerRegistry::snapshot 时被 clear 出 race）。

**步 3 的 leave 广播 best-effort 语义**：spec 第 3 节 / 第 4 节 AC #4 已锁"拔网线时 1.5 秒后仍 exit"+"丢即丢，靠心跳兜底"。本 ADR 只是把 timeout 从 v0 commands.rs 内部移到 lifecycle.shutdown step 3，不改语义。

### 3.4 4 退出路径收敛 — 全部唯一调 `quit_app` Tauri command

**Tauri 2 API 层挂载方式**（4 入口收敛到 `commands::group::quit_app` → `Lifecycle::shutdown`）：

| 入口 | 挂载点 | 实施细节 |
|---|---|---|
| 托盘菜单 退出 | `TrayIconBuilder::on_menu_event` 内 match `id == "quit"` | invoke `quit_app`；不直接 `app.exit(0)` |
| 设置面板 退出按钮 | 前端 `invoke("quit_app")` | 按钮 onclick 直接 invoke |
| macOS Cmd+Q | main 窗口 `on_window_event` 内 match `CloseRequested` + `api.prevent_close()` + invoke `quit_app` | macOS Cmd+Q 实际触发 main 窗口 CloseRequested |
| Windows X 关闭 | 同上 | Windows X 与 macOS Cmd+Q 在 Tauri 2 都映射为 CloseRequested |

**P0 阶段例外**（与 ADR-003 第 4.3 节 P2 升级清单 + tray-integration spec 第 3 节"P0 简化"一致）：tray P0 允许直接 `app.exit(0)` 不发 leave；P2 必须升级到 quit_app；过渡期加 `// TODO(ADR-010 第 3.4 节): upgrade to quit_app at P2` 注释。**强制观测线**：P0 tray quit 直接 `app.exit(0)` 之前必须 `tracing::warn!(target: "lifecycle", path = "tray-p0-bypass", reason = "P0 phase, leave broadcast skipped")` —— 让 prod 日志可观测出"P2 后是否仍有用户走 P0 路径"；P2 升级时 grep 该 warn 清除（ADR-008 第 6.3 节审计追溯链）。

**禁止的反模式**：
- ❌ 任何路径调 `app.exit(0)` / `std::process::exit(_)` 跳过 Lifecycle::shutdown（除 panic hook 内 `process::abort`）
- ❌ 任何路径自己写 leave 广播 + sleep 序列绕过 lifecycle.shutdown step 3
- ❌ 在 `on_window_event(CloseRequested)` 内忘记 `api.prevent_close()` 让 OS 直接退（v0 行为）

**重入幂等**（参 3.1 Phase 状态机）：用户连按托盘 quit + Cmd+Q 同时触达 quit_app；第二次 invoke 在 `Lifecycle::shutdown` 入口检查 `phase == Shutting` 即返 Duration::ZERO；前端"正在退出..."灰罩只显示一次。

### 3.5 panic hook 注册位置 + 内容（落实 ADR-008 MUST-5 + 第 6 节 fatal 三件套）

**注册位置**：`src-tauri/src/lib.rs::run` 函数最早入口（在 `tauri::Builder::default()` 之前 + 在 `Lifecycle::new()` 之前）。

```rust
// src-tauri/src/lib.rs（伪代码 — implementer 翻译为真实 Rust）

pub fn run() {
    install_panic_hook();        // ★ 必须最早；让任何后续 panic 都进文件
    // ... tauri::Builder::default().setup(|app| { lifecycle.start(...).await }).run(...)
}

fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // 1. 只取 location() + payload() 字面（ADR-008 MUST-5）
        let loc = info.location().map(|l| format!("{}:{}", l.file(), l.line()))
                                 .unwrap_or_else(|| "<unknown>".into());
        let msg: &str = info.payload().downcast_ref::<&'static str>().copied()
                            .or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str()))
                            .unwrap_or("<non-string panic payload>");

        // 2. tracing::error 进日志文件（fatal 三件套 #1）；step 1 之前 panic 时仅 stderr
        eprintln!("[FATAL] panic at {} : {}", loc, msg);
        tracing::error!(target: "panic", location = %loc, payload = %msg, "fatal panic");

        // 3. 用户可见 dialog 兜底（fatal 三件套 #2）— mac/Win cfg 隔离
        //    文案不含 msg 字面（ADR-008 第 6.1 节）
        show_native_fatal_dialog(&loc);

        // 4. process::abort 不静默（fatal 三件套 #3）
        // SECURITY (ADR-008 MUST-5): 默认 backtrace 含函数符号 + 行号，release 模式不含栈变量值；进 stderr 不进文件。已审接受面
        prev(info);     // 保留原 hook 链；让 OS / runtime 默认 backtrace 仍生效（不影响 abort）
        std::process::abort();
    }));
}

// macOS: NSAlert 或 osascript 子进程；不通过 Tauri AppHandle（runtime 可能已死）
// Windows: Win32 MessageBoxW；同上不通过 Tauri
// Linux (#[cfg(not(...))]): eprintln 兜底
// 文案统一：「Sync Copy 遇到致命错误，已写入日志：<日志路径>，请在设置 → 导出日志后联系开发者」
// 具体 API 由 implementer 在 PR 阶段决定（推荐 cocoa / windows-rs / osascript fallback）
```

**关键约束**：
1. hook **不依赖 Tauri runtime**：不调 `app.emit` / `app.dialog`；用 stderr / OS 原生 API
2. hook **只记 location + payload 字面**：不取 backtrace 栈变量值（ADR-008 第 6.1 节）；所有 `panic! / unwrap / expect` 的 message **不得含运行时变量插值**（`format!("{:?}", key)`）— code-reviewer PR 阶段 grep 检查
3. hook **dialog 文案不显示 panic message 字面**：防特定 payload 让用户截图发敏感数据（ADR-008 第 6.1 节）
4. **mac/Win cfg 隔离**：`#[cfg(target_os = "...")]` 编译期分支；Linux fallback eprintln
5. **`process::abort` 不静默**：v4-7 硬约束；不允许 `std::process::exit(0)` 静默吞 panic

### 3.6 Long-running task lifecycle owner（v5-5 强约束）

| Task | runtime | 启动者 | 取消机制 | 关闭者 |
|---|---|---|---|---|
| 剪切板轮询 + arboard 写入 | std::thread（独立 OS 线程，**不在 tokio 内**）| `Lifecycle::start` step 4 | `mpsc<ClipboardCmd>` 发 `Shutdown` 让 thread loop 自然退出 | `Lifecycle::shutdown` step 4（join 100ms 软上限，超即 detach）|
| HTTP server (axum) | `tauri::async_runtime`（Tauri 内置 tokio multi-thread；选项 A 已决） | step 5 | `oneshot::Sender<()>` send → axum `with_graceful_shutdown` | step 5 server_task.await，500ms timeout |
| 心跳 worker（peer-heartbeat health.rs）| 同 axum runtime | step 6 | `CancellationToken::cancel()` → loop 内 `tokio::select!` 监听该 token；**Shutting 阶段进入后禁止再调 `client_pool.replace`**（health worker cancel 与 client_pool drop 间的窗口期；replace 会让已 cancel 的 worker 复活；主循环每 tick 顶端必须检查 `lifecycle.phase == Shutting` 短路退出，参 ADR-009 第 5 节 #5 + ADR-008 第 5.3 节）| step 4 health_task.await，500ms timeout |
| 健康自检 worker（peer-heartbeat 隐形掉线 #2 兜底）| 同 axum runtime；与心跳合并为同一 task | step 6（合并）| 同心跳 token | 同心跳 |
| Tracing-appender flush worker | `tracing-appender::non_blocking` 内置线程（NonBlocking guard 持 JoinHandle 不暴露）| step 1 | drop guard 触发自动 flush | step 7 drop log_guard，200ms 内自动 flush |
| Leave broadcaster（关闭路径专用，不是 long-running）| 同 axum runtime | step 3（每 peer 一个 spawn task）| 外层 `tokio::time::timeout(1500ms)` 强 abort | step 3 timeout 自然结束 |

**关键不变式**：
- 每 long-running task 必须**有命名的 cancel 句柄**：`CancellationToken` / `oneshot::Sender` / `mpsc::Sender<Shutdown>`；不允许"靠 process exit 杀进程"作正常关闭
- **三套取消机制语义**：CancellationToken 用于 health/自检（多消费者，未来可派生 child token）；oneshot 用于 axum graceful shutdown（axum API）；mpsc::Shutdown 用于 std::thread（不接 tokio token）
- **runtime 唯一**（选项 A 已决）：除 std::thread + appender 内置线程外，所有 async task 在 Tauri 内置 tokio runtime；implementer 不允许在 lifecycle 外启第二个 tokio runtime

### 3.7 Shutdown grace period — 选 选项 A（固定 deadline 全表）

**决议**：每步独立 deadline 硬编码（见第 3.3 节表）；总硬上限 2800ms；实际 ~ 1 秒。

**为什么不选 B**：自适应需 metric（worker_pending / appender_buffered）当前 crate 不暴露；N=8 场景下选项 A 已够用。

**为什么不选 C**：fire-and-forget 违反 group-leave-notify AC #1 + diagnostic-logging AC #2。

**配套约束**：
- 每步 deadline 是经验值；如 prod 观察 leave 经常 timeout（>50% 触达 1500ms）→ supersede
- shutdown() 总耗时由 `Duration` 返回 + tracing 落盘
- **deadline 命中时不静默**：每步 timeout 必须 `tracing::warn!(target: "lifecycle", step, deadline_ms, actual_ms)` 落盘

---

## 4. 后果（Consequences）

### 4.1 正面

- **5 份 spec 退出路径议题一次性收敛**：tray-integration / settings-panel / group-leave-notify / peer-heartbeat / diagnostic-logging 第 5.4 节 / 第 7 节"4 处退出不一致 + task 取消 + log flush 时机"3 项议题闭环
- **ADR-009 启动顺序明文化**：PeerRegistry / RateLimiter / ClientPool 实例化在 step 3；health worker 在 step 6 校验 banned 后 replace（防 A3 zombie peer）
- **ADR-008 MUST-5 + 第 6 节 fatal 三件套落到代码契约**：panic hook 注册位置 + 内安全约束 + cfg 隔离 dialog + abort 全文档化；implementer 无解释空间
- **重入幂等 Phase 状态机**：4 路并发触达 quit_app 不让 leave + clear 重复执行（v0 race 靠用户不连点掩盖）
- **每步 deadline 可观测**：超时 tracing::warn 落盘；prod 用户报"退出慢"时能看出哪步卡

### 4.2 负面 / 妥协

- **shutdown 硬上限 2800ms 是经验值**：弱网时 leave 可能压满 1500ms；health/server 各 500ms 可能少量必要等待被强 abort；可接受（与 v0 对齐）
- **panic hook 在 release build 中 stderr 用户看不到**：dialog 是唯一可见兜底；step 1 之前 panic（tracing init 自身 panic）→ stderr + dialog 双盲；承认边界
- **process::abort 不走 Drop**：aes_key Zeroizing 不会触发清零；fatal panic 已是异常路径，残留风险与 ADR-008 MUST-2 目标场景（运行时 dump）不同，可接受。**前提**：用户未主动 `ulimit -c unlimited`（macOS / Linux）/ 未配置 Windows WerFault full memory dump；若运维要求生产开 core dump，需 supersede 本节并引入 hook 内主动 zeroize（持全局 Lazy<Arc<AppState>>）
- **接受 OS 默认 core dump / mini-dump 边界**：dev profile 默认开（macOS / Linux RLIMIT_CORE>0 / Windows WerFault 默认仅线程栈 mini-dump），release profile 默认关；用户在 prod 只看 Tauri dialog + 文件日志，不暴露内存
- **CancellationToken + oneshot + mpsc 三套取消机制并存**：implementer 需理解三种语义差异；Rust async 生态标准做法

### 4.3 需要警惕的副作用

- **panic hook 注册位置错位**：implementer 若误把 hook 注册放进 `Builder::default().setup(|app| { install_panic_hook(); ... })` 闭包内 → setup 之前的 panic（如 Tauri 自身 init panic）不被捕获。**对策**：code-reviewer grep `set_hook` 必须在 `lib.rs::run` 函数前 5 行内
- **step 5 失败的 unwind 顺序错**：unwind 必须按 step 倒序（drop clipboard_cmd_tx → join clipboard thread → drop log_guard）；先 drop log_guard 再发 Shutdown 会让退出日志丢失。**对策**：第 3.2 节表 + 单测 #2 覆盖
- **shutdown step 6 clear 在 step 4 worker join 之前**：health worker 还在调 peers.snapshot() 时被 clear out → 拿空 snapshot 误剔除。**对策**：第 3.3 节"步 6 顺序锁" + 单测 #7
- **`tracing-appender::non_blocking` 的 guard drop 顺序**：log_guard 必须是 lifecycle struct **最后字段**；字段顺序错让 tracing 在 server graceful shutdown 期间已无文件输出 → 关闭路径日志丢。**对策**：第 3.1 节字段顺序 + 单测 #8
- **macOS Cmd+Q 事件路径**：`WindowEvent::CloseRequested` 在 macOS 触发条件比 Windows 复杂（Cmd+Q 触发主菜单 Quit 非窗口 close）；implementer 在 macOS 需额外挂 `MenuEvent` 的 `quit` id 路由到 quit_app；具体 API 由 implementer 在 PR 阶段验证

---

## 5. 实施提示（≤ 5 条，给 backend-implementer）

1. **`app/lifecycle.rs` 单文件落地**（≤ 350 行硬约束，参 ADR-003 第 3.1 节）；Lifecycle struct + StartupError enum + start/shutdown/phase 三方法 + 内部 step1..step7 助手函数；不内嵌 panic hook（panic hook 在 `lib.rs::install_panic_hook` 独立函数）
2. **依赖 crate**（不引新依赖，全部 ADR-003 锁定栈内）：`tokio` (full) / `tokio_util::sync::CancellationToken`（tokio 生态，加 `tokio-util = { version = "0.7", features = ["rt"] }`）；`tracing-appender`（diagnostic-logging spec 已锁）；`thiserror`（ADR-003 第 3.6 节已用）
3. **panic hook 在 `lib.rs::install_panic_hook` 独立函数**；`run` 函数第一行调；mac/Win cfg 隔离 dialog 实施在同文件 `show_native_fatal_dialog` 函数；Linux fallback eprintln
4. **每步 deadline 命中时 tracing::warn 落盘**：fields 含 step (1-7) + deadline_ms + actual_ms；让 prod 用户报"退出慢"时开发者能 grep `target: "lifecycle"`
5. **不要做的反模式（按风险降序）**：
   - ❌ 任何路径直接调 `app.exit(0)` / `std::process::exit(_)` 跳过 Lifecycle::shutdown（除 panic hook 内 `process::abort`）
   - ❌ 在 `Lifecycle::start` 内部注册 panic hook（应在 `lib.rs::run` 最早入口；ADR-008 第 9 节实施提示 #5）
   - ❌ panic hook 内调 `tauri::AppHandle::emit` / `dialog`（runtime 可能已死；只用 stderr + OS 原生 API）
   - ❌ panic / unwrap / expect message 含运行时变量插值（ADR-008 MUST-5）
   - ❌ shutdown step 6 clear PeerRegistry 在 step 4 worker join 之前（race condition）
   - ❌ NonBlocking guard 在 lifecycle struct 字段顺序中不在最后（drop 顺序错让关闭日志丢）
   - ❌ 在 lifecycle 外启第二个 tokio runtime（v0 lessons-learned 4.4 节）
   - ❌ P0 阶段 tray quit 路径未 emit `tracing::warn!(target: "lifecycle", path = "tray-p0-bypass", ...)` 标记 bypass — 让 P2 升级时 grep 不到（ADR-008 第 6.3 节审计追溯断链）
   - ❌ health worker 在 `lifecycle.phase == Shutting` 后仍触发 `client_pool.replace` 强制重连 — 与 cancel + Drop 顺序冲突；health worker 主循环必须每 tick 检查 lifecycle.phase 短路退出（参 ADR-009 第 5 节 #5 + ADR-008 第 5.3 节）

---

## 6. 验证（How to Verify）

### 6.1 怎么证决策对（单元 + 集成测试）

**Lifecycle 单元测试 list（implementer 必备 ≥ 15 条）**：

1. `start_happy_path` — 7 步全成功；Booting → Running；emit app-ready 一次
2. `start_step5_bind_fail_unwinds_step4_and_step1` — mock TCP bind fail；clipboard mpsc 收 Shutdown；log_guard drop；返 PortBind；phase = Dead
3. `shutdown_idempotent_reentry` — 第二次调 shutdown() 返 Duration::ZERO
4. `shutdown_total_under_2800ms_happy_path` — 7 步全成功；总 Duration ≤ 2800ms
5. `shutdown_step3_leave_timeout` — mock 1 peer connect 永远 timeout；1500ms 强 abort + tracing::warn 落
6. `shutdown_step4_clipboard_join_timeout` — mock clipboard 不退；100ms detach；整体仍完成
7. `shutdown_step6_after_step4_join_no_race` — step 4 之前 spawn 调 peers.snapshot 的 task；断言 step 4 join 完后 step 6 才 clear
8. `shutdown_step7_log_guard_drop_last` — 断言 drop log_guard 在 server_shutdown_tx send 之后
9. `phase_transitions` — 4 态合法转移；非法转移（Dead → Running）panic
10. `start_step1_log_dir_unwritable_degrades_to_stderr` — mock 权限 denied；step 1 不返 err；phase 仍 Running；log_guard = None
11. `start_step2_config_missing_uses_default` — mock config.json 不存在；用 Default + 写盘；phase 仍 Running
12. `panic_hook_no_runtime_dependency` — lifecycle.start 之前 panic!；hook fire；stderr 输出 "[FATAL]"；不依赖 Tauri AppHandle
13. `panic_hook_records_only_location_and_payload` — panic!("test")；captured 仅含 "test"，无 backtrace 变量值
14. `panic_hook_dialog_message_does_not_include_payload` — dialog 文案不含 payload 字面
15. `panic_hook_calls_process_abort` — mock process::abort sentinel；断言被调

**集成测试**（与 group-leave-notify / peer-heartbeat 协同）：跨 2 机 A 端 quit_app → A ≤ 2.5 秒退出 + B 端 1 秒内 peers 减 1；A 端拔网线后 quit_app → A 仍 ≤ 2 秒退出，B 端 25 秒后心跳剔除；4 退出路径并发触达 → 仅 1 次 leave + 1 次 exit。

### 6.2 怎么证决策错（supersede 触发）

- **quit 路径平均耗时 > 2.5 秒**（>30% 触达硬上限）→ supersede 第 3.3 节 deadline 表
- **release build dialog 弹不出来**（"应用突然消失没提示"）→ supersede 第 3.5 节 mac/Win 实施
- **unwind 顺序在 PR 中再次写错** → supersede 第 3.2 节，抽 step1..7 为 builder pattern
- **Phase 仍并发执行 leave 2 次** → supersede 第 3.1 节，phase 改 atomic enum
- **总硬上限 2800ms 不够**（N ≥ 50 超 v2 范围）→ 不 supersede 本 ADR；触发 supersede ADR-003 第 4.2 节"N ≤ 8"

---

## 7. 安全审阅 (by security-reviewer · 2026-05-09)

**结论**：CHANGES_REQUESTED（4 条小补丁；非阻塞主路径，可与 implementer PR 合并落地）

### 7.1 审阅范围

- 聚焦：MUST-5 panic hook 落地 / fatal 三件套（v4-7）/ 4 退出路径不绕过审计 / shutdown grace period 安全边界 / panic 中段的 zeroize 顺序
- 已审过的方向不重复审：算法选型 / nonce / AAD 绑值 / 状态码语义（ADR-008 第 3 / 4 节）
- 未涉及新威胁主体；威胁模型沿用 ADR-008 第 2 节（A1 LAN 监听 / A2 恶意 LAN peer / A3 已被踢除的 zombie peer）+ 新增"已 banned 但仍在线的旧 peer 监听本机生命周期信号"

### 7.2 审阅意见

1. **MUST-5 panic hook 落地** — ✅ APPROVED。第 3.5 节 `install_panic_hook` 在 `lib.rs::run` 第一行调用（在 `Builder::default()` 之前 + `Lifecycle::new()` 之前），覆盖 Tauri Builder init 自身的 panic；hook 内只用 `eprintln!` + `tracing::error!` + 自定义 `show_native_fatal_dialog`（mac NSAlert / Win MessageBoxW / Linux eprintln），**不调** `app.emit` / `tauri::dialog`，runtime 死时仍可弹框；payload 仅取 `&'static str` / `String` 字面，dialog 文案不含 payload（与 ADR-008 第 6.1 节一致）。第 9 节实施提示 #5 / 第 4.3 节 grep 兜底 `set_hook` 必须在 `lib.rs::run` 前 5 行 — 闭环。
2. **fatal 三件套（v4-7）落地** — ⚠ APPROVED-with-nit。三件套 (a) `tracing::error!` (b) `show_native_fatal_dialog` (c) `process::abort()` 全到位；最末调 `prev(info)` 写默认 backtrace 进 stderr。**两个时间窗口的边界 ADR 已自承**：(i) step 1 之前 panic（tracing 未 init）→ tracing! 是 no-op，仅 eprintln + dialog；(ii) step 7 中段 panic（log_guard 已 drop / 正在 drop）→ tracing! 可能写不进文件，仅 eprintln + dialog 兜底（dialog 不依赖 tracing） — 第 4.2 节"release build stderr 用户看不到，dialog 是唯一可见兜底"已声明，可接受。**小遗憾**：`prev(info)` 调默认 hook 在 release 模式默认 backtrace 含函数符号 + 行号（不含变量值），属 ADR-008 MUST-5 已审接受面，但本 ADR 第 3.5 节伪代码未点明 → 见补丁 P1。
3. **4 退出路径不绕过审计** — ⚠ APPROVED-with-nit。4 入口收敛到 `commands::group::quit_app` ✅；macOS Cmd+Q / Win X 走 `CloseRequested` + `api.prevent_close()` + invoke quit_app ✅；重入幂等 Phase 状态机 ✅。**P0 tray 例外的 TODO 兜底机制偏弱**：仅靠注释 + code-reviewer 在 P2 PR 时 grep TODO 清除；若 implementer 漏写 TODO 字面 / 改了注释格式，例外路径可能永久化（leave 广播 + log flush 跳过的隐性退化不可观测）— 见补丁 P2。
4. **shutdown grace period 安全边界（leave 广播 / banned peer 信息泄露 / health worker cancel 顺序）** — ⚠ CHANGES_REQUESTED。
   - **(a) leave 广播对 banned peer 的信息泄露面**：第 3.3 节 step 3 `broadcast_leave` 对 `peers.snapshot()` 每 peer 起 spawn POST。ADR-009 第 3.3 节 `ban` 行为 = `inner remove + banned insert` → snapshot **不包含** banned peer，正常路径下 banned peer 不收 leave ✅。**残留窗口**：`snapshot()` 拿到副本之后、broadcast 发起之前的若干毫秒内，若另一线程 ban 某 peer（如 Shutting 期间 group-approval handler 还在处理一个晚到的恶意请求），该 peer 仍会收到 leave。窗口 ≤ 1500ms；信息价值 = "本机正在下线"；A2/A3 拿到该信号可在 1.5s 内提速攻击（如趁 server 关停瞬间发握手探测）。属低危但需文档化 — 见补丁 P3。
   - **(b) leave reqwest task 半截请求的 peer 视角**：1500ms timeout 即 abort 整个 `join_all`；spawn 出去的 reqwest task 在 abort 时若已 `connect()` 完成但 body 未写完 → peer 端看到 TCP RST / chunked 不全；这是 best-effort 语义，不泄露密钥（AAD + AES-GCM 完整性失败即拒），可接受 ✅。
   - **(c) health worker cancel 与 step 6 clear 的顺序**：step 2 cancel 信号发出 → step 4 join；中间 step 3-4 内 health worker 在 select 收 cancel 后退出 loop，但**正在飞的 reqwest 回调仍要写 PeerRegistry**。第 3.3 节"步 6 顺序锁"已锁 step 6 clear 在 step 4 join 之后 → race 已规避 ✅。但**没有覆盖**："health worker 已 cancel 但回调还没跑完时" 调 `client_pool.replace` 的路径（ADR-009 第 7.3 节 P2 反模式）—— Shutting 阶段不应再 replace 任何 peer（白浪费 1 次 X25519 握手 + 让 banned 检查窗口期延长）— 见补丁 P4。
5. **panic hook + Shutting 中段的 zeroize 顺序** — ⚠ APPROVED-with-bound。第 3.3 节 step 6 clear PeerRegistry → drop PeerState → aes_key Zeroizing 清零；正常关闭路径 MUST-2 闭环 ✅。**panic 异常路径**：若 panic 发生在 Shutting step 3-5（即 leave 已发但 step 6 未达），`process::abort()` 不走 Drop → 残留 PeerState.aes_key 在内存。第 4.2 节明确"panic-induced fast exit ≠ runtime sample dump，可接受"——技术上 OS core dump 与 sample dump 是同样的内存快照面，但默认 macOS RLIMIT_CORE=0 / Win mini-dump 仅含线程栈，**实战 panic → core dump 落地概率极低**。hook 内主动 zeroize 需引入全局 static AppState（unsafe 或 Lazy<RwLock<Option<Arc<AppState>>>>），复杂度收益不匹配。**接受架构师权衡**，但边界条件需文档化（前提：用户未主动 `ulimit -c unlimited` / 未配置 Win full mini-dump）— 已含在补丁 P1。

### 7.3 必修补丁（4 条，最小修订）

- **P1（第 3.5 节伪代码注释 + 第 4.2 节负面声明）**：(a) 在 `prev(info)` 调用前加注释一行 `// SECURITY (ADR-008 MUST-5): 默认 backtrace 含函数符号 + 行号，release 模式不含栈变量值；进 stderr 不进文件。已审接受面`；(b) 第 4.2 节 "process::abort 不走 Drop" 那段末追加一句 "前提：用户未主动 `ulimit -c unlimited`（macOS / Linux）/ 未配置 Windows Werfault full memory dump；若运维要求生产开 core dump，需 supersede 本节并引入 hook 内主动 zeroize（持全局 Lazy<Arc<AppState>>）"。
- **P2（第 3.4 节 P0 tray 例外段 + 第 5 节实施提示 #5 反模式）**：(a) P0 tray quit 直接 `app.exit(0)` 处除 TODO 注释外，**强制同步加一行** `tracing::warn!(target: "lifecycle", path = "tray-p0-bypass", "leave broadcast + log flush skipped (P0 fast-path; ADR-010 第 3.4 节)")` —— 让 prod 日志可观测出"P2 后是否仍有用户走 P0 路径"；(b) 第 5 节实施提示 #5 反模式列表追加 "❌ P0 tray quit 处仅写 TODO 注释而不写 tracing::warn 观测线"。
- **P3（第 3.3 节 step 3 注释 + 第 4.3 节副作用）**：在第 3.3 节 step 3 `broadcast_leave` 行的"超时处理"列下方追加注释段："**banned peer 视角**：snapshot 时刻已 banned 的 peer 不会收到 leave（ADR-009 第 3.3 节 ban 行为 = inner remove）；snapshot 后到 broadcast 发起前窗口期（≤ 数 ms）内被 ban 的 peer 仍会收到 leave，泄露 '本机正在下线' 信号；A2/A3 利用该信号提速攻击的窗口 ≤ 1500ms — 属低危可接受。"
- **P4（第 3.6 节 health worker 行 + 第 5 节实施提示 #5 反模式）**：(a) 第 3.6 节 long-running task 表 "心跳 worker" 行 "取消机制" 列追加一句 "**收到 cancel 后** select 内 in-flight reqwest 回调禁止再调 `client_pool.replace`（应在 worker 主循环顶端检查 `phase == Shutting` 短路）"；(b) 第 5 节实施提示 #5 反模式列表追加 "❌ health worker 在 Shutting 阶段仍调 client_pool.replace（白浪费 1 次 X25519 + 与 step 6 clear 抢占）"。

### 7.4 结论

CHANGES_REQUESTED — ADR-008 MUST-5 + 第 6 节 fatal 三件套 + ADR-009 第 7.3 节 P2 反模式在本 ADR 主决策面闭环；4 条补丁均为"在已写决议旁补一段约束注释 / 一行 tracing::warn"，不动决策本身、不增 implementer 工作面 ≤ 半小时。补丁落定后即可推 ACCEPTED。


---

## 8. 决策卡片清单（v5-11 — 让用户 5 分钟拍板）

> 仅 3.4 / 3.5 / 3.6 / 3.7 是有可选项或关键拍板点。3.1 / 3.2 / 3.3 是 ADR-003 已决方向的细化，无可选项不出卡片。

### 卡片 1 / 4 — Shutdown grace period 处理（第 3.7 节）

**问题**：shutdown 7 步的 deadline 怎么定？

**选项**：

- **A**: 固定 deadline 全表（leave 1500 / worker 500 / clipboard 100 / server 500 / log 200ms；总 ≤ 2800ms）
- **B**: 自适应 grace period（按 peer_count / queue_depth 动态调整）
- **C**: 仅 best-effort 不等（fire-and-forget，100ms exit）

**推荐**：A

**取舍**：
- A：可预测；与 v0 + group-leave-notify 1500ms 经验一致；每步 deadline 命中 tracing::warn 落盘可观测
- B：需 metric（worker_pending / appender_buffered）当前 crate 不暴露；复杂度收益不匹配
- C：违反 group-leave-notify AC #1（"≤ 2 秒退出 + 1 秒内组员看到"）+ diagnostic-logging AC #2

**must-fix**：选 A 后，每步 deadline 命中 `tracing::warn!(target: "lifecycle", step, deadline_ms, actual_ms)` 必须落盘；prod 观察 > 30% quit 触达硬上限即 supersede

### 卡片 2 / 4 — 4 退出路径 P0 例外（第 3.4 节）

**问题**：tray-integration P0 阶段是否允许托盘 quit 直接 `app.exit(0)`，P2 才升级到 quit_app？

**选项**：

- **A**: P0 阶段允许 tray quit 直接 `app.exit(0)`（带 TODO 注释），P2 升级 — 推荐
- **B**: P0 就强制全走 quit_app（更严格但阻塞 P0 实现节奏）
- **C**: P0 / P2 都允许 `app.exit(0)`（不收敛）

**推荐**：A

**取舍**：
- A：与 tray-integration spec 第 3 节 + ADR-003 第 4.3 节"P2 升级清单"已锁一致；过渡期带 TODO + code-reviewer P2 PR 时检查清除
- B：P0 需 leave + lifecycle 同时落地，节奏冲突
- C：违反 group-leave-notify AC #2

**must-fix**：选 A 后，P0 托盘 quit 代码处必须加 `// TODO(ADR-010 第 3.4 节): upgrade to quit_app at P2`；code-reviewer 在 P2 PR 时 grep TODO 清除

### 卡片 3 / 4 — Panic hook 注册位置（第 3.5 节）

**问题**：`std::panic::set_hook` 注册在哪个文件 / 哪个函数？

**选项**：

- **A**: `lib.rs::run` 函数最早入口（在 `Builder::default()` 之前 + `Lifecycle::new()` 之前）— 推荐
- **B**: `Lifecycle::start` 内部 step 0（lifecycle 集中管理）
- **C**: `Builder::default().setup(|app| { install_panic_hook(); ... })` 闭包内

**推荐**：A

**取舍**：
- A：让 Tauri Builder init 自身的 panic 也被捕获；hook 不依赖 Tauri runtime；与 ADR-008 第 9 节实施提示 #5 原文一致
- B：lifecycle.start 之前的 panic（Tauri init / bootstrap 路径）落空 → ADR-008 MUST-5 兜底失败
- C：setup 之前的 panic 完全落空；fatal 三件套不达标

**must-fix**：选 A 后，code-reviewer grep `set_hook` 必须在 `lib.rs::run` 前 5 行内；hook 内禁止调 `app.emit` / `dialog`（用 stderr + OS 原生 API + cfg 隔离）

### 卡片 4 / 4 — Long-running task tokio runtime 归属（第 3.6 节 + 第 2.2 节）

**问题**：HTTP server / health worker / leave broadcaster 用 Tauri 内置 runtime 还是独立 tokio runtime？

**选项**：

- **A**: 全部复用 Tauri 内置 runtime（`tauri::async_runtime`）— 推荐
- **B**: 独立 tokio runtime（`Builder::new_multi_thread()`）for backend tasks；Tauri runtime 只跑 IPC

**推荐**：A

**取舍**：
- A：与 v0 lessons-learned 4.4 节"禁止 #[tokio::main]"经验一致；不引第二个 runtime；ADR-009 第 3.4 节"RwLock 临界区内禁 I/O"已规避 handler 拖累 health worker
- B：v0 教训反向方向；两 runtime 间需 channel / Arc 共享；shutdown 顺序更复杂；与 ADR-003 第 3.5 节方向相悖

**must-fix**：选 A 后，code-reviewer grep `tokio::runtime::Builder` 出现位置（除 main 入口 Tauri builder 外禁止其它处出现）

---

## 9. 自查

**过度工程**：≤ 500 行硬约束已达；不重复 ADR-003 第 3.5 节方向论证；状态机仅 4 态不引入 supervisor；不引新依赖（tokio-util 在 tokio 生态非新类）；决策卡片仅 4 张（覆盖 3.4 / 3.5 / 3.6 / 3.7 真正可选点；3.1 / 3.2 / 3.3 不出卡 — ADR-003 已锁方向）。

**owner 边界**：只写 trait / struct 签名 + 状态机表 + 步序伪代码 + 单测 list；未写 .rs 实现代码；未改 spec 第 1-7 节业务范围；未改 PLAN.md；未调任何 agent；未复制 v0 源码片段。

**v5 规则镜像**（CLAUDE.md 第 14 节）：v5-3 严格 SDLC（依赖 ADR-003 + 008 + 009）；v5-4 不引新依赖；v5-5 lifecycle owner 强约束（本 ADR 即落地）；v5-9 long-running task 归属表 6 项；v5-10 三向决议（leave 1500ms 在 group-leave-notify spec / 本 ADR 第 3.3 节 / ADR-003 第 3.5 节三处一致）；v5-11 决策卡片 4 张含 问题/选项/推荐/取舍/must-fix；v5-12 章节符号禁令。

**状态机制**：PROPOSED → 可选调 security-reviewer 在第 7 节签字（如评估仅落实 ADR-008 已审条款 → 直接 ACCEPTED）→ ACCEPTED → P2-1.b 第三批 ADR-011 crypto traits 启动。
