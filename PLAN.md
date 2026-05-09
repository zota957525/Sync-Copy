# PLAN.md — Sync Copy 任务看板

> 主窗口的"工作记忆"。所有任务、状态、阻塞、负责人都在此处。
> 任一 agent 完成工作后**必须**更新对应任务的状态字段。
> 主窗口看本文件决定下一步调谁，不依赖会话历史。

---

## 状态字段定义

| 状态 | 含义 |
|---|---|
| `BACKLOG` | 主窗口收到，待 PM 写 spec |
| `SPEC_DRAFTED` | PM 写完 spec |
| `SPEC_REVIEWED` | UX/security 已 review spec |
| `ADR_DRAFTED` | 架构师提案 ADR |
| `ADR_ACCEPTED` | ADR 通过（被主窗口/用户拍板） |
| `IMPL_IN_PROGRESS` | implementer 编码中 |
| `IMPL_DONE` | 实现完成，等评审 |
| `REVIEW_PASSED` | 代码评审通过 |
| `TEST_PASSED` | QA 测试通过 |
| `DOCS_DONE` | 文档同步完成 |
| `RELEASED` | 已发布 |
| `BLOCKED_BY_<role>` | 卡在某角色环节 |
| `SUPERSEDED` | 被另一 task 替代 |

---

## Phase 0 — 重写准备

| ID | Task | 状态 | 负责人 | 备注 |
|---|---|---|---|---|
| P0-1 | 把当前 main 分支打成 `legacy-prototype` 分支保留 | `IMPL_DONE` | main + 用户 | `git branch -a` 确认 `legacy-prototype` 已存在（指向 v0 最后 commit `f4be188`），2026-05-08 复审时归档 |
| P0-2 | 清空 `src-tauri/src/` 和 `src/routes/` 业务源码 | `IMPL_DONE` | backend-implementer | 2026-05-09 完成：删 8 个 .rs（clipboard/commands/config/crypto/history/peer/state）+ network/ 整目录（client/health/mod/protocol/server）+ 改 main.rs 为最小 Tauri 入口 + 改 lib.rs 清空业务调用 + +page.svelte 替换为最小外壳。`cargo check` + `cargo clippy -D warnings` 全 pass 0 warning。0% 自由发挥；0 越界。`legacy-prototype` 分支 `f4be188` 留底完整 |
| P0-3 | 保留 `项目架构.md`、`使用说明.md` 在仓库供新版**反向参考**，但不再是真理来源 | `BACKLOG` | docs-writer | 在两份文件顶部加 banner：「v0 历史文档，新版以 specs/ 为准」 |
| P0-4 | 写 ADR-001 锁定本次重写决策、SDLC 流程、主窗口边界 | `ADR_ACCEPTED` | main（用户拍板） | 已在 `decisions/ADR-001-rewrite-with-strict-sdlc.md` 落盘（2026-05-06） |
| P0-5 | 项目升级到 HANDOFF v5 规范（增量补丁迁移） | `IMPL_DONE` | main（用户拍板 A）| 落盘 5 件：`decisions/ADR-002-adopt-handoff-v5.md` / `specs/_assumptions.md`（PENDING_USER_REVIEW）/ `docs/handoff-lessons-learned.md`（10 段骨架）/ `CLAUDE.md` 新增 第 14 节 / `safety-bar.sh` 加 4 条新 pattern（17/17 测试通过，2026-05-08） |

---

## Phase 1 — 需求重新梳理（用户指定的下一步）

> 用户原话：「安排产品经理重新梳理过往决策记录，重新完善需求。」

