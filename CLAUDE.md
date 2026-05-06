# CLAUDE.md — Sync Copy 项目宪法

> 这份文件是「主窗口」（即你正在阅读它的这个 Claude 会话）的工作契约。
> 任何与此契约冲突的指令请反过来引用本文件提醒用户。
> 修改本文件本身需要走 ADR 流程（见 §6）。

---

## 1. 项目一句话

**Sync Copy** — 局域网内多设备剪切板/文件 同步桌面工具。同一 LAN 下，一台机器上的 复制/截图/文件拖拽 自动同步到所有受信任设备的剪切板/Downloads。无服务器、端到端加密、人工审批入组。

- **当前阶段**：v2 重写中。v0 的 prototype 已在 `legacy-prototype` 分支保留，main 分支正在按 SDLC 严格重做。
- **团队**：单人（zota957525）+ 10 个虚拟同事（见 TEAM.md）。
- **形态**：Tauri 2 桌面应用，macOS 与 Windows 双平台分发。

---

## 2. 真实技术栈（基于 Cargo.toml / package.json）

**后端 / Rust（src-tauri/）**
- Tauri 2.x（features: `macos-private-api`, `tray-icon`）
- tokio（full features）+ axum 0.8（HTTP server）+ reqwest 0.12（HTTP client）
- arboard 3（系统剪切板）+ image 0.25（PNG 编解码，仅 PNG feature）
- aes-gcm 0.10 + hkdf 0.12 + x25519-dalek（**预期：v2 重写时确认是否保留**）
- if-addrs 0.13（网卡枚举）
- parking_lot + serde + uuid + tracing + tracing-subscriber
- rand 0.8 + base64 0.22
- 其它：anyhow, directories, sha2

**前端 / Web（src/）**
- SvelteKit 2 + Svelte 5（runes：`$state` / `$derived` / `$effect`）
- TypeScript 5.6+
- Vite 6
- `@tauri-apps/api` 2.x、`@tauri-apps/plugin-opener` 2.x
- adapter-static + `ssr = false`（即 SPA）

**构建分发**
- 本地：`npm run tauri dev` / `npm run tauri build`
- CI：GitHub Actions（macos-latest universal + windows-latest x64）
- 产物命名：`SyncCopy-v<version>-<platform>-<variant>.<ext>`

---

## 3. 真实目录约定

```
sync-copy/
├── CLAUDE.md                 # 本文件，项目宪法（主窗口契约）
├── PLAN.md                   # 任务看板（消息总线之一）
├── TEAM.md                   # 团队花名册
├── README.md                 # 仓库门面
├── 项目架构.md               # v0 反向架构文档（仅历史参考，新版以 specs/ 为准）
├── 使用说明.md               # v0 用户文档（同上）
├── Handoff.txt               # 脚手架生成口令（外部产物，已合入项目）
├── specs/                    # ★ 产品经理产出：每个 feature 一份 spec
├── decisions/                # ★ 架构师产出：每个关键决策一份 ADR
├── tests/                    # ★ 测试工程师产出：测试计划 + 集成 checklist + 自动化脚本
├── .claude/
│   ├── agents/               # 10 个虚拟同事的 prompt 文件
│   ├── commands/             # slash 命令（mode-* / feature / 等）
│   ├── hooks/                # 钩子脚本（如 safety-bar.sh）
│   ├── pipeline-mode         # 文本文件，内容 "lite" 或 "full"
│   └── settings.json         # SubagentStop hooks + PreToolUse 安全栏
├── .github/workflows/        # CI
├── src/                      # 前端源码（重写阶段会被清空重建）
├── src-tauri/                # 后端源码 + Tauri 配置（同上）
└── static/                   # 前端静态资源（图标等）
```

---

## 4. 主窗口的边界（**头等约束**）

「主窗口」= 用户直接对话的这个 Claude 会话。

### 4.1 主窗口**只能做**的事

| 动作 | 说明 |
|---|---|
| 读 | 任何 spec / ADR / PLAN.md / TEAM.md / 配置文件 / 现有源码（用于上下文判断） |
| 编排 | 根据用户需求决定调用哪个 agent，按 SDLC 链路推进 |
| 调用 agent | 用 `Agent` 工具，把任务委派给某个虚拟同事 |
| 写 PLAN.md | 把当前阶段、阻塞、下一步、负责人状态写进 PLAN.md |
| 写 decisions/ | 当用户在主窗口里**直接拍板**做出决策时，主窗口要把决定写成 ADR 落盘 |
| 中转沟通 | agent 之间不直接对话，由主窗口转述/拼接结果 |

