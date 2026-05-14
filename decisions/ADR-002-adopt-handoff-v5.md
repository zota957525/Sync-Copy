---
id: ADR-002
feature_id: F-2026-002
title: 项目升级到 HANDOFF v5 规范（增量补丁迁移）
status: ACCEPTED
date: 2026-05-08
deciders: [main, user]
related_specs: []
related_adrs: [ADR-001]
supersedes: []
superseded_by: []
revision_history:
  - version: v1
    date: 2026-05-08
    summary: 初版决议——选项 A 增量补丁迁移
depends_on_artifacts:
  - path: HANDOFF.md
    version: v5 (2026-05-08)
  - path: CLAUDE.md
    version: 2026-05-06
  - path: decisions/ADR-001-rewrite-with-strict-sdlc.md
    version: 2026-05-06
---

# ADR-002 — 项目升级到 HANDOFF v5 规范（增量补丁迁移）

## 1. 上下文（Context）

`HANDOFF.md` 在 2026-05-08 由用户从 v1 替换为 v5，叠加了 12 条新规则——3 条工作流纪律（v5-1 错位升级信号 / v5-2 流水线自动跑 / v5-3 严格 SDLC）+ 7 条工程规范（v5-4~v5-10 依赖兼容 / lifecycle owner / 反序列化 coerce / SDK idempotent / 物理资源并发 / agent registry 完整性 / 三向决议日常审计）+ 2 条用户交互纪律（v5-11 决策卡片格式 / v5-12 § 符号禁令），并继承 v4 的 8 条长期记忆机制（v4-1~v4-8）。

主窗口对当前项目做差距 audit，发现 17 项缺漏 / 违反，分三类：
- **资产缺漏**：`specs/_assumptions.md` 不存在（v2-6）；`docs/handoff-lessons-learned.md` 不存在（v4-1）；agent 全是 v1 5-section 结构（v2-11/v2-12 的"过度工程自查 + owner 边界自查"两段缺失）；ADR-001 frontmatter 缺 v2 字段；20 份 spec 缺 `depends_on_artifacts`
- **流程违反**：subagent 直接写 PLAN.md（v2-9）；流水线非自动跑（v5-2）；决策卡片格式不严（v5-11）；缺会话压缩复审 ritual（v4-3）
- **CLAUDE.md 未镜像 v4+v5 规则**（v2-1 要求）

现行项目宪法（CLAUDE.md）第 13 节规定："每一次 CLAUDE.md 修改必须有对应 ADR 论证"——本 ADR 即此论证。

## 2. 选项（Options Considered）

### 选项 A：增量补丁（最小可行）— 用户选定

- **执行**：
  1. 立即创建 `specs/_assumptions.md`（从 P1-1~P1-5 已做工作中反向提取事实假设让用户校对）
  2. 立即创建 `docs/handoff-lessons-learned.md` 10 段骨架（先空着，待事件触发追加）
  3. CLAUDE.md 追加 第 14 节"HANDOFF v5 镜像规则"——v4-1~v4-8 + v5-1~v5-11 全部条目化
  4. safety-bar.sh 追加 4 条 HARD_BLOCKS pattern：`xcrun altool --upload-app` / `fastlane release|deliver` / `rm.*\.(jks|p8|p12|pem)$` 类
  5. PLAN.md 新增 P0-5 任务记录本次迁移
  6. **立即生效（无需写文件）**：v5-2 流水线自动跑 / v5-11 决策卡片格式 / v4-3 复审 ritual / v4-5 主窗口管家职责
  7. **懒迁移**：agent 7-section 结构升级、ADR-001 frontmatter 升级、20 份 spec `depends_on_artifacts` 增补——留到下次该 agent 被调用 / 该 ADR 被引用 / 该 spec 被改动时就地升级
- **优点**：30 分钟主窗口工作即可生效；不浪费 P1-1~P1-5 已完成的 6 小时 PM 工作；真正立即生效的是主窗口行为，不需要批量改文件
- **缺点**：22 份现有产物的非合规字段会陪着我们一段时间；如果某 spec 永远不被改，它就永远缺 `depends_on_artifacts`；纪律上有"半合规"窗口期
- **用户选定**

### 选项 B：全量主动迁移

- **执行**：A 的所有动作 + 立即重写所有 10 个 agent 文件到 7-section + 立即给 ADR-001 升级 frontmatter + 立即给 20 份 spec 全部增补 `depends_on_artifacts`
- **优点**：立即 100% 合规
- **缺点**：约 1.5 小时机械工作完全没业务价值，只是为合规而合规；多数字段填进去也是冗余信息（22 份产物互相依赖关系简单）
- **否决**

### 选项 C：推倒重做

