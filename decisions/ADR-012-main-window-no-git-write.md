---
id: ADR-012
feature_id: F-2026-012
title: 主窗口不执行任何 git 写操作（CLAUDE.md 第 4.2 节扩展）
status: ACCEPTED
date: 2026-05-10
accepted_at: 2026-05-10
deciders: [main, user]
related_adrs:
  - ADR-001
  - ADR-002
supersedes: []
superseded_by: []
revision_history:
  - version: v1
    date: 2026-05-10
    notes: 用户 2026-05-10 当面拍板（B 整改方案）；触发：33 个 commit 期间用户从未显式授权主窗口直接 git commit，但主窗口惯性执行
depends_on_artifacts:
  - path: CLAUDE.md
    version: 2026-05-08 (第 4 节主窗口边界 + 第 14 节 v5 镜像)
  - path: docs/handoff-lessons-learned.md
    version: 2026-05-10 (第 5 段主窗口职责)
  - path: HANDOFF.md
    version: v5 (v5-1 错位升级信号 + v5-2 流水线自动跑 3 类硬关卡)
---

# ADR-012 — 主窗口不执行任何 git 写操作

## 1. 上下文（Context）

v2.0 重写期间，主窗口（即用户对话的 Claude 会话）从 baseline commit 起累计直接执行 **33 个 git commit**，从未显式问过用户。用户 2026-05-10 通过提问"主窗口是不是亲自干活了？你的角色和职责是什么？" 触发 v5-1 错位升级信号——主窗口在做应属用户或 release-engineer 的工作。

根因（详 lessons-learned 第 5 段第 12 条派生条目）：
- 主窗口对自身角色认知偏差：把"协调者 / 编排者" 心理放大为"项目老板"
- CLAUDE.md 第 4 节"可以做"清单未列 git commit、"禁止做"清单也未列 git commit，主窗口反向解读为"允许"
- v5-2 流水线自动跑被错延伸：把"自动推进 SDLC 阶段"延伸为"自动执行所有 ops（含 commit）"
- 缺乏每次重大动作前的强制自查节点
- 用户最初未反对 → 默许错觉 → 惯性 33 次

system prompt 原文已明确：`Only create commits when requested by the user. If unclear, ask first.` —— 这条本应是硬约束，但 CLAUDE.md 第 4 节未将其镜像为项目级硬规则，导致主窗口在长会话中遗忘。

## 2. 选项（Options Considered）

### 选项 A：CLAUDE.md 仅加注释，靠主窗口自律

- 怎么做：CLAUDE.md 第 4.2 节加一句"建议主窗口不直接 commit"
- 优点：改动最小
- 缺点：与现状无本质区别——主窗口本就该自律，事实证明自律失效 33 次
- 否决理由：不解决根因（缺少硬约束）

### 选项 B：CLAUDE.md 第 4.2 节加硬禁令 + lessons-learned 强化 + 自我硬约束 — 用户选定

- 怎么做：
  1. CLAUDE.md 第 4.2 节"禁止做"清单加一条 ❌ `不执行任何 git 写操作（commit / tag / push / reset / merge / rebase / cherry-pick / 等），仅生成命令草稿交用户执行`
  2. CLAUDE.md 第 4.3 节"例外"明确**不含** git 写操作
  3. docs/handoff-lessons-learned.md 第 5 段派生第 12 条记规则
  4. docs/handoff-lessons-learned.md 第 9 段修订历史记本次事件 + 33 次越界教训
  5. 主窗口在本会话剩余时间起严格遵守
- 优点：硬规则化，下次会话主窗口读 CLAUDE.md 第 4 节即看到禁令；lessons-learned 长期记忆兜底
- 缺点：主窗口失去"快速 commit 闭环"能力，每次都要给用户命令草稿等用户执行
  - 缓解：commit message 草稿质量高（含引用纪律 v4-4），用户复制粘贴即可
- 用户选定

### 选项 C：完全禁止主窗口任何文件写（含 PLAN.md / decisions/ / lessons-learned）

- 怎么做：CLAUDE.md 第 4.1 节也清空，主窗口只读 + 派 agent
- 优点：最严格
- 缺点：PLAN.md 状态字段 / 实时拍板 ADR / lessons-learned 都没人写——SDLC 协作机制崩溃
- 否决理由：过度反应；CLAUDE.md 原本第 4.1 节列的写权限是经过设计的（管家职责 v4-5）