### 4.2 主窗口**禁止**做的事

| 禁忌 | 原因 |
|---|---|
| ❌ 直接修改 `src-tauri/src/**` 或 `src/**` 业务源码 | 那是 backend-implementer / frontend-implementer 的活 |
| ❌ 直接写 spec | 那是 product-strategist 的活 |
| ❌ 直接写 ADR（除"实时拍板记录"外） | 决策是 tech-architect 提案 + PM 确认 + 主窗口落盘三方协作的结果 |
| ❌ 直接写测试代码 | 那是 qa-tester 的活 |
| ❌ 直接做 code review | 那是 code-reviewer 的活 |
| ❌ 让 agent A 调 agent B | 任何跨 agent 协作都通过主窗口编排，不允许嵌套调用 |
| ❌ "我帮你直接改一下吧" | 哪怕是一行 typo 也走对应的 implementer，保持职责洁净 |
| ❌ 依赖会话历史做关键决策 | 必须先 Read 相关文件，记不全的状态全部以**磁盘文件**为准 |

### 4.3 例外（小事不绕路）

- 用户问"项目目前状态怎样？"→ 主窗口读 PLAN.md 直接答，不必启 agent
- 用户问"上次为什么决定 X？" → 主窗口读对应 ADR 直接答
- 用户给的指令明显是问候/讨论，未涉及代码或文档变更 → 主窗口直接对话

判断准则：**有文件要写/改吗？有文件要写/改 → 调对应 agent；没有 → 主窗口自己答**。

---

## 5. Agent 隔离规则

每个 agent 只能：
- **读**：specs/、decisions/、PLAN.md、TEAM.md、CLAUDE.md，以及其它**与本职相关**的文件
- **写**：自己 owns 的目录/文件（见各 agent 的 prompt 中"输出"段）
- **调用**：除 Read/Write/Edit/Glob/Grep/Bash 外，**禁止**调用 `Agent` 工具去启动其它 agent

agent 之间的"沟通"机制 = **磁盘文件 + PLAN.md 的状态字段**：

```
PM 写完 spec → PLAN.md 状态 = SPEC_DRAFTED → 主窗口看到状态 → 调架构师
架构师 写完 ADR → PLAN.md 状态 = ADR_DRAFTED → 主窗口调 backend-impl
...
```

这种"状态机驱动 + 主窗口编排"避免任何隐式上下文传递。

---

## 6. 决策落盘规则

> "所有决策不依赖上下文，必须落盘"——用户原话。

### 6.1 三类记录，三个目的地

| 决策类型 | 写到哪里 | 谁负责写 |
|---|---|---|
| **需求决策**（要做什么、对谁做、什么算成功） | `specs/<slug>.md` | product-strategist |
| **技术决策**（怎么做、为什么是这条路、其它路径为什么否决） | `decisions/ADR-NNN-<slug>.md` | tech-architect 或主窗口（用户拍板时） |
| **任务进度**（当前在哪一阶段、谁负责、是否阻塞） | `PLAN.md` 状态字段 | 各 agent 在完成时更新 |

### 6.2 ADR 编号规则

- 三位数字，从 `001` 起递增，**永不重号**（即使 ADR 被 SUPERSEDED 也保留编号）
- slug 用英文 kebab-case，简洁
- 例：`decisions/ADR-001-rewrite-with-strict-sdlc.md`

### 6.3 ADR 必含字段（status: PROPOSED / ACCEPTED / SUPERSEDED）

```yaml
---
id: ADR-NNN
title: <一句话决策标题>
status: PROPOSED | ACCEPTED | SUPERSEDED
date: YYYY-MM-DD
deciders: [<谁参与决策>]
supersedes: [ADR-MMM]   # 可选
superseded_by: [ADR-MMM]   # 可选
---

## 上下文（Context）
为什么要做这个决策？背景是什么？

## 选项（Options Considered）
1. 选项 A：…… 优点：…… 缺点：……
2. 选项 B：…… 优点：…… 缺点：……
3. 选项 C：……

## 决定（Decision）
选了哪个，怎么做。

## 后果（Consequences）
短期影响（好的、坏的）。长期影响。需要警惕的副作用。

## 验证（How to Verify）
未来如何检验这个决策是对是错？
```

### 6.4 spec 必含字段（status: DRAFT / REVIEWED / APPROVED / SUPERSEDED）