- **执行**：删除现有 specs/ + decisions/，从 v5 第一阶段（项目调研）重启；PM 重写产品总览 → `_assumptions.md` → backlog → 18 份 spec；既有 22 份产物作为 legacy 参考但不再权威
- **优点**：100% 按 v5 流程产出
- **缺点**：浪费 P1-1~P1-5 已完成的工作（约 6 小时 PM 工时）；且重写未必比现有更好——`_assumptions.md` 是缺漏的真痛点，但其它 v5 字段对当前阶段（还没进 ADR）价值有限
- **否决**

## 3. 决定（Decision）

**选项 A：增量补丁。**

具体执行清单：

**立即生效行为（主窗口自身规约，无需写文件）**：

1. **v5-2 流水线自动跑**：从此主窗口默认连续推进所有 SDLC 阶段，**仅 3 类硬关卡停下问用户**：① 关键产品方向决策（spec 完成 K-Q 拍板）② 早期架构性决策（design 阶段决策卡片）③ 不可逆操作（合 PR / 上线 / 删数据 / 公开发布）。其它全自动。BLOCKED 上报阈值：同一阶段连续 BLOCKED 2 次 → 上报用户决策。
2. **v5-11 决策卡片格式**：任何 stop-and-ask 必须含——一句话问题 + ≥2 选项（含推荐）+ 取舍 + 不做的后果 + reviewer must-fix。**禁止**开放式"接下来怎么办？""你说选哪个？"。
3. **v4-3 复审 ritual**：会话压缩 / 上下文重启后第一动作 = 4 步复审（读最新 spec/ADR → 对每条 todo 逐条 cross-check 决议状态 → 标 obsoleted 归档 → 第一段话报告复审结果）。本次会话已落入此场景，本 ADR 的产出本身即为 ritual 第一轮报告。
4. **v4-5 主窗口管家职责**：审视而非搬运 / 主动归档 / 强制引用纪律 / 多 agent 协作 RACI 把关 / 会话重启复审 / 被动 → 主动。
5. **v5-1 错位升级信号**：检测到用户 / 主窗口 / 错位 agent 在做某专家应做的事，立即触发 lite→full 切换或新角色引入；不让流水线带着错位硬跑。
6. **v4-4 引用纪律**：commit / 总结 / audit 引用决议必须精确到 `ADR-NNN` / `spec [N.M]` / `commit-SHA`；禁止"之前定的""刚才所说""以前讨论过"模糊引用；已 obsoleted 决议引用必须带 `OBSOLETED` 标签。

**立即落盘文件**：

7. `specs/_assumptions.md`：PM 已隐含校对过的事实假设，反向提取列表让用户**逐条确认**（P1-1~P1-5 跑过修了 5 处事实层假设，剩余假设需要兜底校对）
8. `docs/handoff-lessons-learned.md`：10 段骨架（[0] 这是什么 / [1] 30 秒引导 / [2] 元决议总览 / [3] obsoleted 清单 / [4] 历史踩坑分类 / [5] 主窗口职责 / [6] 复审 ritual / [7] 引用纪律 / [8] 反风控约束 / [9] 修订历史 / [10] 引用关联）。先建骨架，后续触发条件发生时追加。
9. `CLAUDE.md` 追加 第 14 节"HANDOFF v5 规则镜像"：v4-1~v4-8 + v5-1~v5-12 简表化。
10. `.claude/hooks/safety-bar.sh` HARD_BLOCKS 段追加 4 条：`xcrun.*altool.*--upload-app` / `fastlane\s+(release|deliver)` / `rm\s+.*\.(jks|p8|p12|pem)\b` / `security\s+import.*-k\s+.*\.keychain` 类。
11. `PLAN.md` 新增 P0-5 任务记录本次 v5 迁移落盘动作及懒迁移待办清单。

**懒迁移**（不立即做，但记账）：

- 10 个 agent 文件升级到 7-section（在该 agent 下次被主窗口派单前升级）
- ADR-001 frontmatter 升级到 v2 字段集（在该 ADR 下次被引用 / 修订时升级）
- 20 份 spec 增补 `depends_on_artifacts` 字段（在该 spec 下次被修订时增补）
- 三项懒迁移在 `docs/handoff-lessons-learned.md` 第 9 段"修订历史"持续记账，迁移完成时勾掉

## 4. 后果（Consequences）

**正面**：
- v5 关键纪律（自动跑流水线 / 决策卡片 / 复审 ritual）立即生效，纠正之前"每步问用户"的低效模式
- `_assumptions.md` 补齐事实层校对兜底，降低未来修事实假设的返工成本
- `lessons-learned` 长期记忆机制建立，避免会话压缩后误执行已 obsoleted todo
- safety-bar.sh 收口商店上传与证书文件删除，防误发布 / 误删私钥
- 已完成的 P1-1~P1-5 工作完整保留，不返工

