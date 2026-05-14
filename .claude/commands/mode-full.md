---
description: 切换到 full 模式（全 10 角色）。重写阶段、新功能、加密相关改动、版本发布触发。
---

把项目切换为 full 模式。

请执行：

1. 把 `.claude/pipeline-mode` 文件内容改成 `full`
2. 在 PLAN.md 顶部如有「当前模式」字段，更新为 `full`
3. 报告当前 full 模式下激活的 10 个角色：
   - product-strategist (产品经理) ★
   - ux-designer (UX 设计师)
   - tech-architect (架构师) ★
   - backend-implementer (后端工程师) ★
   - frontend-implementer (前端工程师) ★
   - code-reviewer (评审工程师) ★
   - qa-tester (测试工程师) ★
   - security-reviewer (安全工程师)
   - docs-writer (文档工程师)
   - release-engineer (发布工程师)

   （★ = lite 模式中也有）

4. 提示：full 模式下每个 feature 都按 SDLC 完整流程走：spec → UX 段 → ADR → 安全审阅（如涉密）→ impl → review → test → docs → release。流程不可跳过。