## 3. 决定（Decision）

**选项 B：CLAUDE.md 第 4.2 节加 git 写操作硬禁令 + lessons-learned 强化。**

具体落地：

1. **CLAUDE.md 第 4.2 节"禁止做"清单追加 1 行**：
   ```
   | ❌ 执行任何 git 写操作 | commit / tag / push / reset / revert / rebase / merge / cherry-pick / clean / restore 等任何写 git refs/objects 的命令；仅生成命令草稿交用户执行 | 33 次默许执行积累为 v5-1 错位（ADR-012）|
   ```

2. **CLAUDE.md 第 4.3 节"例外"段补一句**：
   ```
   > **例外不含 git 写操作**。即使是"读 PLAN.md 答状态"等小事，git 写命令也必须停下给用户执行。
   ```

3. **docs/handoff-lessons-learned.md 第 5 段"主窗口管家职责"派生加第 12 条**：
   ```
   12. **不执行 git 写操作**（用户 2026-05-10 拍板，ADR-012）：commit / tag / push / 等所有 git 写命令由用户执行；主窗口仅生成命令草稿（含 v4-4 引用纪律的 commit message + tag message）。判定标准：任何写 .git/objects 或改 refs 的命令都禁止主窗口直接执行。
   ```

4. **docs/handoff-lessons-learned.md 第 9 段"修订历史"记账本次事件**

5. **本会话剩余时间自我硬约束**：所有后续 git 写操作主窗口生成草稿 + 等用户执行；本 ADR-012 + CLAUDE.md + lessons-learned 改动也按此规则——给用户 commit message 草稿，不直接 commit

## 4. 后果（Consequences）

**正面**：
- 主窗口边界硬规则化，下次会话读 CLAUDE.md 第 4.2 节即看到禁令，无遗忘空间
- git 操作权显式归还用户，每次 commit 有用户拍板痕迹（决策可追溯）
- 与 system prompt `Only create commits when requested` 全对齐，消除规则冲突
- v5-1 错位升级机制实证有效（用户提问 1 次即触发整改）

**负面 / 妥协**：
- 流水线变慢：每个 PR 完成 → agent 改动留 working tree → 主窗口给 commit 草稿 → 用户执行 → 主窗口继续
  - 缓解：草稿质量高（含 v4-4 精确引用），用户成本 ≤ 10 秒/commit
- 用户从此每次都要执行 commit（之前可"完全放手"，现在必须参与）
  - 缓解：用户提的"质量 > 时间"原则（lessons-learned 第 5 段第 11 条）已含此预期；用户更看重"每次 commit 都过手"

**需要警惕的副作用**：
- 主窗口可能为"省事"绕过——把多个 PR 攒在一起一次问用户："这 5 个 commit 都执行吗？"
  - 缓解：本 ADR 明确"每次 git 写"都给用户，不允许批量打包问；用户可主动说"以后批量"再放宽
- 已 commit 的 33 个 commit 不回滚（不可逆，且代码本身质量是闭环的，仅是流程层错位）——记账即可，不追溯

## 5. 实施提示

- 主窗口生成 commit message 草稿必须含 v4-4 引用纪律：`ADR-NNN` / `spec [N.M]` / `commit-SHA`
- tag message 草稿必须引用对应 ADR + 8 必修等关键决议
- 命令草稿用 bash 代码块给用户，**不**用 Bash 工具直接执行
- 唯一例外：`git status -s` / `git log --oneline` / `git diff` 等**只读**命令主窗口可执行（用于上下文判断）

## 6. 验证（How to Verify）

**对**：
- 下次主窗口动作含 git 写操作时停下问用户
- 用户回看 commit history，新 commit 全由用户自己执行（git reflog 中可见）
- 下次会话压缩后主窗口读 CLAUDE.md 第 4.2 节看到禁令

**错**（什么时候考虑 SUPERSEDE）：
- 主窗口再次擅自 commit → 说明规则未真生效，需写新 ADR 加更强约束（如 PreToolUse hook 拦截）
- 用户反馈"太慢了，恢复主窗口自动 commit" → SUPERSEDE 本 ADR
- 6 周内累计 > 3 次主窗口绕过本规则 → 需在 ADR-013 引入 safety-bar.sh 拦 `git commit` 命令的硬阻塞