**负面 / 妥协**：
- 半合规窗口期：22 份现有产物 frontmatter 与 v5 不完全一致；如果某 spec 永远不被改，它就永远缺 `depends_on_artifacts`
  - 缓解：lessons-learned 第 9 段"修订历史"记账可见
- 懒迁移可能被遗忘：agent 7-section 升级如果该 agent 后续不被调用就不发生
  - 缓解：每次 agent 派单前主窗口检查其结构，发现是 5-section 就先升级再派
- ADR-002 本身写得较长（这条本身违反 v2-11 过度工程自查倾向）
  - 缓解：本节"过度工程自查"已说明 30% 段落（如选项 B/C 的详细论证）可省略，保留主要为后人理解决策背景

**需要警惕的副作用**：
- 用户可能把"流水线自动跑"理解为"主窗口可以擅自做不可逆操作"——本 ADR 明确"3 类硬关卡"边界，主窗口必须严守
- v4-3 复审 ritual 如果做成形式主义（每次复述固定模板），会浪费用户阅读成本——本 ADR 要求复审报告**只列变化**（删除几条、新增几条、保留几条），不复述全部 todo

## 5. 实施提示

- 本 ADR 的执行步骤 7~11 在主窗口拍板后立即落盘
- 立即生效行为 1~6 从本会话起即生效
- 第一次"实战检验"是 PLAN.md P0-1（`git branch legacy-prototype`），按 v5-2 应该自动推进——但 P0-1 涉及 git 写操作（虽然非破坏性），按 v5-2 第 3 类硬关卡定义"不可逆操作"——`git branch` 创建分支非破坏性，可自动；但 P0-2"清空 src-tauri/src/"涉及代码删除，是不可逆操作的边缘情况，需要决策卡片确认
- v5-1 错位升级信号在本次审视中已检出 1 处：上次会话主窗口给 PM 派任务后 PM 直接改了 PLAN.md 的 P1-2~P1-5 备注栏（违反 v2-9）。本 ADR 落盘后主窗口接管 PLAN.md 写入权，PM 改为在汇报中提议、由主窗口落盘

## 6. 验证（How to Verify）

**对**：
- 下次会话压缩后第一段话主动跑复审 ritual 4 步报告
- 用户问任意决策时主窗口给出的提问都符合 v5-11 决策卡片格式
- 流水线推进过程中除 3 类硬关卡外不再问"继续吗"
- safety-bar.sh 拦截行为：写入 `xcrun altool --upload-app ...` 命令时被拦
- `specs/_assumptions.md` 用户校对完成，每条都有 ✓ / ✗ 标记
- `docs/handoff-lessons-learned.md` 触发条件发生时被追加（第一次预期触发：3 项懒迁移有任一完成时）

**错**（什么时候考虑 SUPERSEDE 本 ADR）：
- 自动跑流水线导致用户被卡了不可逆操作没拦住 → 说明 3 类硬关卡定义有缺口
- 决策卡片格式被用户反馈"太啰嗦"或"问题不必要"→ 说明 v5-11 形式主义化了，要降低门槛
- 懒迁移积压 > 3 个月仍未触发任何升级 → 说明懒迁移策略失效，应改为定期主动批量升级（升级回选项 B 思路）

## 7. 过度工程自查（v2-11）

本 ADR 文档约 240 行，**可省略段落**：
- 选项 B / C 的详细执行清单与缺点（约 15 行）：决策已定，未来读者只需知道有过备选，不必看具体内容
- 第 4 节"需要警惕的副作用"两条（约 8 行）：是规则书层面的告诫，不是本决策的核心后果
- 估计 25-30 行属于过度工程

**保留理由**：本 ADR 是 v5 迁移的"地基性"决议，未来 6 周内任何对当前流程的疑问（如"为什么主窗口不再问继续了？"）都会回到本 ADR 找答案——稍详细可降低后续解释成本。3 个月后回头看若仍觉冗长，由主窗口在新 ADR 里 SUPERSEDE 时精简。

## 8. owner 边界自查（v2-12）

主窗口本次落盘的文件：
- `decisions/ADR-002-adopt-handoff-v5.md` ← 本文件，属"用户实时拍板的决议"，主窗口落盘合规（CLAUDE.md 第 4.1 节例外）
- `CLAUDE.md` ← 本 ADR 即论证（第 13 节要求）
- `specs/_assumptions.md` ← PM 域，但首次创建可由主窗口起草，下次由 PM 修订
- `docs/handoff-lessons-learned.md` ← 长期资产，主窗口 owner（v4-5 主窗口管家职责）
- `.claude/hooks/safety-bar.sh` ← 主窗口 owner（项目治理工具）
- `PLAN.md` ← 主窗口 owner（v2-9）

无越界。
