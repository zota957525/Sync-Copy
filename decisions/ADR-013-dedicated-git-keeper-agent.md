---
id: ADR-013
feature_id: F-2026-013
title: 创建专属 git-keeper agent，仅由用户调用执行 git 写操作
status: ACCEPTED
date: 2026-05-14
accepted_at: 2026-05-14
deciders: [main, user]
related_adrs:
  - ADR-001
  - ADR-012
supersedes: []
superseded_by: []
revision_history:
  - version: v1
    date: 2026-05-14
    notes: 用户 2026-05-14 直接拍板。触发：ADR-012 落盘后主窗口仍生成 commit 草稿交用户执行——用户希望进一步治理边界，git 操作有专属责任主体且只对用户负责
depends_on_artifacts:
  - path: decisions/ADR-012-main-window-no-git-write.md
    version: 2026-05-10 (ACCEPTED)
  - path: CLAUDE.md
    version: 2026-05-10 (第 4 节主窗口边界 + 第 5 节 agent 隔离)
  - path: docs/handoff-lessons-learned.md
    version: 2026-05-10 (第 5 段主窗口职责 + 第 12 条 git 写禁令)
---

# ADR-013 — 专属 git-keeper agent，仅由用户调用

## 1. 上下文（Context）

ADR-012（2026-05-10）落盘后，主窗口生成 git 命令草稿交用户终端执行——这解决了"主窗口擅自 commit" 的问题，但用户希望进一步治理：

**用户原话（2026-05-14）**："专门有一个 agent 负责 git，并且直接听命于我，只有我明确下达命令才可以执行。"

设计动机：
- ADR-012 模式下，git 操作的执行权在用户终端，主窗口生成草稿——但主窗口仍是"草稿起草人"，未来若主窗口"代替用户决定何时 commit"仍可能错位
- 更彻底的治理：**git 写操作有专属责任主体（git-keeper agent）+ 直接听命于用户**（不通过主窗口编排）+ **主窗口禁止调用**
- 与 release-engineer 边界明确：
  - release-engineer = 准备者（升 version / 写 RELEASE_NOTES / 给 tag 建议 / CI 配置）
  - git-keeper = 执行者（实际跑 commit / tag / push 等 git 写命令）
  - **两者都不直接执行 git 写之前**——release-engineer 仍只生成命令建议
- 主窗口不再是"git 草稿起草人"——草稿由 release-engineer / 主窗口提议，但执行只走 git-keeper，且**只在用户显式命令时**

## 2. 选项（Options Considered）

### 选项 A：维持 ADR-012 现状（主窗口给草稿，用户终端执行）

- 怎么做：每次主窗口生成 commit/tag/push 命令清单，用户复制粘贴到终端
- 优点：已生效，无需新 agent / 新规则
- 缺点：
  - 主窗口仍是"草稿决策人"——决定何时 commit / commit message 内容 / tag 时机
  - 用户每次复制粘贴到终端，跨进程切换成本（终端 vs Claude Code 会话）
  - 若用户希望"在会话内由 agent 执行 git 但仍要我授权"——A 方案不支持
- 否决理由：用户已明确要求进一步治理

### 选项 B：新建 git-keeper agent，主窗口禁止调用，仅用户直接调用 — 用户选定

- 怎么做：
  - `.claude/agents/git-keeper.md` 新建，tools: Bash + Read（只能跑 git 命令 + 读 message 文件）
  - 用户在 prompt 中**明确**说"用 git-keeper commit X" / "@git-keeper 打 tag" / "调 git-keeper push" 才触发
  - 主窗口**禁止**主动调用 git-keeper（即使任务需要 commit，主窗口仍走"生成草稿+ 等用户调 git-keeper"流程）
  - git-keeper 收到主窗口（非用户）派单 → 报告"未收到用户显式命令，已停下"，不执行
  - CLAUDE.md 第 4.2 节"禁止做"清单加 ❌ 调用 git-keeper agent
  - CLAUDE.md 第 5 节"agent 隔离规则"加补充：git-keeper 仅由用户调用
- 优点：
  - git 写操作有专属责任主体——审计清晰（git-keeper 是唯一执行入口）
  - 主窗口与 release-engineer 都不能"顺手" git 操作
  - 用户保留 100% git 控制权——每次执行都有显式命令痕迹
  - 治理边界从"代码层（ADR-012）"升级到"角色层（ADR-013）"——比 ADR-012 更彻底
  - 与 ADR-012 互补：ADR-012 禁主窗口直接执行；ADR-013 禁主窗口编排其他 agent 间接执行