| ID | Task | 状态 | 负责人 | 备注 |
|---|---|---|---|---|
| P1-1 | PM 阅读 `项目架构.md`、`使用说明.md`、`legacy-prototype` 分支源码、`Handoff.txt`、近 30 个 commit message，写一份 `specs/00-product-overview.md` 总览 | `SPEC_DRAFTED` | product-strategist | 已落盘：`specs/00-product-overview.md`（2026-05-06）。8 条项目级验收标准 + 6 条 v0 教训 + 14 个未决问题 |
| P1-2 | PM 拆出 v2 的功能清单：每个功能一份 `specs/<slug>.md` | `SPEC_REVIEWED` | product-strategist | 全 17 份 spec 已产出（2026-05-06）；架构师 ADR-003 已对 15/20 spec 做 SPEC_REVIEWED 推进（2026-05-08）|
| P1-3 | PM 在每份 spec 里标注：v0 是怎么做的、有什么已知坑、v2 想怎么改 | `SPEC_REVIEWED` | product-strategist | 同 P1-2 |
| P1-4 | PM 提出关键开放问题（待架构师/UX/安全回答） | `SPEC_REVIEWED` | product-strategist | 同 P1-2 |
| P1-5 | PM 系统性 spec 一致性 review：in scope (第 3 节) 与 open question (第 7 节 / 第 5.4 节) 必须互斥；验收标准 (第 4 节) 不固化未定方案；第 7 节 未决问题加 owner 优先级 | `SPEC_DRAFTED` | product-strategist | 全 18 份 spec（含 00-overview）一致性 review 完成（2026-05-06）。修复：类型 A 5 处（in-scope vs open 冲突）/类型 B 3 处（验收硬编码妥协）/类型 C 18 处（第 7 节 加优先级 + 顶部统计）/类型 D 2 处（内部矛盾）。主窗口指定必修 4 处全部处理；建议 6 处 4 采纳 2 调整说明。第 7 节 总分布：[P0 46 条] [P1 57 条] [P2 29 条] |
| P1-6 | PM 增量写 2 份新 spec：`diagnostic-logging`（持久化日志 + 导出）+ `clipboard-snapshot-sync`（新成员加入时自动同步当前剪切板内容）。用户 2026-05-06 提出 | `SPEC_DRAFTED` | product-strategist | 2 份新 spec 已落盘（2026-05-06）：`specs/diagnostic-logging.md`（10 条验收 + 9 条未决，priority P1）+ `specs/clipboard-snapshot-sync.md`（8 条验收 + 8 条未决，priority P1）。两份与之前 18 份合并进 P2-1 架构师 ADR 范围；spec 总数 18 → 20 |
| P1-7 | PM 应用 _assumptions 校对结果，修订 3 份 spec：① `file-transfer-drag.md`（v0 spec 原本就是 5 MB，无字面替换；改 frontmatter + 第 5.2.1 v2 修订段 + 第 7 节加非 PNG 路由议题）；② `clipboard-image-sync.md`（明确仅 PNG 走剪切板通路 + 非 PNG 转 file-transfer 兜底 + OS 光栅化边界议题）；③ `peer-heartbeat.md`（隐形掉线 1.1 段 + 3 条新 AC：强制重连 / 被动健康自检 / 上次成功同步时间字段；priority P2→P1）。用户 2026-05-08 校对 + v0 实战 bug 反馈 | `IMPL_DONE` | product-strategist | 落盘 2026-05-08：3 份 spec v2 升级；新增 3 条 [P1] [架构师] 议题汇总到 P2-1 input；`peer-heartbeat` priority P2→P1。PM 报告 0% 过度工程，0 越界 |

### 候选功能清单（PM 第一步用）

> 这是主窗口提供的初始 feature 候选；PM 在调研中可调整、合并、拆分。

- `clipboard-text-sync` — 跨设备文本剪切板同步
- `clipboard-image-sync` — 跨设备图片剪切板同步
- `file-transfer-drag` — 拖文件到浮窗发送
- `group-discovery` — 设备发现 / 加入小组
- `group-approval` — 加入审批流程（含分布式弹框 + first-responder-wins）
- `group-trust-gossip` — 信任/封禁传播
- `group-leave-notify` — 离线广播
- `peer-heartbeat` — 心跳掉线检测
- `e2e-encryption` — 端到端加密（X25519 + AES-GCM）
- `history-list` — 浮窗历史列表（最近 N 条 + 单击复制 + 删除）
- `history-sync-delete` — 跨机同步删除条目
- `floating-window` — 主浮窗外观与拖动
- `floating-ball` — 最小化为悬浮球
- `tray-integration` — 系统托盘
- `settings-panel` — 设置面板（设备名 / 端口）
- `local-ip-display` — 底部 IP:PORT 展示与复制
- `cross-platform-build` — 跨平台 CI 构建与发布
- `diagnostic-logging` — 持久化日志记录 + 导出（用户增补 2026-05-06）
- `clipboard-snapshot-sync` — 新成员加入时自动同步当前剪切板内容（用户增补 2026-05-06）

