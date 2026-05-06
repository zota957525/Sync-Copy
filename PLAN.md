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
| P0-1 | 把当前 main 分支打成 `legacy-prototype` 分支保留 | `BACKLOG` | main + 用户 | 用户手动跑 `git branch legacy-prototype` 后我们继续 |
| P0-2 | 清空 `src-tauri/src/` 和 `src/routes/` 业务源码 | `BACKLOG` | backend-implementer + frontend-implementer | 保留 main.rs 入口 + +layout.ts + 配置文件 |
| P0-3 | 保留 `项目架构.md`、`使用说明.md` 在仓库供新版**反向参考**，但不再是真理来源 | `BACKLOG` | docs-writer | 在两份文件顶部加 banner：「v0 历史文档，新版以 specs/ 为准」 |
| P0-4 | 写 ADR-001 锁定本次重写决策、SDLC 流程、主窗口边界 | `ADR_DRAFTED` | main（用户拍板） | 已在 `decisions/ADR-001-rewrite-with-strict-sdlc.md` 落盘 |

---

## Phase 1 — 需求重新梳理（用户指定的下一步）

> 用户原话：「安排产品经理重新梳理过往决策记录，重新完善需求。」

| ID | Task | 状态 | 负责人 | 备注 |
|---|---|---|---|---|
| P1-1 | PM 阅读 `项目架构.md`、`使用说明.md`、`legacy-prototype` 分支源码、`Handoff.txt`、近 30 个 commit message，写一份 `specs/00-product-overview.md` 总览 | `BACKLOG` | product-strategist | 包含产品定位/目标用户/价值主张/不做的事 |
| P1-2 | PM 拆出 v2 的功能清单：每个功能一份 `specs/<slug>.md` | `BACKLOG` | product-strategist | 候选 slug 列表见 §候选功能清单 |
| P1-3 | PM 在每份 spec 里标注：v0 是怎么做的、有什么已知坑、v2 想怎么改 | `BACKLOG` | product-strategist | 这是"反向梳理决策"的关键产出 |
| P1-4 | PM 提出关键开放问题（待架构师/UX/安全回答） | `BACKLOG` | product-strategist | 列在每份 spec 的 §未决问题 |

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

---

## Phase 2 — 架构 + 安全决策

| ID | Task | 状态 | 负责人 | 前置 |
|---|---|---|---|---|
| P2-1 | 架构师 review 所有 specs，对每份提出关键技术决策选项，写 ADR | `BACKLOG` | tech-architect | 等 P1 完成 |
| P2-2 | 安全工程师专题 review：`e2e-encryption` / `group-approval` / `group-trust-gossip` / `peer-heartbeat` 四份 ADR | `BACKLOG` | security-reviewer | 等 P2-1 |
| P2-3 | UX 设计师 review：`floating-window` / `floating-ball` / `group-approval` / `history-list` / `settings-panel` 这几份 spec 的 UX 段 | `BACKLOG` | ux-designer | 等 P1 完成 |

---

## Phase 3 — 全员冲突排查（用户原话：「各个角色检查并解决冲突点」）

| ID | Task | 状态 | 负责人 | 输出 |
|---|---|---|---|---|
| P3-1 | 主窗口收齐所有 specs + ADRs + UX 段 + 安全审阅，整理冲突矩阵 | `BACKLOG` | main | `specs/_conflicts.md` |
| P3-2 | 每个冲突由对应 owner 在自己的 spec/ADR 里更新立场，主窗口在 `_conflicts.md` 标记仲裁结果 | `BACKLOG` | main + 各 owner | 直到所有冲突 = `RESOLVED` |
| P3-3 | 全员 specs/ADR 状态推到 `APPROVED` / `ACCEPTED` | `BACKLOG` | main | 之后才能进 Phase 4 实现 |

---

## Phase 4 — 实现

> 实现阶段每个 feature 走完整 SDLC 链路（见 CLAUDE.md §7）。

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
（实现阶段任务表会随 Phase 1/2 完成后逐步填充，此处仅占位。）

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