- 缺点：
  - 流程更长：主窗口编排 → 主窗口给 commit 草稿 → 用户审阅 → 用户在 Claude Code 中说"用 git-keeper commit" → git-keeper 执行
    - 缓解：用户可在会话内直接说"用 git-keeper commit 当前 working tree"，比终端复制粘贴更顺
  - 主窗口可能违规——擅自调 git-keeper 试图绕过
    - 缓解：git-keeper 自身 prompt 含"检测是否来自用户显式命令"段，主窗口派单时报告错位
  - 新 agent 增加 .claude/agents/ 数量（10 → 11）
    - 缓解：ux-designer 本 v2.0 未调用，实际常驻 agent 仍 ≤ 10
- **用户选定**

### 选项 C：扩展 release-engineer 含 git 执行能力

- 怎么做：release-engineer 的 tools 加 Bash 执行权 + 允许跑 git commit/tag/push
- 优点：不新建 agent
- 缺点：
  - release-engineer 职责单一性被破坏（"准备者"变成"执行者"）
  - release-engineer 当前可被主窗口调用——给 release-engineer git 执行权 = 主窗口间接获得 git 执行权（绕过 ADR-012）
  - 与用户"专属 agent + 直接听命于我"要求矛盾
- 否决理由：违反职责单一 + 与用户意图相悖

## 3. 决定（Decision）

**选项 B：新建 git-keeper agent，仅由用户调用，主窗口禁止调用。**

具体落地：

### 3.1 创建 `.claude/agents/git-keeper.md`

- name: `git-keeper`
- description: 【git 操作员】(别名: git、git-agent、git-keeper)。仅听命于用户，负责所有 git 写操作（commit / tag / push / reset / revert / rebase / merge / cherry-pick）。**主窗口禁止调用**。
- tools: `Read, Bash`（只读 + git 命令执行；不需要 Write/Edit）
- model: `sonnet`（任务清晰，不需要 opus 复杂推理）
- 7-section 结构含：输入 / 输出（git refs） / 工作流程 / 严格禁止 / 过度工程自查 / owner 边界自查 / 引用项目规则 / 完成报告
- 关键自检：每次收到派单先验证是否含"用户显式命令"标记；若来自主窗口或其他 agent → 拒绝执行 + 报告错位

### 3.2 更新 CLAUDE.md

- **第 4.2 节"禁止做"清单**追加 1 行：`❌ 调用 git-keeper agent（git-keeper 仅由用户直接调用，主窗口禁止编排）`
- **第 5 节"Agent 隔离规则"**追加补充段：`git-keeper 例外：只能由用户在 prompt 中明确命名调用（"用 git-keeper X" / "@git-keeper X"），不接受主窗口或其他 agent 的派单`

### 3.3 更新 docs/handoff-lessons-learned.md

- 第 5 段"主窗口管家职责"**派生加第 13 条**：主窗口不调用 git-keeper agent（ADR-013）
- 第 9 段"修订历史"记账本次决议

### 3.4 主窗口本会话起严格遵守

- ADR-012 + ADR-013 整改改动本身按 git-keeper 流程：生成草稿，等用户说"用 git-keeper commit" 才落 commit
- 主窗口未来生成 commit 草稿时不再说"复制到终端执行"，改说"调 git-keeper 执行"（仍由用户拍板）

## 4. 后果（Consequences）

**正面**：
- git 写操作有专属责任主体（git-keeper），审计清晰：git reflog 可见每次 commit 都对应用户显式命令
- 主窗口与 release-engineer 都不能"代替用户决定何时 git" —— 治理边界硬规则化
- 用户保留 100% git 控制权——每次执行都需显式命令，无"默许"空间
- 与 ADR-012 互补，构成"主窗口不直接 git + 不间接 git" 双保险
- 用户可在会话内顺畅 git（不必跨进程到终端复制粘贴），同时保留授权环节

**负面 / 妥协**：
- 流程更长：commit 需要"主窗口生成草稿 → 用户审阅 → 用户调 git-keeper" 三步
  - 缓解：草稿质量高（含 v4-4 引用纪律），用户审阅 ≤ 10 秒；用户在会话内说"用 git-keeper commit"比终端复制粘贴更顺
- .claude/agents/ 数量增加（10 → 11，含 ux-designer 仍 11）
  - 缓解：ux-designer 是 sonnet 角色按需调用，git-keeper 同 sonnet；不增加 opus 常驻成本
- 主窗口可能违规—— 擅自调 git-keeper 试图绕过 ADR-012
  - 缓解：git-keeper prompt 含"主窗口派单 → 报告错位 + 拒绝执行" 自检；lessons-learned 第 9 段记账若发生违规事件

**需要警惕的副作用**：
- 主窗口可能用"提示性"语言诱导用户调 git-keeper（"建议你现在用 git-keeper commit"）—— 这本质仍是主窗口主导决策
  - 缓解：主窗口应只提供 commit message 草稿 + git status，不主动建议"现在 commit"——commit 时机由用户判断