---

## Phase 2 — 架构 + 安全决策

| ID | Task | 状态 | 负责人 | 前置 |
|---|---|---|---|---|
| P2-1.a | 架构师做"项目层架构骨架决策"：模块切分 / 技术栈再确认 / HTTP 协议总骨架 / PeerState 数据模型 / 加密层抽象边界 / 错误处理与日志总策略。对应一份 ADR-003～ADR-008 区间（≤ 6 份）。**不**逐 feature 做 ADR | `ADR_ACCEPTED` | tech-architect | `decisions/ADR-003-project-architecture-skeleton.md` ACCEPTED（2026-05-08，971 行，7 子决策 + 7 张决策卡片）。用户 7/7 全选 B + ADR-008 3/3 全选 A 双签完成。15/20 spec frontmatter 加 ADR-003 引用并推 SPEC_REVIEWED；剩 5 份 spec（cross-platform-build / floating-window / floating-ball / history-list / local-ip-display）不在本 ADR 触及域，待 P2-1.b 触及时再升级 |
| P2-1.b | 架构师做 "feature 层 ADR" — 按 spec 分批，每批 3-5 份 ADR。**第一批（基础设施三件套）**：ADR-009 PeerRegistry / ADR-010 Lifecycle / ADR-011 crypto traits。用户选 A 串行（2026-05-08）。**新策略 2026-05-09**：技术细节决策卡片不上报用户（lessons-learned 第 5 段第 10 条），仅 sec CHANGES_REQUESTED 小补丁主窗口直接派 arch 落补丁 → 静默 ACCEPTED | `DONE (3/3 ACCEPTED)` | tech-architect | **第一批基础设施三件套全部 ACCEPTED**（2026-05-09）：ADR-009 v1.2（560 行 / sec CHANGES_REQUESTED 4 补丁已落）/ ADR-010 v1.2（539 行 / sec CHANGES_REQUESTED 4 补丁已落 / 新策略首例静默运行）/ **ADR-011 v1.2（502 行 / sec APPROVED 0 补丁，项目最关键加密 ADR 一次过）**。第二批 feature ADR + 实现阶段解锁 |
| P2-1.c | 实现阶段第一个 PR（基础设施落地）：把 ADR-009 / ADR-010 / ADR-011 三件套从 trait 签名 + 状态机 + 单测清单 落到 src-tauri/src/peer / app/lifecycle / crypto 三个 module 的实际 Rust 代码 + ≥ 18 条单测（PeerRegistry 7 + Lifecycle 5 + crypto 6） | `IN_PROGRESS (1/3 PR-1 REVIEW_PASSED, 2/3 待启)` | backend-implementer | **PR-1（crypto module）REVIEW_PASSED（2026-05-09）**：commit b3382cb 落 ADR-011 三件套 + 18 单测；code-reviewer **APPROVED 0 必修 3 低级 nit**（5 聚焦点全 ✅）；review 段在 `specs/e2e-encryption.md` 第 8 节。MUST-1/2/5 全闭环。**等 PR-2（PeerRegistry, ADR-009）启动** |
| P2-1（旧条目，整体由 P2-1.a/.b 替代） | 架构师 review 所有 specs，对每份提出关键技术决策选项，写 ADR | `SUPERSEDED` | tech-architect | 由 ADR-002 自动跑流水线规则拆分为 .a / .b 子任务（2026-05-08）|
| P2-2 | 安全工程师专题 review：ADR-003 第 3.4（加密 trait）/ 3.6（错误日志）/ 3.7（隐形掉线 + client_pool）三节，出 ADR-008 或在 ADR-003 第 7 节追加签字。审 7 节列出的 10 条待审项（AAD 绑值 / zeroize / PSK / content_hash → HMAC / filename sanitize / size early validate / handshake DoS / device_name 字符集 / /ping origin 校验 / 日志敏感字段细化）+ 11/12 条评审 checklist。后续 feature ADR（e2e-encryption / group-approval / group-trust-gossip / peer-heartbeat）留到 P2-1.b 阶段时按需追加 | `IMPL_DONE` | security-reviewer | `decisions/ADR-008-security-review-of-adr003.md` 已落盘 ACCEPTED（2026-05-08，687 行）。结论 CHANGES_REQUESTED：方向 APPROVED + 8 必修（5 项目层 + 3 feature 层）+ 3 不必修（PSK / /ping origin / HMAC 全组 epoch key 演进）。1 严重发现（/file 缺 seq dedupe，重放 file-pending 弹框）+ 11 中危 + 3 低危。3 张确认卡片待用户最终拍板 |
| P2-3 | UX 设计师 review：`floating-window` / `floating-ball` / `group-approval` / `history-list` / `settings-panel` / `file-transfer-drag` 6 份 spec 的 UX 段 | `SPEC_DRAFTED` | ux-designer | UX 段已填（2026-05-06）。6 份 spec 第 6 节全部补全；视觉语言字典定义在 floating-window.md 第 6.5 节；共回答 PM 标 [UX] 的 open question 14 条；每份 spec 含 6.8 节反馈。 |
| P2-3.b | UX 设计师二次入场：补 P1-6 增量 2 份新 spec 的 UX 段（`diagnostic-logging` + `clipboard-snapshot-sync`），引用 P2-3 已定的视觉语言字典 | `SPEC_DRAFTED` | ux-designer | UX 段已填（2026-05-06）。diagnostic-logging 第 6 节：8 小节，含 DBG 角标设计、导出 loading 态、settings-panel 布局扩展说明；clipboard-snapshot-sync 第 6 节：8 小节，含 toast 策略决策、snapshot 条目与 remote 条目视觉一致性决策、异常静默策略；共回答 PM 标 [UX] 的 open question 2 条。 |