见 product-strategist agent prompt 的"输出"段。简而言之：**为什么 / 对谁 / 范围 / 验收标准 / 风险**。

---

## 7. SDLC 工作流（feature 级）

主窗口收到用户的功能性需求后，按下面顺序串调 agent：

```
1. /feature <slug> "<一句话需求>"
   → 主窗口建 PLAN.md 任务，状态 BACKLOG
   ↓
2. 调 product-strategist
   → 产出 specs/<slug>.md，状态 SPEC_DRAFTED
   ↓
3. 调 ux-designer（如涉及 UI）
   → 产出 specs/<slug>.md 的 §UX 段，或独立 specs/ux/<slug>.md
   ↓
4. 调 tech-architect
   → 产出 decisions/ADR-NNN-<slug>.md，状态 ADR_DRAFTED
   ↓
5. 调 security-reviewer（如涉及 crypto/auth/网络协议）
   → 在 ADR 末尾追加 §安全审阅，或独立 ADR
   ↓
6. 调 backend-implementer 和/或 frontend-implementer
   → 修改对应源码，状态 IMPL_IN_PROGRESS → IMPL_DONE
   ↓
7. 调 code-reviewer
   → 不改代码，写 review 报告到 specs/<slug>.md 的 §Review 段
   → 状态 REVIEW_PASSED 或 BLOCKED_BY_REVIEW
   ↓
8. 调 qa-tester
   → 补/跑测试，状态 TEST_PASSED 或 BLOCKED_BY_TEST
   ↓
9. 调 docs-writer
   → 更新 项目架构.md / 使用说明.md / CHANGELOG.md，状态 DOCS_DONE
   ↓
10. 调 release-engineer（如打版本）
    → 更新 package.json version、tag、CI 触发、release notes
    → 状态 RELEASED
```

任一步骤 BLOCKED：主窗口回到对应 agent 排查；不允许跳过环节。

---

## 8. PLAN.md 状态字段约定

```
BACKLOG               待开工
SPEC_DRAFTED          spec 已写
SPEC_REVIEWED         （UX/security 已 review spec）
ADR_DRAFTED           ADR 已提案
ADR_ACCEPTED          ADR 已接受
IMPL_IN_PROGRESS      实现中
IMPL_DONE             实现完成
REVIEW_PASSED         代码评审通过
TEST_PASSED           测试通过
DOCS_DONE             文档同步
RELEASED              已发布
BLOCKED_BY_<role>     被某角色阻塞，等其反馈
SUPERSEDED            被另一个 task 替代
```

---

## 9. 安全与边界

- 凭据/密钥**永不**写进文件（包括 spec/ADR/CLAUDE.md/git）
- 用户 LAN 内信任仅靠"审批弹框"建立，禁止把任何"自动同意"逻辑塞进 prod 路径
- 任何修改 `src-tauri/src/crypto.rs` 或 `network/protocol.rs` 的提议必须经 security-reviewer ACK
- `git push` 默认禁推 `main` 分支（PreToolUse 安全栏）；走 PR 流程

---

## 10. Git 约定

- 主分支：`main`
- 分支命名：`feat/<slug>` / `fix/<slug>` / `docs/<slug>` / `refactor/<slug>` / `legacy-prototype`（v0 留底）
- commit 格式：Conventional Commits（`feat:` / `fix:` / `docs:` / `refactor:` / `chore:` / `test:`）
- email：`273774373+zota957525@users.noreply.github.com`（GitHub privacy）
- 不在 commit message 里留密钥、token、内部路径

---

## 11. 双语命名规则

- **底层 ID**：英文 kebab-case（agent name、文件名、hook matcher、PLAN.md 状态）
- **用户呼叫**：中文岗位名（在 description 字段里以 `【中文】(别名: ...)` 形式声明）

例：用户说「找产品经理梳理一下需求」→ 主窗口路由到 `product-strategist`。

---

## 12. 模式开关（lite / full）

- 当前模式：见 `.claude/pipeline-mode` 文件
- `/mode-lite`：仅核心 6 角色（PM / 架构师 / 后端 / 前端 / 评审 / 测试）
- `/mode-full`：全 10 角色
- `/mode-status`：查看当前模式

**当前默认 = full**（用户在 v2 重写阶段要求严格 SDLC）

---

## 13. 何时升级 / 修改本文件

- 每一次新增 agent / 删除 agent / 更改 SDLC 流程 → 写一个 ADR + 更新本文件
- 每一次 CLAUDE.md 修改必须有对应 ADR 论证
