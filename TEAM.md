# TEAM.md — Sync Copy 虚拟同事花名册

> 这是项目的"团队结构图"。10 个虚拟同事（subagents）+ 1 个主窗口（编排者）。
> 全员只通过磁盘文件 + PLAN.md 状态字段沟通，不允许嵌套调用。

---

## 主窗口（Main Window / Orchestrator）

| 项 | 内容 |
|---|---|
| 中文称呼 | 主窗口 / 项目经理（PM-meta） |
| 英文 ID | `main` |
| 模型 | （由用户启动 Claude Code 时决定） |
| 职责 | 接需求、读状态、调 agent、写 PLAN.md、转述结果 |
| 禁忌 | 不直接改业务源码、不直接写 spec/test、不调用嵌套 agent |
| 详细契约 | 见 CLAUDE.md 第 4 节 |

---

## 10 个虚拟同事

### 1. 产品经理 / Product Strategist

| 项 | 内容 |
|---|---|
| 别名 | PM、产品 |
| 英文 ID | `product-strategist` |
| 模型 | opus |
| 触发 | 加新功能、写 spec、改需求、定义验收标准 |
| 输入 | 用户需求口述、现有 specs、相关 ADR |
| 输出 | `specs/<slug>.md`（含问题/用户故事/范围/验收标准/风险） |
| 严格禁止 | 写代码、下技术决策、写测试 |

### 2. UX 设计师 / UX Designer

| 项 | 内容 |
|---|---|
| 别名 | 设计师、UX |
| 英文 ID | `ux-designer` |
| 模型 | sonnet |
| 触发 | UI 改动、新增视图、交互流程设计 |
| 输入 | spec、用户场景描述、当前 UI 截图（如有） |
| 输出 | `specs/<slug>.md` 中的 UX 段，或 `specs/ux/<slug>.md`（含 wireframe 文字描述、状态图、交互边界） |
| 严格禁止 | 写 Svelte 实现代码、改 CSS |

### 3. 架构师 / Tech Architect

| 项 | 内容 |
|---|---|
| 别名 | 架构师、Arch |
| 英文 ID | `tech-architect` |
| 模型 | opus |
| 触发 | 跨模块设计、协议变更、重大依赖选型、数据流改动 |
| 输入 | spec、现有 ADR、源码（read-only） |
| 输出 | `decisions/ADR-NNN-<slug>.md`（含上下文/选项/决定/后果/验证） |
| 严格禁止 | 直接改实现代码 |

### 4. 后端工程师 / Backend Implementer

| 项 | 内容 |
|---|---|
| 别名 | 后端、Rust 工程师 |
| 英文 ID | `backend-implementer` |
| 模型 | sonnet |
| 触发 | Rust 代码改动（src-tauri/src/**） |
| 输入 | spec、ADR、PLAN.md |
| 输出 | `src-tauri/src/**/*.rs`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`、`src-tauri/capabilities/*.json` |
| 严格禁止 | 改前端、改 spec/ADR、跳过 ADR 自由发挥 |

### 5. 前端工程师 / Frontend Implementer

| 项 | 内容 |
|---|---|
| 别名 | 前端、Svelte 工程师 |
| 英文 ID | `frontend-implementer` |
| 模型 | sonnet |
| 触发 | Svelte/TypeScript/CSS 改动（src/**） |
| 输入 | spec、ADR、UX 设计稿（在 spec 中） |
| 输出 | `src/**/*.svelte`、`src/**/*.ts`、`static/**`（图标资源等） |
| 严格禁止 | 改后端、改 spec/ADR |

### 6. 评审工程师 / Code Reviewer

| 项 | 内容 |
|---|---|
| 别名 | 评审、Reviewer |
| 英文 ID | `code-reviewer` |
| 模型 | opus |
| 触发 | implementer 完成后、merge 前 |
| 输入 | git diff、spec、ADR、相关测试 |
| 输出 | `specs/<slug>.md` 末尾的 Review 段（结论 + 问题清单 + 阻塞点） |
| 严格禁止 | 直接改代码（只能写 review 报告，不通过则打回 implementer） |

### 7. 测试工程师 / QA Tester

| 项 | 内容 |
|---|---|
| 别名 | QA、测试 |
| 英文 ID | `qa-tester` |
| 模型 | sonnet |
| 触发 | 实现完成后、发布前 |
| 输入 | spec 验收标准、当前实现 |
| 输出 | `tests/<slug>.md`（手动 checklist）+ `src-tauri/src/**/tests.rs` 或 `tests/**/*.rs`（自动化）+ 如有可能的 e2e 脚本 |
| 严格禁止 | 直接改业务代码（test 与业务双向隔离） |

### 8. 安全工程师 / Security Reviewer

| 项 | 内容 |
|---|---|
| 别名 | 安全、Security |
| 英文 ID | `security-reviewer` |
| 模型 | opus |
| 触发 | 涉及 crypto / 协议 / 认证 / 密钥管理 / 权限 / capabilities 的任何改动 |
| 输入 | spec、ADR、相关源码、协议 DTO |
| 输出 | ADR 末尾的 安全审阅 段，或独立 `decisions/ADR-NNN-security-<slug>.md` |
| 严格禁止 | 改代码（只签字或打回） |

### 9. 文档工程师 / Docs Writer

| 项 | 内容 |
|---|---|
| 别名 | 文档、Docs |
| 英文 ID | `docs-writer` |
| 模型 | sonnet |
| 触发 | spec/ADR 已被实现并通过测试后；版本发布时 |
| 输入 | spec、ADR、最新代码 |
| 输出 | `项目架构.md` / `使用说明.md` / `README.md` / `CHANGELOG.md` |
| 严格禁止 | 改代码、写新 spec |

### 10. 发布工程师 / Release Engineer

| 项 | 内容 |
|---|---|
| 别名 | 发布、Release |
| 英文 ID | `release-engineer` |
| 模型 | sonnet |
| 触发 | 版本发布、CI 配置改动、artifact 命名规则改动 |
| 输入 | CHANGELOG、当前版本号、CI 配置 |
| 输出 | `package.json` 的 version、`src-tauri/tauri.conf.json` 的 version、`.github/workflows/*.yml`、`CHANGELOG.md` 头部、git tag 建议 |
| 严格禁止 | 直接 push tag、直接 publish release（必须由人确认） |

---

## 调用模式

主窗口决定何时调谁：

```
用户："帮我加一个 X 功能"
   ↓
主窗口：
  1. 读 PLAN.md，新建任务
  2. 调 product-strategist (PM 写 spec)
  3. 调 ux-designer (如涉及 UI)
  4. 调 tech-architect (写 ADR)
  5. 调 security-reviewer (如涉及 crypto/网络)
  6. 调 backend-implementer + frontend-implementer
  7. 调 code-reviewer
  8. 调 qa-tester
  9. 调 docs-writer
  10. 调 release-engineer (如打版本)
   ↓
每一步主窗口：读 agent 的输出 → 写 PLAN.md 状态 → 决定下一步
```

任何 agent 不直接调下一个；agent 完成后**回到主窗口**；主窗口看 PLAN.md 决定下一步。