---

## Phase 3 — 全员冲突排查（用户原话：「各个角色检查并解决冲突点」）

| ID | Task | 状态 | 负责人 | 输出 |
|---|---|---|---|---|
| P3-1 | 主窗口收齐所有 specs + ADRs + UX 段 + 安全审阅，整理冲突矩阵 | `BACKLOG` | main | `specs/_conflicts.md` |
| P3-2 | 每个冲突由对应 owner 在自己的 spec/ADR 里更新立场，主窗口在 `_conflicts.md` 标记仲裁结果 | `BACKLOG` | main + 各 owner | 直到所有冲突 = `RESOLVED` |
| P3-3 | 全员 specs/ADR 状态推到 `APPROVED` / `ACCEPTED` | `BACKLOG` | main | 之后才能进 Phase 4 实现 |

---

## Phase 4 — 实现

> 实现阶段每个 feature 走完整 SDLC 链路（见 CLAUDE.md 第 7 节）。

按优先级排序的实现顺序（建议，可调整）：

1. `cross-platform-build` — 先把 CI 跑通，方便后面每次改动都过 CI
2. `floating-window` + `tray-integration` — 最小可见的 UI
3. `clipboard-text-sync` + `e2e-encryption` + `group-discovery` + `group-approval` — MVP 核心闭环
4. `history-list`
5. `local-ip-display` + `settings-panel`
6. `clipboard-image-sync`
7. `file-transfer-drag`
8. `floating-ball`
9. `group-trust-gossip` + `group-leave-notify` + `peer-heartbeat`
10. `history-sync-delete`

每个 task 在这里以 `P4-<n>` 形式独立行展示，状态 BACKLOG → IMPL_IN_PROGRESS → ... → RELEASED。

### P0 实现 backlog（8 个 feature，已有 spec 待 ADR）

