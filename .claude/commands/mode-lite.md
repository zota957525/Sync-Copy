---
description: 切换到 lite 模式（仅核心 6 角色：PM / 架构师 / 后端 / 前端 / 评审 / 测试）。适合简单 bug 修复或日常迭代。
---

把项目切换为 lite 模式。

请执行：

1. 把 `.claude/pipeline-mode` 文件内容改成 `lite`（不要保留换行外的字符）
2. 在 PLAN.md 顶部如有「当前模式」字段，更新为 `lite`
3. 报告当前 lite 模式下激活的 6 个角色：
   - product-strategist (产品经理)
   - tech-architect (架构师)
   - backend-implementer (后端工程师)
   - frontend-implementer (前端工程师)
   - code-reviewer (评审工程师)
   - qa-tester (测试工程师)
4. 提示：如本次改动涉及 UI 设计 / 加密 / 文档 / 发布，需要切回 full 模式（`/mode-full`）