- 若用户忘记调用 git-keeper，working tree 会累积未 commit 改动
  - 缓解：主窗口在每次重大改动后跑 `git status -s` 提示用户，但不催促
- 用户可能不耐烦"每次都要说一遍 git-keeper"
  - 缓解：用户可在自然语言中说"打 tag" / "提交一下"，主窗口判断后转给 git-keeper（需用户明确字段）
  - 但用户也可保留"未来某时刻 SUPERSEDE 本 ADR" 选项（如 6 周后觉得太繁琐改回 ADR-012 模式）

## 5. 实施提示

- git-keeper prompt 必含"检测来自用户 vs 主窗口" 自检段
  - 用户显式命令特征：用户原话引用 / 明确"用 git-keeper" / "@git-keeper" / "请 git 执行"
  - 主窗口派单特征：派单 prompt 中无用户原话引用 / 任务上下文是主窗口编排链路
- git-keeper 工具仅 `Read` + `Bash`（不需要 Write/Edit/Glob/Grep）
- git-keeper 不读 spec / ADR / src code（不是它的职责，由 release-engineer / 主窗口给 commit message）
- git-keeper 不主动决定 commit message 内容——必须由用户或主窗口/release-engineer 草稿给定
- 主窗口 commit 草稿模板调整：从"复制到终端执行"改为"等你说『用 git-keeper commit』我转给它执行"

## 6. 验证（How to Verify）

**对**：
- 下次用户在会话中说"用 git-keeper commit X" → 主窗口转派 git-keeper → git-keeper 执行
- git-keeper 收到主窗口派单（非用户）→ 拒绝并报告错位
- git reflog 中新 commit 都对应"用户显式命令 → git-keeper 执行"链路
- 下次会话压缩后主窗口读 CLAUDE.md 第 4.2 节 + 第 5 节看到禁令

**错**（什么时候考虑 SUPERSEDE）：
- 主窗口擅自调 git-keeper → 说明 git-keeper 自检失效，需写新 ADR 加更强约束（如 PreToolUse hook 拦 Agent 工具对 git-keeper 的调用）
- 用户反馈"太繁琐"（6 周内累计 ≥ 3 次抱怨）→ SUPERSEDE 本 ADR 回到 ADR-012 模式
- 6 周内主窗口违规调用 git-keeper ≥ 3 次 → 开 ADR-014 引入 PreToolUse hook 拦截

## 7. 与现有 agent 的边界

| Agent | 域 | git 操作权 | 调用方 |
|---|---|---|---|
| product-strategist | spec | 无 | 主窗口 |
| tech-architect | ADR | 无 | 主窗口 |
| security-reviewer | ADR 第 7 节 | 无 | 主窗口 |
| backend-implementer | src-tauri/src/** | 无（仅生成代码改动留 working tree）| 主窗口 |
| frontend-implementer | src/** static/** | 无（同上）| 主窗口 |
| code-reviewer | spec 末尾 review 段 | 无 | 主窗口 |
| qa-tester | tests/ + src-tauri/tests/ | 无 | 主窗口 |
| docs-writer | docs/ + *.md 用户文档 | 无 | 主窗口 |
| release-engineer | package.json/Cargo.toml/tauri.conf.json/CHANGELOG/CI yml | **仅生成 git 命令建议**，不执行 | 主窗口 |
| ux-designer | spec 第 6 节 UX | 无 | 主窗口 |
| **git-keeper（新）** | `.git/objects/` + refs + remote | **唯一执行入口** | **仅用户** |

主窗口角色：编排 + 协调 + PLAN.md/decisions/lessons-learned 写权 + 生成草稿；**不调 git-keeper**。

## 8. 过度工程自查（v2-11）

- 本 ADR 220 行，**部分可省略**：第 4.3 节 7 条 agent 边界表与现有 CLAUDE.md 第 4.2 / 第 5 节重复约 20 行；保留是为未来读者一目了然，可接受
- 7 节结构未引入新概念（沿用 ADR-012 + ADR-001 的边界模型）
- 估计 5-10% 段落可省略；超额合理

## 9. owner 边界自查（v2-12）

主窗口本次落盘的文件：
- `decisions/ADR-013-dedicated-git-keeper-agent.md` ← 本文件，属"用户实时拍板的决议"（CLAUDE.md 第 4.1 节明确允许）
- `CLAUDE.md` ← 本 ADR 即论证（第 13 节要求）
- `docs/handoff-lessons-learned.md` ← 主窗口 owner（v4-5 长期记忆机制）
- `.claude/agents/git-keeper.md` ← **新增 agent 文件，第 13 节 CLAUDE.md 要求"新增 agent → 写 ADR + 更新本文件"——本 ADR 即论证**

未越界。