| ID | Slug | 状态 | 当前 ADR | 备注 |
|---|---|---|---|---|
| P4-1 | cross-platform-build | `BACKLOG` | (待 ADR) | spec: `specs/cross-platform-build.md` |
| P4-2 | floating-window | `BACKLOG` | (待 ADR) | spec: `specs/floating-window.md` |
| P4-3 | tray-integration | `BACKLOG` | (待 ADR) | spec: `specs/tray-integration.md` |
| P4-4 | local-ip-display | `BACKLOG` | (待 ADR) | spec: `specs/local-ip-display.md` |
| P4-5 | group-discovery | `BACKLOG` | (待 ADR) | spec: `specs/group-discovery.md` |
| P4-6 | e2e-encryption | `BACKLOG` | (待 ADR) | spec: `specs/e2e-encryption.md` |
| P4-7 | group-approval | `BACKLOG` | (待 ADR) | spec: `specs/group-approval.md` |
| P4-8 | clipboard-text-sync | `BACKLOG` | (待 ADR) | spec: `specs/clipboard-text-sync.md` |

### P1 实现 backlog（5 个 feature，已有 spec 待 ADR）

| ID | Slug | 状态 | 当前 ADR | 备注 |
|---|---|---|---|---|
| P4-9 | clipboard-image-sync | `BACKLOG` | (待 ADR) | spec: `specs/clipboard-image-sync.md` |
| P4-10 | file-transfer-drag | `BACKLOG` | (待 ADR) | spec: `specs/file-transfer-drag.md` |
| P4-11 | history-list | `BACKLOG` | (待 ADR) | spec: `specs/history-list.md` |
| P4-12 | settings-panel | `BACKLOG` | (待 ADR) | spec: `specs/settings-panel.md` |
| P4-13 | floating-ball | `BACKLOG` | (待 ADR) | spec: `specs/floating-ball.md` |

### P2 实现 backlog（4 个 feature，已有 spec 待 ADR）

| ID | Slug | 状态 | 当前 ADR | 备注 |
|---|---|---|---|---|
| P4-14 | group-trust-gossip | `BACKLOG` | (待 ADR) | spec: `specs/group-trust-gossip.md` |
| P4-15 | group-leave-notify | `BACKLOG` | (待 ADR) | spec: `specs/group-leave-notify.md` |
| P4-16 | peer-heartbeat | `BACKLOG` | (待 ADR) | spec: `specs/peer-heartbeat.md` |
| P4-17 | history-sync-delete | `BACKLOG` | (待 ADR) | spec: `specs/history-sync-delete.md` |

### 用户增补（2026-05-06，待 PM 完成 P1-6 后归类到对应优先级）

| ID | Slug | 状态 | 当前 ADR | 备注 |
|---|---|---|---|---|
| P4-18 | diagnostic-logging | `BACKLOG` | (待 ADR) | spec: `specs/diagnostic-logging.md`（priority P1） |
| P4-19 | clipboard-snapshot-sync | `BACKLOG` | (待 ADR) | spec: `specs/clipboard-snapshot-sync.md`（priority P1） |

---

## Phase 5 — 测试与发布

> 各 feature 在 Phase 4 中已分别走完测试/文档/发布；Phase 5 是整体集成测试 + v2.0.0 release。

| ID | Task | 状态 | 负责人 |
|---|---|---|---|
| P5-1 | QA 写 `tests/integration-checklist.md` 包含 2 台 / 3 台双机/三机集成场景 | `BACKLOG` | qa-tester |
| P5-2 | 跨平台手测（Mac×2, Win×1） | `BACKLOG` | 用户 + qa-tester |
| P5-3 | docs-writer 重写 `项目架构.md` 和 `使用说明.md` 为 v2 版本 | `BACKLOG` | docs-writer |
| P5-4 | release-engineer 升 v2.0.0、tag、release notes、CI artifact 验证 | `BACKLOG` | release-engineer |

---

## 当前阻塞

- 等用户确认本 PLAN.md 后开始 Phase 0
- 等用户回答：是否同意 P0-1 的 `legacy-prototype` 分支策略？

---

## 给主窗口的下一步建议

1. 用户确认本 PLAN 后，主窗口立即调用 `product-strategist` 启动 P1-1（写 `specs/00-product-overview.md` 总览）
2. P1-1 完成后，主窗口写 PLAN.md 把 P1-1 状态改 `SPEC_DRAFTED`，并把 P1-2 状态改 `BACKLOG`（待开工），然后回报用户进展，等用户决定下一步是继续 P1-2 还是先 review P1-1
