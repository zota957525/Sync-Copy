---
description: 开启一个新 feature 的 SDLC 流水线。用法：/feature <slug> "<一句话需求>"
---

# /feature <slug> "<一句话需求>"

参数解析：

- `<slug>`：英文 kebab-case 的功能短名，如 `clipboard-text-sync`
- `"<一句话需求>"`：用户视角的需求陈述

请按以下顺序执行：

## 1. 校验环境

- 当前 git working tree 是否 clean？（`git status -s` 必须空）—— 不空则报错让用户先 commit/stash
- `.claude/pipeline-mode` 当前是 lite 还是 full？提示用户：
  - lite 模式下不会自动调 UX/安全/文档/发布
  - full 模式下走完整流程

## 2. 在 PLAN.md 新增 task

在 PLAN.md 的 Phase 4（实现） 段落里新增一行：

```
| P4-<n> | <slug> — <一句话需求> | BACKLOG | product-strategist | / |
```

（n 取当前 Phase 4 task 数 + 1）

## 3. 启动 SDLC 流水线（不调 agent，只通知用户）

报告以下内容：

```
🚀 已开启 feature 流水线：<slug>

任务 ID：P4-<n>
当前模式：<lite/full>

接下来按 SDLC 顺序，主窗口会逐个调用对应 agent。每一步完成后，主窗口会更新 PLAN.md
状态字段，并把控制权交给你。你看完每个 agent 的产出，确认后说"继续"，主窗口推进下一步。

预期顺序：
1. 调 product-strategist 写 specs/<slug>.md
2. (full only) 调 ux-designer 填 第 6 节 UX 段
3. 调 tech-architect 写 ADR
4. (full only & 涉密) 调 security-reviewer 加 第 7 节 安全审阅
5. 调 backend-implementer / frontend-implementer 实现
6. 调 code-reviewer review
7. 调 qa-tester 测试
8. (full only) 调 docs-writer 更新文档
9. (full only & 打版本时) 调 release-engineer 升版本

我现在执行第 1 步：调用产品经理。
```

## 4. 调用产品经理

Use Agent tool with subagent_type=`product-strategist`，prompt 指引它读 PLAN.md 找到对应 P4-<n>，然后开始写 spec。

⚠️ **本 slash 命令只主导第 1 步**。后续步骤由用户在主窗口里手动确认 / 推进。
