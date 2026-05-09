---
id: ADR-001
title: v0 prototype 留底；main 上以严格 SDLC 重写 v2，主窗口仅做编排
status: ACCEPTED
date: 2026-05-06
deciders: [main, user]
related_specs: []
related_adrs: []
supersedes: []
superseded_by: []
---

# ADR-001 — v0 prototype 留底；main 上以严格 SDLC 重写 v2

## 1. 上下文（Context）

Sync Copy 的 v0 prototype（commit `f4be188` 及之前）已经能跑：剪切板/图片/文件三类同步、E2E 加密（X25519+AES-GCM）、组内分布式审批、悬浮球最小化、跨平台 CI 构建——功能完成度高。

但这套代码是"边写边定"的产物：
- 0 单元测试 / 0 集成测试
- 0 spec 文档（事后补的 `项目架构.md` 是反向描述，不是事前规约）
- 0 ADR 历史（每个技术决策都散落在 commit message 和会话记忆里）
- 单文件膨胀：`src/routes/+page.svelte` 1483 行、`src-tauri/src/network/server.rs` 784 行
- 多次返工的隐式不变式（如 `clipboard.rs` 的 `last_text` 在写入图片时必须置 None；`forwarded_approvals` 与 `pending_approvals` 的双 map 模式）没有任何文档说明

用户判定：再继续在这个基础上加功能，未文档化的不变式会持续被打破——决定**重写**，并在重写过程中**所有决策强制落盘**、**主窗口仅编排不实现**、**多角色分工不交叉**。

## 2. 选项（Options Considered）

### 选项 A：渐进重构（保留现有代码，逐步补齐 spec/ADR/test）
- 怎么做：把现有功能反向拆成 specs/，每个写 ADR，逐步补测试。代码不动或微调
- 优点：成本最低；保留所有踩过的坑的代码经验
- 缺点：用户明确表态"代码已经感觉不专业"，反向文档化无法清洗代码层面的问题（单文件膨胀、隐式不变式、组件耦合）；相当于"给走形的房子写说明书"
- 实现复杂度：低
- 主窗口曾推荐此方案，但用户拒绝

### 选项 B：完整重写（新代码 + 完整 SDLC + 主窗口编排）
- 怎么做：v0 留在 `legacy-prototype` 分支；main 上清空业务代码（保留配置文件、CI、依赖清单），从 spec 开始重做
- 优点：能把 v0 学到的所有教训直接编进 spec/ADR；代码结构由 ADR 决定而非堆叠演化；从此每次改动都有文档可循
- 缺点：成本高 2-3x；需要 N 个 sprint 才能回到 v0 的功能完整度
- 实现复杂度：高
- 用户选定

### 选项 C：双轨并行
- 怎么做：v0 仍在 main 跑且修 bug，v2 在 `v2-rewrite` 分支按 SDLC 推进
- 优点：v0 用户不受影响
- 缺点：sync-copy 没有真用户，双轨没必要；维护成本翻倍
- 否决

## 3. 决定（Decision）

**选项 B：完整重写。**

具体执行路径：
1. **保留 v0**：`git branch legacy-prototype`（指向当前 main HEAD `f4be188`）。任何时候可以 `git show legacy-prototype:<path>` 参考。
2. **main 清空业务代码**：移除 `src-tauri/src/*.rs`（保留 `main.rs` 入口外壳）、`src/routes/+page.svelte`、`src/routes/+layout.ts`。**保留** `Cargo.toml`、`package.json`、`tauri.conf.json`、`capabilities/`、`.github/workflows/`、`static/` 资源、`项目架构.md` 与 `使用说明.md`（标 banner 注明 v0 历史，待 v2 完工后由 docs-writer 重写）。
3. **新流程**：所有 feature 从 PM 写 spec 开始，按 CLAUDE.md 第 7 节 的 9 步 SDLC 链路推进。
4. **主窗口边界**：见 CLAUDE.md 第 4 节。主窗口不直接修改业务源码、不直接写 spec/ADR（除"用户实时拍板的决议"外）、不让 agent 之间互相调用。
5. **决策落盘**：每个非平凡技术决定 = 一份 ADR；每个 feature = 一份 spec；任务进度 = PLAN.md 状态字段。**任何"我记得我们说过 X"的论据无效；找文件出处。**

## 4. 后果（Consequences）

**正面**：
- 每个改动都有起点（spec）、有论证（ADR）、有验证（test）、有用户视角说明（docs）
- 代码结构由 ADR 决定而非堆叠演化，避免单文件膨胀
- 单人项目也具备"另一双眼睛"：code-reviewer 强制 review，security-reviewer 把守加密路径
- 决策可追溯：未来任何"为什么这么写"都能从某份 ADR 找到答案
- Claude 主窗口本身职责变清晰：只编排，不实现

**负面 / 妥协**：
- 重新做到 v0 的功能完整度需要更长时间（粗估 3-4 倍）
- 每个 feature 走 8-10 步 agent 流程，对小改动是 overkill
  - 缓解措施：lite 模式跳过 UX/安全/文档/发布，留核心 6 角色
- spec/ADR 自身可能膨胀；要靠 docs-writer 在 release 前合并/精简
- 用户需要适应"不能直接说『改一行就行』"——所有改动走流程

**需要警惕的副作用**：
- 文档跟不上代码会让 spec/ADR 变成形式主义。**对策**：每个 feature 的 PLAN.md task 不到 `RELEASED` 状态前不能开下一个；docs-writer 是必经环节
- 角色边界僵化导致协作低效。**对策**：本 ADR 在执行 3 个 feature 后由主窗口召集 retro，必要时 SUPERSEDE 调整

## 5. 实施提示

- 第一个 feature 启动前，主窗口先把当前 PLAN.md 的 Phase 0（重写准备）走完
- 第一个 feature 选小的：如 `local-ip-display`（底部 IP:PORT + 复制功能）—— 单纯前端 + 一个后端命令，能完整跑通整个 SDLC 链路而成本可控
- 跑过一个完整 feature 后，docs-writer 应回头检查整个流程是否如预期，必要时主窗口建议 SUPERSEDE 本 ADR

## 6. 验证（How to Verify）

**对**：
- 每个 commit 对应一个 PLAN.md task 状态推进
- 任何"为什么这样实现"的提问能在 ADR 里找到答案
- code-reviewer 在 review 时能逐条核对验收标准
- 三个月后回看，新人（或 Claude 自己重新进会话）只读 specs/ + decisions/ + PLAN.md 即可上手

**错**（什么时候考虑 SUPERSEDE 本 ADR）：
- 6 周后 RELEASED 状态的 task 数 < 2（说明流程过重）
- spec/ADR 与代码大幅脱节（说明 docs-writer 没起作用，需调整流程）
- 用户反复绕过 SDLC 直接要主窗口"快速改一下"——说明流程不被遵守，需要降低门槛或改回选项 A
