# HANDOFF: 自适应 Claude Code 脚手架生成口令 v5

> 本仓库（`zota957525/dev-team`）即"SaaS Claude Code 多 Subagent 开发流水线"脚手架的源。
> 本文件是发给**其它项目**的口令——别在本仓库里跑它（脚手架已经在本仓库 `.claude/` 里）。
>
> **版本谱系**：
> - **v0**（已淘汰）：克隆-改装式口令，让新项目 AI 直接 `git clone` 本仓库再适配
> - **v1**：基于**首个项目**实践（详见末尾「v1 变更日志」），重写为三阶段调研-裁剪-生成，淘汰 v0
> - **v2**：基于**摩托车导航屏 App 项目**实践（详见末尾「v2 变更日志」），在 v1 上叠加 12 条硬性规则
> - **v3**：文档级融合——把 v1 隐性踩坑反向陈述为显式 changelog，与 v2 的 12 条并列；不引入新规则
> - **v4**：基于**hubstudio-skills monorepo 项目**踩坑（详见末尾「v4 变更日志」），叠加 8 条**长期记忆机制**类规则
> - **v5（本版，当前最新）**：基于本仓库自身演进 + 用户对 v4 的反馈深化（详见末尾「v5 变更日志」），叠加 12 条规则——3 条**工作流纪律**（错位升级信号 / 流水线自动跑里程碑 / 严格 SDLC 不轻流程）+ 7 条**工程规范**（v4 评估时被舍弃但实战证明通用价值的：依赖兼容性 / 生命周期 owner / 外部接口 coerce / SDK idempotent / 物理资源并发 / agent registry 完整性 / 三向决议日常审计）+ 2 条**用户交互纪律**（请求用户决策前必须提供方案 / 禁止 § 等难读符号）

---

## 怎么用这份文件

1. 在**目标项目**根目录启动 Claude Code（必须是项目根，不是任意目录）
2. 把下面"## 移交口令"整段（从分隔线开始到结束）作为第一条消息发给它
3. 不要急着同意第一阶段总结——仔细看 AI 提的画像和角色裁剪推荐，有不同意就改
4. 第三阶段生成完后，按它给的"接下来要做的事"清单走

**如果 AI 不问就直接生成，打断它**："先调研再生成，按口令第一阶段先来"

---

## 三个项目 + 本仓库自身演进的踩坑经验汇总

本口令是四批实战踩坑的累计沉淀（v1 首个项目 / v2 摩托车 App / v4 hubstudio-skills / v5 本仓库自身用户反馈）。完整故事在末尾各 changelog 段，下表是速览：

### 首个项目（v0 → v1，7 条结构性改动）

| # | 改动 | 对应坑 |
|---|---|---|
| v1-1 | 强制三阶段（调研 → 裁剪 → 生成），调研做完才能动手 | 直接套用本仓库脚手架，结果与项目脱节 |
| v1-2 | 允许跳出 13 角色池自由设计 | SaaS 模板硬塞 ML / 嵌入式 / 移动 App 项目 |
| v1-3 | CLAUDE.md 必须**实填**（禁止 `<TODO: ...>` 残留） | TODO 占位被 AI 跳过，CLAUDE.md 长期空壳 |
| v1-4 | 默认 lite 模式 + lite/full 开关 | 全员 13 角色对原型/小项目成本不可控 |
| v1-5 | 生成完成必须实跑至少 3 个 hook bash 命令验证 | 中文 hook 在 bash 双引号嵌套下乱码 |
| v1-6 | 故障排查速查表（`/agents` 空、`/mode-lite not found` 等）| 用户重复遇相同问题反复求助 |
| v1-7 | 第二阶段角色裁剪强制用户确认 | AI 自行拍板裁剪结果常与用户预期不符 |

### 摩托车 App 项目（v1 → v2，12 条强化规则）

| # | 规则 | 对应坑 |
|---|---|---|
| v2-1 | CLAUDE.md 生成前必须 Read `~/.claude/CLAUDE.md` 把全局硬规则镜像到项目级 | subagent 不读全局，规则只在全局会被忽视 |
| v2-2 | CLAUDE.md 必含「用户角色边界」章节 | 主窗口让用户审 ADR 全文 |
| v2-3 | design 阶段「用户签字」= reviewer 双签 + 主窗口写决策卡片，禁止甩 ADR 给用户 | 用户被迫读 1500+ 行技术文档 |
| v2-4 | ADR 模板加 `superseded_by` 字段；新版替代旧版必须显式标注 | v1/v2 同文件共存读者不知哪份当前 |
| v2-5 | 主窗口主动监测当前任务需要的角色是否在岗；如需 full 模式但当前 lite 主动提示用户切 | 用户没切 full → reviewer 不在岗 → 主窗口被迫让用户审 |
| v2-6 | product-strategist 第一份 backlog 前先产出 `specs/_assumptions.md` 让用户校对 | 用户后期才修正多个事实假设引发返工 |
| v2-7 | 涉及外部生态（芯片厂、协议族、API 限额、平台政策）的关键事实，写 ADR 前必须 WebSearch 独立验证 | 架构师把 V536 误认联咏，命名错误后期修订 |
| v2-8 | 主窗口派单 prompt ≤ 1500 字；只含任务摘要 + 必读文件清单 + 输出列表 + 严格禁止 + 完成报告 | 派单 prompt 2000+ 字大量复述背景 |
| v2-9 | PLAN.md 只在 stage 转换 / 重大里程碑 / 用户拍板时更新；subagent 不直接写 PLAN.md | PLAN.md 一会话被改十几次产生噪音 |
| v2-10 | agent 产物 frontmatter 必含 `depends_on_artifacts:` 字段；下一轮 agent 据此增量 | 协议联络员每轮重读所有上游产物 |
| v2-11 | agent 完成报告必含「过度工程自查」一段：本轮产物哪些段落可省略 | ADR-001 v2 写 746 行约 30% 过度设计 |
| v2-12 | subagent 完成时跑 `git status` 自查只动了自己 owner 范围内的文件；越界要在汇报中显式列出 | 协议联络员/架构师越权改 PLAN.md |

### hubstudio-skills monorepo 项目（v2 → v4，8 条长期记忆类规则）

| # | 规则 | 对应坑 |
|---|---|---|
| v4-1 | **Living lessons-learned 文档**（项目级长期资产）：每次决议反转 / 生产 bug / 会话误用过期决议 / 新 ADR 落档 / RACI 冲突 / 用户反馈不满，必须追加 1 条；agent 进入会话第一动作 = 读"obsolete 清单 + 复审 ritual" | "PyPI yank v0.1.0" 已完成却被会话 summary 当 pending 重列；老决议在 commit/summary/todo 之间自传播 |
| v4-2 | **决议 obsolete 清单显式维护**：每条记 原决议 → 反转触发引用 → 新决议 → ❌ 禁止做的事；agent 看到 ❌ 项即使在历史 commit 里出现也忽略 | 红档决议反转后旧 todo / 旧 commit 仍在被新会话搬运复用 |
| v4-3 | **会话压缩/重启复审 ritual**（4 步硬性流程）：① 读最新 sprint spec K-Q + 最近 ADR ② 对每条 pending todo 逐条 cross-check 决议状态 ③ 标 obsoleted 并归档 ④ 第一段话报告复审结果 + 列删除/保留/新增数；做完才能干事 | 上下文压缩后 agent 机械搬运旧 todo，不查最新决议状态 |
| v4-4 | **引用纪律**：commit / summary / audit 引用决议必须精确到 `ADR-NNN` / `K-Q-X.Y` / `spec [N.M]` / `commit-SHA`；禁止"之前定的""刚才所说""以前讨论过"模糊引用；已 obsoleted 决议引用必须带 `OBSOLETED` 标签 | 模糊引用导致权威性无法追溯；obsoleted 决议被当现状复用 |
| v4-5 | **主窗口 = 助理 / 管家而非 reminder bot**：六条职责显式写入 CLAUDE.md——审视而非搬运 / 主动归档 / 强制引用纪律 / 多 agent 协作 RACI 把关 / 会话重启复审 / 被动 → 主动 | 主窗口被动列 todo，把已完成 / 已废止当 pending；用户被迫自己审视 |
| v4-6 | **决议反转 = 触发四步文档同步**：(a) Living doc obsolete 清单加条 (b) 老 ADR status → SUPERSEDED + superseded_by (c) spec K-Q 表同步 (d) 整理 forbidden actions 列表；四步缺一阻塞 | 反转决议后只改了 ADR 一处，其他文档继续承载旧决议引发不一致 |
| v4-7 | **fatal error 三件套**：任何用户面应用 / agent 工具 fatal error 必须 (a) 写文件日志 (b) 弹 GUI/CLI 用户可见对话 (c) **不允许**静默 exit；windowed 应用尤其禁止依赖 stderr | windowed .exe 启动后秒退，无错误窗口、无日志、用户/agent 都无从排查 |
| v4-8 | **跨边界自动操作禁令**（升级 v1+v2 PreToolUse 的产品规则版）：不 auto-install 系统组件 / 不修系统代理 / 不动用户证书 / 不要求关闭 Clash 等代理工具 / 不动 HKLM 注册表；应用层兜底（如 `--proxy-bypass-list`）而非动系统 | 让用户关 Clash → 阻塞所有依赖代理的工作流；auto-install 触发提权弹框 |

### 本仓库自身演进 + 用户反馈（v4 → v5，10 条规则：3 工作流 + 7 工程规范）

**工作流纪律**（来自 v4 后用户对流程粒度的明确要求）：

| # | 规则 | 对应坑 |
|---|---|---|
| v5-1 | **错位工作 = 模式/角色升级信号**：发现非对应角色（用户 / 主窗口 / 错位 agent）在做某类工作，立即触发审视——这是 lite→full 升级 OR 引入新角色的硬信号 | lite 模式跑着跑着用户被迫审 ADR / 主窗口被迫写实现代码——说明该 reviewer / implementer 角色不在岗，但流水线照样硬跑 |
| v5-2 | **流水线默认自动跑完里程碑**：主窗口连续推进所有 SDLC 阶段，仅在三类硬关卡停下问用户：① 关键产品方向决策（spec 完成 K-Q 拍板）② 早期架构性决策（design 阶段 5 分钟决策卡片）③ 不可逆操作（合 PR / 上线 / 删数据）。其他全部自动 | 主窗口每跨阶段都问"继续吗"，用户疲于应付；或反过来不问就 commit 不可逆操作 |
| v5-3 | **严格 SDLC，lite 不等于轻流程**：任何功能都必须 spec.md（含 AC）→ ADR（如有架构决策）→ design.md → 实现 → review.md → test-matrix.md → release-notes，**一个不能少**。lite 仅是默认更少 agent；当某个 SDLC 步骤需要 lite 之外的角色（如 ADR 需要 tech-architect），主窗口必须临时召唤该角色，**不能略过该 SDLC 步骤** | 把 lite 当成"省事模式"，跳过 ADR 直接写代码；后期回头补 ADR 发现实现偏离了未声明的架构假设 |

**工程规范**（v4 评估时舍弃、v5 用户要求按"通用方法论 + 项目举例"加回，因为都是踩坑换来的）：

| # | 规则 | 对应坑（项目举例作说明） |
|---|---|---|
| v5-4 | **第三方依赖版本兼容性 cross-check**：引入 / 升级任何第三方库前必须主动查 trove classifier / engines / minimum-supported-version；不能假设新 minor / major 都向前兼容 | hubstudio-skills 项目用 Python 3.14 装 pythonnet 失败——pywebview Win backend 不兼容 3.14，被迫硬装 Python 3.13 并锁 `requires-python = ">=3.13,<3.14"` |
| v5-5 | **长生存周期任务必须显式声明 lifecycle owner**：调度器 / 后台 worker / 长连接等 long-running task 必须挂在长生存周期 event loop（明确归属哪个 loop / 谁负责拉起谁负责关），不能挂在临时 loop | scheduler 跑在 `asyncio.run()` 临时 loop 里，函数返回时 loop 退出杀掉所有 task；用户提交任务后看到 "Scheduler started" 但 tick 0 次 |
| v5-6 | **外部接口字段类型不可信，反序列化必须 try-coerce**：所有 from_external / from_sdk / from_api 反序列化必须宽容入参（field_validator + 显式 str/int coerce）；不能假设第三方返回类型与文档一致 | HubStudio API 文档写 `containerCode: string`，实际返回 `int`；Pydantic ValidationError，整链中断 |
| v5-7 | **SDK 资源操作必须 idempotent + 残留恢复路径**：任何 open / close / acquire / release 类 SDK 操作必须 (a) 复用已存在资源 (b) 残留状态恢复 (c) 重试机制；不能假设每次 fresh state | `SdkFacade.open(test01)` 遇到已 running container → `EnvironmentStartError "上次未结束"`；后改三层 fallback：复用进程内 pool → 正常 open → 残留恢复（stop_browser → 重试 1 次） |
| v5-8 | **物理 / 外部资源并发管控**：任何对物理资源（设备 / 容器 / 第三方账号 / 限额 API）的访问必须有并发控制——per-resource lock 或全局串行；scheduler concurrency 默认 1，等观测稳定再升 | scheduler tick 同时分发多 task 到同一 container_code → HubStudio "环境正在运行中" 反复抢；后改全局串行 `if self._inflight: return 0` |
| v5-9 | **agent / scheduler / registry 完整性 cross-check**：agent 能干啥完全等同 tools / registry 暴露啥；缺工具就缺能力。每次 architecture 改动 / 新增功能域必须伴随 registry tool inventory check | agent 想列环境却 registry 没 list_environments → 反复打开秒关看似"无动作"；用户实际反馈"agent 怎么跟没装一样" |
| v5-10 | **三向决议一致性日常审计（v4-6 扩展）**：v4-6 只覆盖了"决议反转时"的同步；v5 扩展为"正常演进时"也要审计。状态流转门禁（如 `READY_FOR_DESIGN → READY_FOR_IMPL`）必须有 SDLC 预审 cross-check ADR + spec K-Q + architecture 三处一致 | 架构师 v6 写时引旧值 70%，新 ADR-006 已决 80%，老九 SDLC 预审才发现三向冲突；说明只在反转时同步不够，正常演进也要查 |

**用户交互纪律**（来自用户对 v5 起草过程中的反馈）：

| # | 规则 | 对应坑 |
|---|---|---|
| v5-11 | **请求用户决策前必须提供方案 / 选项 / 推荐 / 不得不让用户做的理由**：任何 stop-and-ask user 必须满足"决策卡片格式"——一句话问题 + 至少 2 选项（含推荐）+ 取舍 + 不做这件事的后果 + 必要时附 reviewer must-fix。**禁止开放式"我们怎么办？""你说怎么办？"** | 主窗口动不动甩"接下来怎么办"扔给用户，用户被迫从零想；体验差且违反 v2-2 用户角色边界 |
| v5-12 | **禁止使用 § 符号**（及类似难读 / 难输入符号如 ‡ ¶ ⁂）：所有产物（spec / ADR / commit / 文档 / 代码注释）都不允许出现。表达"段 / 节"用 `第 X 段` `[X.Y]` `#X` `## 标题`；表达脚注用 markdown footnote 语法 | 用户全局 `~/.claude/CLAUDE.md` 已写"禁止 §"，但 subagent 没继承导致大量产出含 §，后期批量 sed 删；v5 升格为顶层 HANDOFF 硬规则 |

如果你想进一步了解每条规则的具体应用场景与项目背景，看本文件末尾的各 changelog 段（v1 / v2 / v3 / v4 / v5）。

---

## 这份口令的设计意图（可选阅读）

每条设计选择都对应上面表格里的某条踩坑：

**为什么分三阶段**（→ v1-1）：直接生成脚手架容易"塞一堆角色然后填占位"，最后产出与项目脱节。三阶段强制 AI 先理解项目再设计。

**为什么允许跳出 13 角色池**（→ v1-2）：SaaS 模板不普世。ML 项目要"数据工程师 / 模型评估工程师"，移动 App 要"应用商店发布工程师"，游戏要"关卡设计师 / 美术资源管理工程师"。口令明确允许 AI 自由设计。

**为什么强调 CLAUDE.md 实填**（→ v1-3）：占位不填实是脚手架失败最大原因。让 AI 在调研阶段搜集信息，直接生成实填版本。

**为什么保留模式开关默认开启**（→ v1-4）：除非项目已成熟稳定（13 角色常态运转），两档模式对成本控制极有帮助。

**为什么强调验证**（→ v1-5）：hook 中文文本容易有引号嵌套问题。口令要求 AI 实际跑 bash 验证。

**为什么 v2 加用户角色边界**（→ v2-2、v2-3）：v1 时代主窗口经常让用户审 ADR 全文，违反主窗口宪法第 5 条（用户角色错位）。v2 把"design 阶段用户签字"明确为 reviewer 双签 + 决策卡片流程。

**为什么 v2 加事实假设清单先行**（→ v2-6）：摩托车 App 项目里事实层假设（厂商关系、芯片归属）后期才修正，引发产物全面返工。让用户在 backlog 前一次性校对，避免链式重写。

**为什么 v4 加 Living lessons-learned 文档**（→ v4-1、v4-2、v4-3）：hubstudio-skills 项目里出现"红档决议反转后，旧 todo 在新会话被当 pending 重列"——这不是单点错误，是**决议在 commit/summary/todo 之间自传播**的系统性问题。v4 用"长期记忆机制"（Living doc + obsolete 清单 + 复审 ritual + 引用纪律）四件套兜底，让 agent 看到旧文本能立即识别"这是已废止的，不要做"。

**为什么 v4 把主窗口职责显式化**（→ v4-5）：v1+v2 时代主窗口只是被动调度器，列 todo / 派 agent。但 hubstudio-skills 项目暴露：主窗口必须**主动审视过期事项、归档完成的、提请决策**——是助理 / 管家而非 reminder bot。这条职责升级写进 CLAUDE.md 后，避免"主窗口列出已 obsoleted 项当 pending"的低级错。

**为什么 v4 加 fatal error 三件套**（→ v4-7）：windowed 应用 / 后台 agent / 远程任务，依赖 stderr 暴露错误等于不暴露。任何用户面工具都必须有"日志 + 可见对话 + 不静默"三道兜底。

**为什么 v4 加跨边界禁令**（→ v4-8）：v1+v2 已经有 PreToolUse 安全栏拦危险**操作**，但产品**功能设计**层面也要遵守同样原则——不要让"自动装 X / 关掉 Y / 修改系统 Z"变成产品需求，否则代码层无论怎么兜底都治标不治本。

**为什么 v5 把"错位工作"升格为升级信号**（→ v5-1）：v2-5 只覆盖了"主窗口主动监测当前任务需要的角色是否在岗"这个被动观察。v5 把它升级为**主动信号**——一旦发现用户 / 主窗口 / 错位 agent 在做某专家应做的事，立即触发 lite→full 切换或新角色引入，**不让流水线带着错位硬跑**。

**为什么 v5 让流水线默认自动跑**（→ v5-2）：v1+v2 设计是"每跨阶段问一次用户"，但实践中过于碎片化——用户疲于"继续吗 / 继续吗 / 继续吗"。v5 把停止粒度收敛到**仅 3 类硬关卡**：关键产品决策、早期架构决策（决策卡片形式）、不可逆操作。其余全自动。

**为什么 v5 重申严格 SDLC**（→ v5-3）：v1 时代有人把 lite 当"省事模式"，跳过 ADR 直接写代码；后期回头补 ADR 发现实现已经偏离了未声明的架构假设。v5 钉死：lite 只是默认更少 agent，**不等于跳过 SDLC 步骤**。需要 ADR 时即使 lite 也要召唤 tech-architect。

**为什么 v5 把 v4 时舍弃的工程规范加回来**（→ v5-4 ~ v5-10）：v4 评估时我把"PyInstaller 打包 / Playwright driver / pythonnet 兼容性"等当作技术栈特异性条目舍弃。但用户复盘指出："这些都是踩坑换来的宝贵经验，应该按通用方法论 + 项目举例的形式加回"。v5 把 7 条工程规范按这个形式重写——通用规则在前，项目特定关键词作为举例。

**为什么 v5 加"请求用户决策前必须提供方案"**（→ v5-11）：v2-3 决策卡片只覆盖了 design 阶段。但实践中主窗口在其它阶段也会"动不动甩开放式问题给用户"——"接下来怎么办？""你说选 A 还是 B？"。v5 把决策卡片格式**统一升格为所有 stop-and-ask 的硬要求**：必须含"问题 + 选项 + 推荐 + 取舍 + must-fix"。

**为什么 v5 禁止 § 符号**（→ v5-12）：用户全局 `~/.claude/CLAUDE.md` 早已写过"禁止 § 符号"，但 subagent 不读全局，导致大量产出文档含 §，后期手工 sed 批量删。v2-1 已把"镜像全局规则"作为机制，但 v5 把这条具体规则**单独升格**到顶层 HANDOFF 硬规则——避免每个项目都要复刻一遍。

---

## 移交口令（复制下方分隔线之间的内容到 Claude Code）

============================================================

我要在当前项目搭建一套 Claude Code 多 subagent 开发流水线脚手架（按 HANDOFF v5 规范——融合三个项目 + 本仓库自身演进的实战踩坑经验。v5 在 v4 基础上叠加 12 条规则：3 工作流纪律 + 7 工程规范 + 2 用户交互纪律）。请不要直接套用模板，先调研项目实际情况，再按需裁剪生成合适规模的虚拟同事团队。

## 第一阶段：项目调研（你必须先做完才能开始生成）

请用 Read/Glob/Grep/Bash 工具扫描当前项目，回答以下问题。如果某些信息无法从代码推断，**直接问我**，不要猜：

### A. 项目基本面
1. 项目类型：SaaS Web 应用 / 移动 App / CLI 工具 / 库 / 数据管道 / 嵌入式 / 其他？
2. 主要技术栈：语言、框架、数据库、前端框架（如有）
3. 部署形态：自部署服务器 / 容器化（Docker/K8s）/ Serverless / 桌面应用 / 客户端分发 / 纯本地工具？
4. 项目阶段：纯实验/MVP / 有少量真实用户 / 有付费用户 / 成熟产品 / 维护期？
5. 团队规模：单人 / 小团队（2-5 人）/ 中型（5-20）/ 大型？
6. 已有的工程实践：CI/CD？测试框架？代码评审流程？监控？文档系统？

### B. 项目特征性
7. 是否处理 PII / 支付 / 认证授权？（决定是否需要安全工程师）
8. 是否有 SLA / 性能 NFR 要求？（决定是否需要性能工程师）
9. 是否需要前端 UI？（决定是否需要设计师 / 前端工程师）
10. 是否有数据库 schema 频繁变更？（决定是否需要数据库工程师）
11. 是否有外部用户文档要维护？（决定是否需要文档工程师）
12. 是否需要监控告警 runbook？（决定是否需要 SRE 工程师）
13. 是否合规敏感（SOC2 / GDPR / HIPAA / 等保）？（决定是否需要安全工程师 + 文档工程师）
14. 项目是否要分发给外部客户/开源？（影响发布工程师 + 文档工程师重要度）

### C. 工作风格
15. 我希望流水线"自动化跑完"还是"每步停下等我确认"？
16. 我希望严格 SDLC（每个功能写 spec + ADR）还是轻流程（直接动手 + 测试）？
17. 是否需要中英文双层标识（用中文岗位名称呼）？（默认是）
18. 是否需要两档模式开关（lite/full）？（默认是，对早期项目特别有用）

调研结束后，请总结一份"项目画像"给我看，等我确认后再进入第二阶段。

## 第二阶段：按需裁剪角色清单

根据项目画像，从下面 13 个完整角色池里**勾选**真正需要的，给出推荐清单和理由：

| 角色（中文 / 英文 ID） | 模型 | 适用条件 |
|---|---|---|
| 产品经理 / product-strategist | opus | 几乎总是需要（除非纯个人玩具） |
| 设计师 / ux-designer | sonnet | 有用户界面时需要 |
| 架构师 / tech-architect | opus | 中大型项目或涉及关键技术决策 |
| 数据库工程师 / migration-specialist | sonnet | 有数据库且 schema 会变更 |
| 后端工程师 / backend-implementer | sonnet | 有服务端逻辑 |
| 前端工程师 / frontend-implementer | sonnet | 有前端 |
| 评审工程师 / code-reviewer | opus | 团队 ≥ 2 人或代码质量要求高 |
| 测试工程师 / qa-tester | sonnet | 几乎总是需要 |
| 安全工程师 / security-reviewer | opus | 处理 PII/支付/认证 或合规要求 |
| 性能工程师 / performance-tester | sonnet | 有 SLA 或高 QPS 路径 |
| 文档工程师 / docs-writer | sonnet | 有外部用户/开发者文档 |
| 发布工程师 / release-engineer | sonnet | 有 CI/CD 或正式发布流程 |
| SRE 工程师 / sre-observability | sonnet | 生产服务且需要监控 |

**典型组合参考**（仅供参考，按项目实际定）：
- **个人 CLI 工具/库**（3-4 人）：产品经理 + 后端工程师 + 测试工程师 + 发布工程师
- **早期 SaaS MVP**（4-5 人）：产品经理 + 后端工程师 + 前端工程师 + 测试工程师 + 发布工程师
- **有付费用户的 SaaS**（7-8 人）：上面 + 数据库工程师 + 评审工程师 + 安全工程师
- **合规敏感的成熟 SaaS**（11-13 人）：接近全员
- **数据/ML 管道**：产品经理 + 后端工程师 + 测试工程师 + 性能工程师 + 发布工程师 + SRE 工程师
- **桌面/移动 App**：产品经理 + 设计师 + 前端工程师（替换为客户端工程师角色）+ 测试工程师 + 发布工程师

如果项目类型不在上述模板里（如 ML 项目、嵌入式、游戏、Chrome 扩展等），**自由设计角色**——可以新增（如 ML 项目加"数据工程师"和"模型评估工程师"，移动 App 加"应用商店发布工程师"），不必拘泥 13 人池。

裁剪完后，列出推荐角色清单 + 每个角色为什么需要 + 跳过哪些 + 为什么跳过，等我确认。

### v2 强化：事实假设清单先行（在 backlog v0.1 之前）

第二阶段确认完角色清单后，在 product-strategist 写第一份 backlog 之前，**强制产出 `specs/_assumptions.md`**——列出所有从我表述中推断的事实假设，让我逐条校对。

格式建议：

```markdown
| # | 假设内容 | 出处 / 推断依据 | 我（产品经理）的置信度 | 用户校对 |
|---|---|---|---|---|
| 1 | ODM 即方案公司 X | 用户表述中"ODM 提供" | 高 | ☐ 确认 / ☐ 修正 |
| 2 | 协议文档目录里的 vendor-X 资料是 ODM 直交 | 文件名命中 vendor-X | 中 | ☐ 确认 / ☐ 修正 |
| 3 | 设备运行 Linux 系统 | 用户表述"基于 Linux" | 高 | ☐ 确认 |
```

我校对完后，product-strategist 才能基于已确认事实写 backlog v0.1。**禁止**跳过此步直接写 backlog，否则后期事实层修正会引发产物全面返工。

## 第三阶段：生成脚手架（确认后才开始）

按以下规范生成：

### 文件结构

```
.
├── .claude/
│   ├── agents/                  # 实际选择的 N 个 agent 文件
│   ├── commands/                # slash 命令
│   │   ├── mode-lite.md
│   │   ├── mode-full.md
│   │   ├── mode-status.md
│   │   └── feature.md
│   ├── pipeline-mode            # 内容: "lite" 或 "full"，默认 lite
│   ├── hooks/                   # SubagentStop 提示脚本 + PreToolUse 安全栏脚本（命名你定，建议 next.sh + safety-guard.sh）
│   └── settings.json            # SubagentStop hooks + PreToolUse 安全栏
├── CLAUDE.md                    # 项目宪法（已含项目实际信息，非占位）
├── PLAN.md                      # 任务看板模板
├── TEAM.md                      # 团队花名册
├── specs/
│   ├── _template/               # spec / design / adr / test-plan / release-notes 模板
│   └── _assumptions.md          # v2 新增：事实假设清单（第二阶段产出）
├── docs/
│   └── handoff-lessons-learned.md  # v4 新增：项目级 LIVING DOCUMENT（决议 obsolete 清单 + 复审 ritual + 引用纪律 + 主窗口职责）
└── README.md                    # 安装/使用/调试
```

### Agent 设计规范

每个 agent 文件 frontmatter：

```yaml
---
name: <english-kebab-case>     # 必须英文，主 Claude 语义理解依赖此
description: 【中文岗位】(别名: 别名1、别名2)。<职责描述>。当用户说"中文岗位"、"别名"或<触发场景>时调用。**待命模式**：未经主窗口（Coordinator）派单不主动启动。
tools: Read, Write, Edit, Bash, Glob, Grep   # 按角色实际需要选择
model: opus / sonnet / haiku    # 按角色重要性选
---
```

每个 agent 的 prompt 主体必须包含**七段式**（v2 比 v1 五段式新增 2 段）：
1. **输入**：读什么（文件、状态）
2. **输出**：产出什么文件 + 状态如何更新到 PLAN.md 和 spec
3. **工作流程**：步骤
4. **严格禁止**：明确职责边界（dev 不动测试、qa 不动业务代码、发布工程师绝不直接部署生产）
5. **完成时**：报告什么然后停止
6. **过度工程自查**（v2 新增）：完成时回答"本轮产物是否过度？哪些段落可省略 / 留给下一轮再写？"——避免 ADR 写到 700+ 行 30% 过度设计
7. **owner 边界自查**（v2 新增）：完成时跑 `git status` 自查只动了自己 owner 范围内的文件；越界要在汇报中显式列出请求主窗口确认——避免 subagent 越权改 PLAN.md / 其他 owner 文件

### v2 强化：ADR 模板规范

ADR 文件 frontmatter 必含：

```yaml
---
adr_id: ADR-NNN
feature_id: F-YYYY-NNN
title: <一句话决策标题>
status: proposed | accepted | superseded_by: ADR-NNN | deprecated   # 必填，避免 v1/v2 共存读者不知哪份当前
owner: tech-architect
date: YYYY-MM-DD
supersedes: []                    # 如替代某 ADR，填 ADR-NNN 列表
revision_history:                 # v2 新增：每次修订记录
  - version: v1
    date: YYYY-MM-DD
    summary: 初版决策
  - version: v2
    date: YYYY-MM-DD
    summary: 修订 X，原因 Y
---
```

### v2 强化：agent 产物的 depends_on_artifacts 字段

agent 产物 frontmatter 必含 `depends_on_artifacts:` 字段，列出本产物依赖的上游产物路径 + 版本（commit hash 或 updated_at）：

```yaml
---
title: ...
owner: ...
depends_on_artifacts:
  - path: specs/F-2026-001/spec.md
    version: 2026-05-06            # 或 git short hash
  - path: capabilities.yaml
    version: v0.4
---
```

下一轮 agent 接手时，按 `depends_on_artifacts` 增量阅读，不必重读全部上游产物。

### 主窗口派单规范（v2 强化）

- **派单 prompt ≤ 1500 字**：只含任务摘要（≤ 50 字）/ 必读文件清单（≤ 8 个）/ 输出列表 / 严格禁止 / 完成报告要求。**不复述项目背景**——让 agent 自己 Read 必读文件。
- **关键事实独立验证**：tech-architect / odm-protocol-liaison 等涉及外部生态（芯片厂、协议族、API 限额、平台政策）的 agent 接到 prompt 时，写 ADR / 关键决策前必须用 WebSearch 独立验证（不能只信上游 agent 的判断）。引用一手资料链接到 ADR。
- **派单前权限预审**（v4 新增）：dispatch agent 前 cross-check 该 agent 的 `tools` / 写权限白名单是否覆盖任务所需路径；若 agent 工具被拒（如非 owner 路径的 Write 被拦），主窗口必须明示原因 + 自己接管，**禁止静默吞**。

### v4 强化：Living lessons-learned 文档（项目级长期资产）

生成时创建 `docs/handoff-lessons-learned.md`，结构必含 10 段：

| 段 | 内容 |
|---|---|
| [0] 这份文档是什么 + 怎么用 | 触发更新条件（决议反转 / 生产 bug / 会话误用过期决议 / 新 ADR / RACI 冲突 / 用户反馈不满）；怎么读 |
| [1] 30 秒引导（绝对必读）| monorepo 结构 / 分发路径锁定 / 决议优先级（红档 ADR > spec K-Q > architecture > commit > 历史 todo）/ 角色调度 / 关键边界 |
| [2] 项目级元决议总览 | K-N 跨 sprint 决议 / 红档 K-Q / ADR 索引（全在表里）|
| [3] 已 obsoleted / superseded 决议清单 ★ | **每条**：原决议 + 反转触发引用 + 新决议 + ❌ 禁止做的事；agent 看到 ❌ 项必须忽略 |
| [4] 历史踩坑分类 | 按域分类（环境 / 打包 / 网络 / 异步 / LLM 集成 / SDK / 并发 / 团队 / SDLC / 决策反转 / 上下文管理）|
| [5] 主窗口（助理 / 管家）职责 | 显式 6 条（见下节） |
| [6] 会话压缩 / 上下文重启 复审 ritual | 4 步硬性流程 |
| [7] 引用纪律 | commit / 总结 / audit 引用决议必须精确格式 |
| [8] 反风控约束 | 永远生效的边界（不能关用户安全软件 / 不能并发同物理资源 / 不能 auto-install / 不能修系统）|
| [9] 修订历史 | 每次重大追加在表格里登记 |
| [10] 引用 / 关联文档 | 链 CLAUDE.md / 当前 sprint spec / ADR 目录 / 等 |

**触发追加条件**（任一发生必须立即追加 1 条）：

1. 一个红档决议被反转
2. 一个生产 bug 暴露根因属于"agent 之前应该想到但没想到"
3. 上下文压缩后 agent 误执行已 obsoleted todo
4. 新 ADR 落档（同步把它列入 [2] / 把它废止的旧条目列入 [3]）
5. subagent 协作出现 RACI 冲突 / 越界 / 权限被拒
6. 用户反馈"产品形态 / 流程不满意"

### v4 强化：会话压缩 / 上下文重启 复审 ritual（主窗口必跑）

任何会话压缩或上下文重启后，**第一动作不是干事，而是按 4 步复审**：

```
Step 1 — 读最新 spec K-Q + 最近 1-2 commit + ADR 索引
  git log --oneline -5
  ls specs/<latest-sprint>/adrs/
  cat specs/<latest-sprint>/spec.md   # 看 K-Q 表 / 找"已决"行

Step 2 — 对每条 pending todo 逐条问
  ① 这条在最新决议下还成立吗？      不成立 → obsoleted；成立 → 保留
  ② 引用的 ADR / K-Q 是否仍 ACCEPTED？已 SUPERSEDED → 改写或删除
  ③ 是否有新 ADR 把它替代？          是 → 标 obsoleted_by_<ADR>
  ④ 是否实际已完成（git 历史命中）？  是 → 改 completed

Step 3 — 落档归档
  todo 标 obsoleted → 删除（TodoWrite 不支持 obsoleted 状态）
  同时在 docs/handoff-lessons-learned.md [3] 加一条（如果首次发现）

Step 4 — 报告给用户
  会话开始第一段话必须含：
  ✓ 复审 ritual 完成
  - 删除 obsoleted todo 共 N 条（列出）
  - 当前 pending todo 共 M 条（按优先级）
  - 最新决议状态：ADR-XXX / K-Q-Y.Z / spec [...]
  - 待用户确认 / 直接开干？
```

### v4 强化：引用纪律（commit / 总结 / audit 必读）

| 不规范 | 规范 |
|---|---|
| "之前定的" | "ADR-005 K-Q-5.3 用户 2026-05-07 拍板" |
| "如刚才所说" | "spec [8.1] K-Q-5.4 / ADR-006 通过率 ≥ 80%" |
| "以前讨论过" | "Sprint 4 5g/5h 决议（commit ac67490）" |
| "K10" | "K10 已 SUPERSEDED by K12（ADR-003 / 2026-05-07）" |
| "PyPI yank" | "❌ OBSOLETED by K-Q-4.6 反转（已完成）" |

commit message 模板：

```
type(scope): <slug>

短描述（1-2 行 / why + what）

引用决议链：
- spec sprint-X [N.M] / K-Q-X.Y
- ADR-NNN（status / decided_by / date）
- 父 spec / 父 architecture（如适用）

实施细节...

下一棒：<batch / 角色 / DoR / DoD>

Co-Authored-By: ...
```

### v4 强化：决议反转 = 触发四步文档同步

每次用户反转决议（"我说错了，改成 X"），主窗口必须立刻执行 4 步：

1. `docs/handoff-lessons-learned.md` [3] obsolete 清单加条（含 ❌ 禁止动作列表）
2. 老 ADR `status:` 改为 `superseded_by: ADR-NNN`
3. 当前 sprint spec 的 K-Q 表同步标 SUPERSEDED
4. 整理 forbidden actions 列表（如"禁止 commit message 提 PyPI 发布"），向用户确认

### v4 强化：fatal error 三件套

任何用户面应用 / agent 工具 / 后台任务的 fatal error 必须满足三条：

1. **写文件日志**（不依赖 stderr——windowed 应用无 console 看不到）
2. **弹用户可见对话**（GUI dialog / CLI 明显告警 / 通知通道）
3. **不允许静默 exit**（哪怕 health check 失败也要兜底走默认流程 + 提示用户）

### v4 强化：跨边界自动操作禁令（产品规则）

PreToolUse 安全栏拦的是 dev 操作；产品功能设计层也要遵守同样原则：

- ❌ 不 auto-install 系统组件（运行时 / 浏览器 / 证书等）
- ❌ 不修改系统代理 / 系统证书 / HKLM 注册表 / 系统服务
- ❌ 不要求用户关闭安全软件 / 代理工具 / 防火墙以"配合"产品工作
- ✅ 应用层兜底（如 `--proxy-bypass-list` / 显式声明环境变量 / 应用沙箱内自管资源）

如果发现需求方提出"让用户先关 X 才能用"——这条本身就是问题，回头让 product-strategist 重写需求。

### v5 强化：错位工作 = 模式 / 角色升级信号

主窗口在每个阶段开始 / 完成时**自动审视**：本轮该做这事的人是不是真的在岗。检测以下错位场景立即触发动作：

| 检测到的错位 | 升级动作 |
|---|---|
| 用户被要求做 reviewer / security / perf / docs 工作（如审 ADR / 审代码 / 写测试 / 写 changelog）| → 提示用户切 full 模式或显式召唤对应 agent；**不让用户兜底** |
| 主窗口自己在写实现代码（agent 应做的事） | → 报告"这应是 implementer 工作，我先停下；要我召唤 backend-implementer 吗？" |
| backend-implementer 在改 schema / 写迁移（DBA 工作） | → 停下，让主窗口召唤 migration-specialist |
| 当前角色池没有合适岗位（如项目需要 ML 评估师但 13 角色池没有）| → 提示用户**新增角色定义**，按 v1-2 自由设计 |

**核心**：错位不是"将就着做"，而是"流程缺口的硬信号"。

### v5 强化：流水线默认自动跑完里程碑

主窗口默认**连续推进所有 SDLC 阶段**，不在阶段间问用户"继续吗"。**仅 3 类硬关卡停下**：

1. **关键产品方向决策**：spec 完成后 K-Q 拍板（产品价值取舍 / 优先级 / 范围 in vs out）
2. **早期架构性决策**：design 阶段 5 分钟决策卡片（重大技术选型 / 数据模型重大变更 / 外部依赖引入）
3. **不可逆操作**：合 PR / 上线生产 / 删数据 / 公开发布

**其他全部自动**：
- spec → design 自动转
- design → impl 自动转（决策卡片用户回完后）
- impl → review → QA → security → perf → docs 全链路自动转
- 遇 BLOCKED_BY_* 自动反向调度对应 agent，**不问用户**（除非反向修了 2 轮还 BLOCKED 才上报）

**BLOCKED 上报阈值**：同一阶段连续 BLOCKED 2 次 → 上报用户决策（说明系统级问题）。

### v5 强化：严格 SDLC，lite 不等于轻流程

**任何功能开发都必须经过**：
1. `specs/<slug>/spec.md`（含 AC，product-strategist 产）
2. `docs/adr/ADR-NNN.md`（如有架构决策，tech-architect 产）
3. `specs/<slug>/design.md`（如有 UI / 复杂交互，ux-designer 产）
4. 实现代码（backend / frontend / migration-specialist）
5. `specs/<slug>/review.md`（code-reviewer 产）
6. `specs/<slug>/test-matrix.md`（qa-tester 产）
7. `CHANGELOG.md` 条目 + runbook 草稿（docs-writer 产）

**lite 模式的本质**：默认更少 agent **常驻**，但需要某个 SDLC 步骤的产出时**临时召唤**对应专家——绝对不允许跳过该步骤。

**典型场景**：
- lite 模式下用户提出涉及架构变更的功能 → 主窗口必须临时召唤 tech-architect 写 ADR，**不能省**
- lite 模式下功能涉及 PII / 支付 → 必须临时召唤 security-reviewer 做威胁建模，**不能省**
- lite 模式下功能影响公开 API → 必须召唤 docs-writer 更新文档，**不能省**

**绝对禁止**：口头说说就开始写代码、用 commit message 替代 spec、用代码注释替代 ADR。

### v5 强化：第三方依赖版本兼容性 cross-check（工程规范）

引入 / 升级任何第三方库前，主动查：
- Trove classifier（Python：`Programming Language :: Python :: 3.X`）
- engines / minimum-supported-version（Node.js：`package.json` 的 `engines`）
- platform support matrix
- 已知 incompatibilities（GitHub issues / CHANGELOG.md）

**禁止假设**："新 minor / patch 都向前兼容"——这是错的，第三方库经常在 minor 间 break ABI / 隐藏接口。

**项目举例**：hubstudio-skills 用 Python 3.14 装 pythonnet 失败——pywebview Win backend 依赖 pythonnet 但不兼容 3.14。被迫硬装 Python 3.13 并在 `pyproject.toml` 锁 `requires-python = ">=3.13,<3.14"`。

### v5 强化：长生存周期任务必须显式 lifecycle owner

调度器 / 后台 worker / 长连接 / pub-sub 消费者等 long-running task 必须明确：
- **挂在哪个 event loop**：app 主 loop / web 框架启动 hook / dedicated thread loop
- **谁拉起**：进程启动 hook / 第一次请求 / explicit init
- **谁负责关**：进程退出 hook / 显式 shutdown API / 资源 owner GC

**禁止**挂在临时 loop：`asyncio.run()` 是临时 loop，函数返回时 loop 退出**杀掉所有 task**——长任务挂上去等于自杀。

**项目举例**：hubstudio-skills `_async_boot()` 内 `await scheduler.start()`，函数返回后 `asyncio.run()` 退出杀掉 scheduler；用户提交任务后看到 "Scheduler started" 但 tick 0 次。修复：scheduler 改在 NiceGUI 的 `app.on_startup` hook 里启动，跟 NiceGUI event loop 共生命周期。

### v5 强化：外部接口字段类型不可信，反序列化必须 try-coerce

所有 from_external / from_sdk / from_api 反序列化层必须：
- 字段类型用 Pydantic field_validator 显式 coerce（`str(raw)` / `int(raw)`）
- 缺失字段给默认值（`raw.get("field", default)`）
- 类型错误给可读 error（不要让上层看到 ValidationError stack trace）

**禁止假设**：第三方 API 返回类型与文档一致——文档过时 / 实际实现不同步是常态。

**项目举例**：HubStudio API 文档写 `containerCode: string`，实际返回 `int`（`1443245528`），Pydantic ValidationError 整链中断；修：`from_sdk_dict()` 显式 `str(raw.get("containerCode", ""))` coerce。

### v5 强化：SDK / 外部资源操作必须 idempotent + 残留恢复路径

任何 open / close / acquire / release 类 SDK 操作必须有三层 fallback：
1. **复用已存在资源**（pool / cache / process-local registry）
2. **正常 open**（fresh state 路径）
3. **残留状态恢复**：catch "already running" / "already exists" → cleanup → 重试 1 次

**禁止假设**：每次都是 fresh state——前一轮 crash / network drop / 用户手工干预都会留残留。

**项目举例**：`SdkFacade.open(test01)` 遇到已 running container → `EnvironmentStartError "上次未结束"`；修：三层 fallback（复用进程内 pool session → 正常 open → 残留恢复 stop_browser → 重试 1 次）。

### v5 强化：物理 / 外部资源并发管控

任何对**物理资源**（设备 / 容器 / 第三方账号 / 限额 API / 浏览器实例）的访问必须有并发控制：
- **per-resource lock**：每个 resource_id 一把锁，等同 resource 操作排队
- **全局串行**：scheduler concurrency = 1，慢但稳；适合资源数小或并发风险高的场景

**默认值**：scheduler concurrency = 1（先稳，等观测数据稳定再升）。

**禁止**：默认 concurrency = N（N>1）— 早期容易踩"多 task 抢同一物理资源"的坑，且故障难复现。

**项目举例**：scheduler tick 同时分发多 task 到同一 container_code → HubStudio "环境正在运行中" 反复抢；修：`tick_once` 顶部加 `if self._inflight: return 0` 全局串行。后续观察稳定再升级 per-container token 锁。

### v5 强化：agent / scheduler / registry 完整性 cross-check

**核心命题**：agent 能干啥 = tools / registry 暴露啥；缺工具就缺能力。

**强制审计点**：
- 每次 architecture 改动 / 新增功能域 / 新增子系统 → 必须做 registry tool inventory check
- 每个 agent 描述里的"职责"必须能映射到 tools 字段里的具体工具
- 缺口要么补 tool，要么明示"该角色不能做 X，需调度 Y agent"

**禁止假设**：agent 看到 prompt 里写"完成 X 任务"就一定能做——做不到的工具调不出来，只会反复尝试无效路径。

**项目举例**：hubstudio-skills 的 agent 想列环境却 registry 没 list_environments / open_session / close_session 三个工具，结果 agent 反复打开秒关看似"无动作"；用户实际反馈"agent 跟没装一样"。修：registry 加 3 工具，从 8 → 11。

### v5 强化：三向决议一致性日常审计（v4-6 扩展）

v4-6 只覆盖了"决议反转时"的同步；v5 扩展为**正常演进时也要审计**：

- 状态流转门禁（如 `READY_FOR_DESIGN → READY_FOR_IMPL`）必须有 SDLC 预审 cross-check：ADR + spec K-Q + architecture **三处一致**
- spec 修订（spec.md 改动）必须 cross-check 引用方（architecture / ADR / 既有代码）是否还成立
- ADR 修订（status / 关键参数）必须 cross-check 下游（spec / architecture / 实现）

**禁止**：以为"只有反转才需要同步"——日常微调也会引入三向漂移。

**项目举例**：hubstudio-skills architecture v6 写时引旧值 70%（test-strategy 初稿），但新 ADR-006 已决 80%（用户拍板更新过）；老九 SDLC 预审才发现三向冲突，修了 architecture v6 三处 70% → 80%（commit `0061c87`）。

### v5 强化：请求用户决策前必须提供方案（统一决策卡片格式）

v2-3 把决策卡片定义为 design 阶段的子流程；v5 **把这格式扩展为所有 stop-and-ask 用户的硬要求**。任何主窗口需要用户输入 / 决策 / 拍板 / 拨付资源的时候，必须按下面的卡片模板组织提问：

```
[决策点] <一句话问题>

选项：
  A) <方案 A 一句话> ——【推荐】<推荐理由 1 句>
  B) <方案 B 一句话> ——<选 B 的合理场景 1 句>
  C) <方案 C，可选>

取舍：
  - 选 A 的代价：<...>
  - 选 B 的代价：<...>

不做这件事的后果：<延期 / 阻塞 / 风险一句话>

reviewer must-fix（如有）：<...>

我需要你回：A / B / C / 改 X
```

**禁止以下问法**：
- ❌ "我们接下来该怎么办？"
- ❌ "你说选哪个？"
- ❌ "需要你确认一下"（缺方案 + 缺选项 + 缺取舍）
- ❌ 把 1000 行 ADR 链接甩给用户让其自审

**为什么这条硬性化**：用户角色是产品方向决策者（v2-2），不是 reviewer / security / arch / test 工程师。把"开放式问题"翻译成"5 分钟决策卡片"是主窗口（管家）的核心职责。

### v5 强化：禁止使用 § 符号（及其它难读 / 难输入符号）

**绝对禁止**在任何产物中使用 `§` 符号——读者读不出（"section 几"还是"段几"？），用户键盘也输不出。

**等价替代**：
- 表达"段落 / 节" → `第 X 段` / `第 X 节` / `[X.Y]` / `#X` / markdown `## 标题`
- 表达"脚注" → markdown footnote 语法 `[^1]` 或行内 `（注：...）`
- 表达"注释" → `（注：...）` / `// ...` / `# ...`

**也禁止**类似难读符号：`‡`（双剑号）、`¶`（段标）、`⁂`（星号花）、`℡`（电话符号）等。

**适用范围**：spec / ADR / design / commit message / code comments / docs / README / 任何 markdown 文档 / 任何代码生成。

**为什么这条硬性化**：用户全局 `~/.claude/CLAUDE.md` 已写"禁止 §"，但 subagent 不读全局；过往项目大量产出含 § 后期手工 sed 删。v5 升格到顶层 HANDOFF，新项目生成时 product-strategist / tech-architect / docs-writer 等都必须在自身 prompt 里继承这条。

### 关键设计决策

1. **底层英文，用户层中文**：name/文件名/hook matcher 用英文 kebab-case；中文岗位名通过 description 字段承载

2. **两档模式开关 + v2 主动提示**：
   - lite 模式：仅核心角色（按你裁剪结果中标注 ★ 的）
   - full 模式：全部角色
   - 用 `.claude/pipeline-mode` 文件控制
   - 提供 `/mode-lite` `/mode-full` `/mode-status` `/feature` 四个 slash 命令
   - **主窗口主动监测**当前任务需要的角色是否在岗；如需要 full 模式角色但当前在 lite 模式，**主动提示用户切换**并解释为什么需要。不让用户被迫切。

3. **Hook 设计**：
   - 每个 agent 完成后用 SubagentStop hook 输出"下一步"建议
   - 提示文本中文化，用「」中文方括号包围岗位名（避免 bash 双引号嵌套）
   - 模式感知：不同模式给出不同的"下一步"
   - BLOCKED 状态时停止推进，不硬继续

4. **PreToolUse 安全栏**（始终启用）：
   拦截：`git push [任意 flag] origin main/master/production`、`git push [任意 flag] (--force|-f|--force-with-lease)`、`gh pr merge`、`kubectl apply/delete prod`、`terraform apply/destroy`、`aws ... --profile prod`、`rm -rf /`、`sudo`、`dd if=`、`mkfs`、`chmod 777`、`truncate ... production`、应用商店上传命令（`xcrun altool --upload-app` / `fastlane release|deliver`）、删除签名/证书文件（`*.jks` / `*.p8` / `*.p12` / `*.pem`）

5. **状态机驱动协作 + v2 强化**：
   - PLAN.md 是全局看板
   - spec 文件顶部 Status 字段是消息总线
   - 状态命名清晰可追踪，BLOCKED_BY_<阶段> 表示卡住
   - **PLAN.md 更新原则（v2 强化）**：PLAN.md 只在 stage 转换 / 重大里程碑 / 用户拍板时更新；subagent 不直接写 PLAN.md（即使内容质量高也不应越界），agent 想给 PLAN.md 加内容时在汇报中提建议，由主窗口决定是否落盘
   - **design 阶段「用户签字」流程（v2 新增）**：design 阶段的"用户签字"门禁**不是**让用户读 ADR/design 全文。正确流程：
     1. tech-architect 交付 design.md + ADR 后，主窗口主动派 code-reviewer + security-reviewer 双签**提前介入**审（design-phase review，不是 impl 后 PR review）
     2. 双 reviewer 各自产出 `review-notes-design-phase.md` + `security-review-design-phase.md`
     3. 主窗口拿到双 reviewer 报告后，给用户写 **5 分钟决策卡片**：每个关键决策一张卡片（一句话问题 + 选项 + 取舍 + reviewer 的 must-fix）
     4. 用户回 ✓ / ✗ / 改 X
     5. 用户回完 → 主窗口汇总反馈给架构师做 v+1 修正（如有）→ 通过则 stage 进入 impl
   - **禁止**：主窗口直接把 1000+ 行 ADR 链接甩给用户让其自审

6. **职责硬隔离**：每个产出物只有一个 writer，多个 reader（spec/ADR/契约/迁移/代码/测试 各有归属，禁止单方面跨界修改）

### CLAUDE.md 必须填实（非占位）+ v2/v4 强化

基于第一阶段调研，CLAUDE.md 必须包含**真实**内容：
- 项目简介（一句话价值定位 + 当前阶段 + 团队规模）
- 真实技术栈（不是 "Node.js 20 / TypeScript"，而是项目里 package.json/pom.xml/Cargo.toml 实际使用的版本）
- 真实目录约定（基于项目当前目录结构，不是模板里的 src/server）
- 真实常用命令（基于项目里 package.json/Makefile 真实可跑的命令，不是 "npm run lint"）
- 团队介绍（实际选择的 N 个虚拟同事，含中文呼叫示例）
- 状态机（基于实际选择的角色调整）
- 安全与边界规则
- Git 约定（如已有约定就遵循，没有就给个默认）

**v2 强化：** CLAUDE.md 生成前必须做两件事：

1. **Read `~/.claude/CLAUDE.md`**（用户全局规则），把所有硬性格式规则、安全规则、术语规则**逐条镜像**到项目 CLAUDE.md。subagent 不读全局，需要项目级兜底。
2. **必含「用户角色边界」章节**，明确：
   - 用户做：产品方向决策 / 硬约束把关 / 关键事实校对 / 最终签字 / 外部资源拨付
   - 用户不做：读 ADR / design.md / 接口签名全文 / code review / security review / 测试编写 / 双签
   - 主窗口的责任：任何需用户决策的事，必须翻译成 ≤ 5 分钟决策卡片（一句话问题 + 选项 + 取舍 + 推荐 + must-fix）

**v4 强化：** CLAUDE.md 还必须包含「主窗口（助理 / 管家）职责」章节，6 条硬约束：

1. **审视而非搬运**：每条 todo / spec / commit message 引用必须 cross-check 最新 ADR / spec K-Q 状态。看到与现行决议矛盾的旧文本 → 主动标 OBSOLETED + 提请确认。
2. **主动归档**：已废止决议 / 已完成 todo / 已替代 ADR → 移到 `docs/handoff-lessons-learned.md` [3] 已 obsoleted 清单。不让历史污染未来动作。
3. **强制引用纪律**：所有 commit message / 总结 / audit 报告引用决议必须精确到 `ADR-NNN` / `K-Q-X.Y` / `spec [N.M]` / `commit XXX`。不允许"之前定的""如刚才所说"模糊引用。
4. **多 agent 协作 RACI 把关**：dispatch agent 前预审 R/A/C/I + 看权限边界（参见 v4 强化「派单前权限预审」）；agent Write 被拒要明示原因 + 主窗口接管。
5. **会话压缩 / 重启复审 ritual**：见 v4 强化「会话压缩 / 上下文重启 复审 ritual」段。第一动作不是干事而是复审。
6. **被动 → 主动**：从"列出所有 pending"升级到"主动审视 / 提议归档 / 提请决策"。助理不是 reminder bot。

### 技术约束验证

生成完成后必须做：
1. 用 `find . -type f -newer ...` 列出所有新创建文件
2. 用 `python3 -c "import json; json.load(open('.claude/settings.json'))"` 校验 JSON
3. 实际跑至少 3 个 hook 命令（`bash -c "<command>"`）验证中文输出无引号嵌套问题
4. 检查 CLAUDE.md 中没有残留的 `<!-- TODO -->` 或 `__________` 占位
5. 给我"接下来要做的事"清单：
   - 重启 Claude Code 会话（必做）
   - 跑 `/agents` 验证识别
   - 跑 `/mode-status` 验证开关
   - 第一个 `/feature` 试跑场景建议
   - Git 提交建议（哪些文件该 commit、哪些建议 gitignore）

## 开始

先做第一阶段调研。如果项目里有 README、package.json、其他工具的配置文件，请先读取它们；如果有 git 历史，可以用 `git log --oneline -20` 看近期开发节奏。调研后总结画像给我，等我确认。

如果在调研中你发现项目类型很特殊（比如 Chrome 扩展、Unity 游戏、Rust 嵌入式等模板池没覆盖的），自由提议合适的角色组合，不要硬套 SaaS 模板。

第二阶段角色裁剪通过后，**记得先产出 `specs/_assumptions.md` 让我校对再写 backlog**（v2 引入的硬性流程，已在 v3 沿用，避免后期事实层修正引发返工）。

============================================================

---

## 用完之后

生成完成后，AI 会给你一份"接下来要做的事"清单。无论它给的清单是什么，至少做这几件事：

1. **完全重启 Claude Code**（不是 `/clear`），让新的 agent / commands / hooks 加载
2. 跑 `/agents` 看到所有虚拟同事
3. 跑 `/mode-status` 看到当前模式
4. 用一个最简单的功能试跑 `/feature`（比如"加一个 health check 端点"）走完整链路一次，验证 hook 串联正常
5. `git add .claude CLAUDE.md PLAN.md TEAM.md README.md specs/ docs/ && git commit -m "chore: add Claude Code scaffolding (HANDOFF v5)"`

## 故障排查速查

| 现象 | 可能原因 | 修法 |
|---|---|---|
| `/agents` 空空如也 | 没在项目根目录启动 Claude Code | `cd` 到根目录后重启 |
| 中文岗位名调不通 agent | description 没写中文标签 | 编辑对应 agent 文件，加 `【岗位】(别名: ...)` 前缀 |
| Hook 输出被截断或乱码 | bash 双引号嵌套冲突 | 中文用「」不用 `"` 包围 |
| `/mode-lite` 报"command not found" | `.claude/commands/` 没加载 | 完全重启 Claude Code（不是 /clear） |
| settings.json 报错 | JSON 格式不合法 | `python3 -c "import json; json.load(open('.claude/settings.json'))"` 定位 |
| CLAUDE.md 还是占位 | AI 跳过了第一阶段调研 | 让它重做：`> 重新按 HANDOFF 第一阶段调研，CLAUDE.md 必须实填` |
| 主窗口让用户审 ADR 全文 | AI 没遵守 design 阶段 review 子流程（v2 引入） | `> 按 HANDOFF v5 design 阶段流程：派 reviewer 双签 + 给我决策卡片，不要让我读 ADR` |
| 主窗口动不动甩"接下来怎么办"开放式问题 | AI 没遵守 v5-11 决策卡片格式 | `> 按 v5-11 决策卡片：问题 + 选项 + 推荐 + 取舍 + must-fix，不要开放式问` |
| 产物里出现 § 符号 | AI 没遵守 v5-12 | `> 按 v5-12 把所有 § 替换为"段 / 节 / [X.Y] / ## 标题"`  |
| lite 模式跑着跑着用户被迫审 ADR / 写测试 | AI 没遵守 v5-1 错位升级 | `> 按 v5-1：这是错位信号，临时召唤 reviewer / qa-tester，或切 full 模式` |
| 主窗口每跨阶段都问"继续吗" | AI 没遵守 v5-2 自动跑里程碑 | `> 按 v5-2：除关键产品决策 / 早期架构决策 / 不可逆操作 三类硬关卡，其它全自动` |
| lite 模式下跳过 ADR 直接写代码 | AI 没遵守 v5-3 严格 SDLC | `> 按 v5-3：lite 不是轻流程，需要 ADR 时临时召唤 tech-architect，不能略过 SDLC 步骤` |
| 重启会话后旧 todo 被当 pending 列出（含已废止决议）| AI 没遵守 v4 复审 ritual | `> 跑 v4 复审 ritual：先读最新 ADR + obsolete 清单，再报告 N 删 / M 留` |
| commit message / summary 引用模糊（"之前定的""刚才所说"）| AI 没遵守 v4 引用纪律 | `> 改写引用为 ADR-NNN / K-Q-X.Y / commit-SHA 精确格式` |
| 主窗口被动列 todo 不主动审视 | AI 没遵守 v4-5 主窗口职责 | `> 按 v4 主窗口 6 条职责：先审视 / 归档 / 提请决策再列 pending` |
| 全局 CLAUDE.md 规则 subagent 没遵守 | AI 没把全局镜像到项目级 | `> 把 ~/.claude/CLAUDE.md 的硬规则镜像到项目 CLAUDE.md，subagent 才能看到` |
| 用户给的事实修正引发产物大返工 | AI 跳过了 _assumptions.md 校对 | 重启项目时强制走 v2 第二阶段 assumptions 流程 |

---

## v3 变更日志（融合两个项目）

v3 不是新项目踩坑产出，而是**文档级融合**——把 v1 的隐性踩坑（藏在「设计意图」段）和 v2 的 12 条显性 changelog 摆在同一桌面：

| v3 做的事 | 解决的问题 |
|---|---|
| 把 v1 的 5 条「设计意图」反向写成 7 条 changelog（即下一节）| v1 原文只解释 "为什么这么设计"，不说 "因为踩了什么坑"；新人难判断规则刚性 |
| 顶部加「两个项目踩坑速览表」（v1-1~v1-7 + v2-1~v2-12）| 7+12 条规则散落各章节，缺一个总索引 |
| 「设计意图」每条注脚 → 具体踩坑编号 | 读者读到设计动机时能一键跳到对应踩坑故事 |
| 版本谱系明确 v0 → v1 → v2 → v3 演化 | 之前 v0 / v1 / v2 关系散在文中各处，新人理不清 |
| 口令本体保持 v2 内容（v2 已是 v1 超集）| 不引入未经实战验证的新规则——v3 是**整理版本**而非**新功能版本** |

**v3 不变之处**：所有规则的具体内容、口令分阶段流程、agent 七段式、ADR 模板、安全栏黑名单、故障排查表——这些都是 v1+v2 已经稳定的产物，v3 不再修改，只重新组织呈现。

> **v4 触发条件已满足**（hubstudio-skills 项目暴露 v1+v2 没覆盖的"决议自传播 / 会话重启误用"类坑）→ 见下面「v4 变更日志」。

---

## v4 变更日志（hubstudio-skills monorepo 项目踩坑）

本版基于 2026 年 5 月「hubstudio-skills monorepo」项目（NiceGUI 桌面应用 + 3 个 Markdown skill 的 monorepo / 公司内部分发）的 30+ 条具体踩坑提炼出的 8 条**通用规则**。剔除了 PyInstaller / NiceGUI / WebView2 / HubStudio SDK / Clash 等技术栈特异性条目，保留任何项目都适用的"长期记忆 + 流程纪律"类规则。

> **触发反思的事件**：用户在 Sprint 5 末问"怎么又要 PyPI？"——主窗口在新会话 summary 里把已完成的"PyPI yank v0.1.0"当成 pending todo 重新列出。这不是单点错误，是**决议在 commit / summary / todo 之间自传播**的系统性问题，v4 因此沉淀。

### 规则 v4-1：Living lessons-learned 文档（项目级长期资产）
**坑**：项目演进中的踩坑、已废止决议、流程缺陷散落在各 ADR / commit / sprint spec 里。新会话 agent 进入项目时无统一入口读"什么不能做"。
**机制**：仓库根 `docs/handoff-lessons-learned.md` 作为 LIVING DOCUMENT，10 段固定结构（30s 引导 / 决议索引 / obsolete 清单 / 历史踩坑分类 / 主窗口职责 / 复审 ritual / 引用纪律 / 反风控约束 / 修订历史 / 关联文档）。每次决议反转 / 生产 bug / 会话误用 / 新 ADR / RACI 冲突 / 用户反馈不满 → 立即追加 1 条。**agent 进入会话第一动作 = 读 [3] obsolete 清单 + [6] 复审 ritual**。
**修补位置**：第三阶段「v4 强化：Living lessons-learned 文档」段。

### 规则 v4-2：决议 obsolete 清单显式维护
**坑**：用户拍板"v0.1 走 PyPI 公开发布"→ 后期反转"PyPI 必须删除立刻删除"→ 老 todo / 老 commit message / 老 summary 仍然散布"PyPI yank""上传 v0.X PyPI"等动作，被新会话当待办复用。
**机制**：每条 obsolete 条目格式：原决议 + 反转触发引用（含原话）+ 新决议（带 ADR-NNN）+ ❌ **禁止做的事**列表。agent 看到 ❌ 项即使在历史 commit 里出现也忽略，不能"从 git 历史推断"。
**修补位置**：第三阶段「v4 强化：决议反转 = 触发四步文档同步」段。

### 规则 v4-3：会话压缩 / 上下文重启复审 ritual
**坑**：上下文压缩或会话重启后，主窗口生成的 summary 把"已完成 / 已 obsoleted / 仍 pending"摆成平级文本。agent 机械搬运 todo，不查最新决议状态。
**机制**：4 步硬性流程（Step 1 读最新 ADR → Step 2 逐条 cross-check pending todo → Step 3 标 obsoleted 归档 → Step 4 第一段话报告"删 N 留 M 新增 K"）。**做完才能干事**。
**修补位置**：第三阶段「v4 强化：会话压缩 / 上下文重启 复审 ritual」段。

### 规则 v4-4：引用纪律
**坑**："之前定的""刚才所说""以前讨论过"等模糊引用让 obsoleted 决议被当现状复用，权威性无法追溯。commit message 写"PyPI publish"被后续 summary 当事实复用，自传播。
**机制**：所有 commit / summary / audit 引用决议必须精确到 `ADR-NNN` / `K-Q-X.Y` / `spec [N.M]` / `commit-SHA`。已 obsoleted 引用必须带 `OBSOLETED` 标签。commit message 模板包含"引用决议链"段。
**修补位置**：第三阶段「v4 强化：引用纪律」段。

### 规则 v4-5：主窗口 = 助理 / 管家而非 reminder bot
**坑**：主窗口把"PyPI yank v0.1.0"（已 obsoleted）当 pending todo 列出。这是"被动 reminder bot"行为——只列出，不审视。
**机制**：6 条职责显式写入 CLAUDE.md（审视而非搬运 / 主动归档 / 强制引用纪律 / 多 agent 协作 RACI 把关 / 会话重启复审 / 被动 → 主动）。
**修补位置**：第三阶段「CLAUDE.md 必须填实（非占位）+ v2/v4 强化」段。

### 规则 v4-6：决议反转 = 触发四步文档同步
**坑**：用户反转决议时只改了 ADR 一处，spec K-Q 表 / architecture / Living doc 没同步，三向分裂。例如架构师 v6 引旧 70%，新 ADR-006 已决 80%，老九 SDLC 预审才发现。
**机制**：反转决议 4 步（Living doc 加 obsolete 条 + 老 ADR 改 SUPERSEDED + spec K-Q 表同步 + 整理 forbidden actions 列表）。四步缺一阻塞 stage 流转。
**修补位置**：第三阶段「v4 强化：决议反转 = 触发四步文档同步」段。

### 规则 v4-7：fatal error 三件套
**坑**：windowed 应用（NiceGUI desktop / .exe）启动失败秒退，无错误窗口、无日志、stderr 看不到。用户和 agent 都无从排查根因。
**机制**：任何用户面应用 / agent 工具 / 后台任务 fatal error 必须 (a) 写文件日志 (b) 弹用户可见对话 (c) 不允许静默 exit。windowed 应用不能依赖 stderr。
**修补位置**：第三阶段「v4 强化：fatal error 三件套」段。

### 规则 v4-8：跨边界自动操作禁令（产品规则）
**坑**：早期方案让用户"先关 Clash 系统代理才能用本应用"——但用户的 HubStudio 指纹浏览器需要 Clash，**关 Clash = 阻塞所有上游工作**。是产品需求层面的边界违规，不是代码层面能修的 bug。
**机制**：v1+v2 时代 PreToolUse 安全栏拦的是 dev 操作；v4 把同样原则升级为**产品规则**——不 auto-install 系统组件 / 不修系统代理-证书-注册表 / 不要求用户关闭安全软件或代理工具。应用层兜底（`--proxy-bypass-list` / 显式环境变量 / 沙箱内自管资源）。如需求方提"让用户先关 X"，回头让 product-strategist 重写需求。
**修补位置**：第三阶段「v4 强化：跨边界自动操作禁令」段。

> **v5 触发条件已满足**（用户在 v4 落地后基于本仓库自身演进 + 三个项目踩坑回顾，提出 12 条新规则）→ 见下面「v5 变更日志」。

---

## v5 变更日志（本仓库自身演进 + 用户反馈深化）

本版基于 2026 年 5 月用户对 v4 落地后的复盘反馈与本仓库自身演进经验。新增 12 条规则——3 条工作流纪律 + 7 条工程规范（v4 时舍弃但用户认为应按"通用方法论 + 项目举例"加回）+ 2 条用户交互纪律。

> **触发反思的事件**：v4 落地后用户提出 4 项确认 + 2 项补充——希望严格 SDLC（不允许 lite=轻流程）、流水线默认自动跑（仅硬关卡停）、错位工作=升级信号、加回 v4 时舍弃的工程规范、请求用户决策前必须提供方案、禁止 § 符号。这 6 项痛点都不是单点修复能搞定的，需要系统级规则升级。

### 规则 v5-1：错位工作 = 模式 / 角色升级信号
**坑**：v2-5 只覆盖了"主窗口主动监测当前任务需要的角色是否在岗"——这是被动观察。实践中流水线带着错位硬跑：用户被迫审 ADR、主窗口被迫写实现代码、backend 被迫改 schema。
**机制**：错位检测 → 立即升级动作（lite→full 切换 / 召唤对应 agent / 提示用户新增角色）。**核心**：错位不是"将就着做"，是"流程缺口的硬信号"。
**修补位置**：第三阶段「v5 强化：错位工作 = 模式 / 角色升级信号」段。

### 规则 v5-2：流水线默认自动跑完里程碑
**坑**：v1+v2 设计是"每跨阶段问一次用户"，碎片化严重——用户疲于应付"继续吗 / 继续吗 / 继续吗"，且每次都要重新 context-load 才能答。
**机制**：主窗口默认连续推进所有 SDLC 阶段，**仅 3 类硬关卡停**：① 关键产品方向决策 ② 早期架构性决策（决策卡片）③ 不可逆操作（合 PR / 上线 / 删数据）。其他全自动。BLOCKED_BY_* 默认自动反向调度，连续 BLOCKED 2 次才上报用户。
**修补位置**：第三阶段「v5 强化：流水线默认自动跑完里程碑」段。

### 规则 v5-3：严格 SDLC，lite 不等于轻流程
**坑**：v1 时代有人把 lite 当"省事模式"，跳过 ADR 直接写代码；后期回头补 ADR 发现实现已经偏离了未声明的架构假设；返工成本大于省下的 ADR 时间。
**机制**：任何功能都必须 spec → (ADR) → design → 实现 → review → test-matrix → release-notes，**一个不能少**。lite 仅是默认更少 agent **常驻**；某 SDLC 步骤需要 lite 之外的角色时**临时召唤**该专家——绝对禁止跳步骤。
**修补位置**：第三阶段「v5 强化：严格 SDLC，lite 不等于轻流程」段。

### 规则 v5-4：第三方依赖版本兼容性 cross-check
**坑**：hubstudio-skills 用 Python 3.14 装 pythonnet 失败（pywebview Win backend 依赖 pythonnet 但不兼容 3.14），被迫硬装 Python 3.13 并锁 `requires-python = ">=3.13,<3.14"`。
**机制**：引入 / 升级第三方库前主动查 trove classifier / engines / minimum-supported-version / 已知 incompatibilities；不能假设新 minor / patch 都向前兼容。
**修补位置**：第三阶段「v5 强化：第三方依赖版本兼容性 cross-check」段。

### 规则 v5-5：长生存周期任务必须显式 lifecycle owner
**坑**：hubstudio-skills `_async_boot()` 内 `await scheduler.start()`，函数返回后 `asyncio.run()` 临时 loop 退出，杀掉 scheduler；用户提交任务后看到 "Scheduler started" 但 tick 0 次，排查耗时长。
**机制**：scheduler / worker / 长连接等 long-running task 必须挂在长生存周期 event loop（明确 owner / 拉起者 / 关闭者）；不能挂在 `asyncio.run()` 临时 loop。
**修补位置**：第三阶段「v5 强化：长生存周期任务必须显式 lifecycle owner」段。

### 规则 v5-6：外部接口字段类型不可信，反序列化必须 try-coerce
**坑**：HubStudio API 文档写 `containerCode: string`，实际返回 `int`（`1443245528`）；Pydantic ValidationError 整链中断；agent 反复调用看似无果。
**机制**：所有 from_external / from_sdk / from_api 反序列化层必须用 field_validator 显式 coerce + 缺失字段给默认 + 类型错误给可读 error；不能假设第三方返回类型与文档一致。
**修补位置**：第三阶段「v5 强化：外部接口字段类型不可信」段。

### 规则 v5-7：SDK / 外部资源操作必须 idempotent + 残留恢复路径
**坑**：`SdkFacade.open(test01)` 遇到已 running container → `EnvironmentStartError "上次未结束"`；agent 看到错就放弃，用户被迫手工 stop 残留 container 后才能继续。
**机制**：任何 open / close / acquire / release 类 SDK 操作必须三层 fallback：① 复用已存在 ② 正常 open ③ 残留恢复（catch "already running" → cleanup → 重试 1 次）。
**修补位置**：第三阶段「v5 强化：SDK / 外部资源操作必须 idempotent + 残留恢复路径」段。

### 规则 v5-8：物理 / 外部资源并发管控
**坑**：scheduler tick 同时分发多 task 到同一 container_code → HubStudio "环境正在运行中" 反复抢；scheduler 默认 concurrency=5 直接踩雷。
**机制**：物理资源访问必须有并发控制——per-resource lock 或全局串行；scheduler concurrency 默认 1（先稳，等观测稳定再升 per-resource token 锁）。
**修补位置**：第三阶段「v5 强化：物理 / 外部资源并发管控」段。

### 规则 v5-9：agent / scheduler / registry 完整性 cross-check
**坑**：hubstudio-skills agent 想列环境却 registry 没 list_environments / open_session / close_session 三个工具；agent 反复尝试无效路径，用户实际反馈"agent 跟没装一样"。
**机制**：架构层任何改动 / 新增功能域伴随 registry tool inventory check；agent 描述里的"职责"必须能映射到 tools 字段里的具体工具，缺口要么补 tool 要么明示"该角色不能做 X，需调度 Y agent"。
**修补位置**：第三阶段「v5 强化：agent / scheduler / registry 完整性 cross-check」段。

### 规则 v5-10：三向决议一致性日常审计（v4-6 扩展）
**坑**：v4-6 只覆盖了"决议反转时"的同步；hubstudio-skills 实践中 architecture v6 写时引旧值 70%（test-strategy 初稿），新 ADR-006 已决 80%（用户拍板过）；老九 SDLC 预审才发现三向冲突。说明只在反转时同步不够，正常演进也要查。
**机制**：状态流转门禁（`READY_FOR_DESIGN → READY_FOR_IMPL` 等）必须有 SDLC 预审 cross-check ADR + spec K-Q + architecture 三处一致；spec / ADR 修订必须 cross-check 引用方与下游。
**修补位置**：第三阶段「v5 强化：三向决议一致性日常审计」段。

### 规则 v5-11：请求用户决策前必须提供方案（统一决策卡片格式）
**坑**：v2-3 把决策卡片定义为 design 阶段子流程；但实践中主窗口在其它阶段也"动不动甩开放式问题给用户"——"接下来怎么办？""你说选哪个？"。用户被迫从零想方案，体验差且违反 v2-2 用户角色边界。
**机制**：所有 stop-and-ask 用户必须按统一卡片模板——一句话问题 + 至少 2 选项（含推荐）+ 取舍 + 不做的后果 + reviewer must-fix（如有）。**禁止**开放式提问 / 把 ADR 全文链接甩给用户。
**修补位置**：第三阶段「v5 强化：请求用户决策前必须提供方案」段。

### 规则 v5-12：禁止使用 § 符号（及类似难读 / 难输入符号）
**坑**：用户全局 `~/.claude/CLAUDE.md` 已写"禁止 §"，但 subagent 不读全局；过往项目大量产出含 § 文档（"§4.3 加密策略"），后期手工 sed 批量删。v2-1 的"镜像全局规则"机制覆盖此条，但每个项目都要重做一次。
**机制**：v5 把这条具体规则**单独升格到顶层 HANDOFF 硬规则**，spec / ADR / design / commit / docs / 代码注释一律不许出现 §（及类似 ‡ ¶ ⁂ 等难读符号）；用 `第 X 段` `[X.Y]` `#X` `## 标题` 替代。新项目生成时该规则自动继承到 product-strategist / tech-architect / docs-writer 的 prompt 里。
**修补位置**：第三阶段「v5 强化：禁止使用 § 符号」段。

> **下次升级到 v6 的触发条件**：新项目（非首四批）实战中暴露 v1+v2+v4+v5 没覆盖的坑，或本仓库自身演进出新场景——届时按本日志格式追加 v6 changelog。

---

## v1 变更日志（首个项目踩坑反向陈述）

v1 在 v0「克隆-改装」式口令的基础上重写为三阶段流程，每条改动都对应首个项目实践中的真实痛点：

### 规则 v1-1：强制三阶段调研
**坑**：v0 时代 AI 直接 `git clone` 本仓库再适配，结果 SaaS 模板里的目录约定、命名风格、状态机粒度强行套到目标项目，产出与项目脱节，用户大量手工改。
**修补位置**：第一阶段「项目调研」前置门禁。

### 规则 v1-2：角色池可自由设计
**坑**：v0 把 13 个 SaaS 角色硬塞到 ML / 嵌入式 / 移动 App / Chrome 扩展项目，多余角色（设计师/SRE）拖累流水线，缺失角色（数据工程师/平台审核工程师）让用户被迫手工补。
**修补位置**：第二阶段「按需裁剪角色清单」末段「自由设计角色」。

### 规则 v1-3：CLAUDE.md 必须实填
**坑**：v0 留下大量 `<TODO: 例如 PostgreSQL 16>` 占位，AI 完成生成就退出，用户后期才发现 CLAUDE.md 是空壳，subagent 拿不到真实背景仍然瞎猜。
**修补位置**：第三阶段「CLAUDE.md 必须填实（非占位）」段。

### 规则 v1-4：默认 lite 模式 + 模式开关
**坑**：v0 默认全员 13 角色上场，原型/小项目单次成本 ~$15-25 不可控；用户被迫每次手工 disable 一半 agent。
**修补位置**：第三阶段「关键设计决策」第 2 项「两档模式开关」。

### 规则 v1-5：必须实跑 hook bash 命令验证
**坑**：v0 AI 写完 settings.json 就交差，但中文 hook 在 bash 双引号嵌套下经常乱码 / 截断 / EOF 错误，用户启动 Claude Code 后才发现 hook 全挂。
**修补位置**：第三阶段「技术约束验证」第 3 项。

### 规则 v1-6：故障排查速查表
**坑**：v0 用户重复遇到「`/agents` 空空、`/mode-lite` 报 not found、settings.json 报错、CLAUDE.md 还是占位」等高频问题，每次都要重新求助。
**修补位置**：本文件「故障排查速查」段（v2 时已扩充）。

### 规则 v1-7：第二阶段角色裁剪强制用户确认
**坑**：v0 AI 自行决定裁剪结果（甚至不告知用户），最终团队组合常与用户实际期望偏差。
**修补位置**：第二阶段末段「裁剪完后...等我确认」硬性门禁。

---

## v2 变更日志

本版基于 2026 年 5 月「摩托车导航屏配套 App」项目踩坑总结。原 v1 内容保留，v2 新增 12 条强化规则，每条都对应实际踩过的坑：

### 规则 1：CLAUDE.md 镜像全局规则
**坑**：用户全局 `~/.claude/CLAUDE.md` 已写"禁止使用 § 符号"，但 subagent 只读项目 CLAUDE.md（不读全局），导致大量产出含 § 文档，后期批量 sed 删除。
**修补位置**：第三阶段「CLAUDE.md 必须填实」段。

### 规则 2：用户角色边界章节
**坑**：主窗口让用户审 1500+ 行 ADR，违反主窗口宪法第 5 条（用户角色错位）。
**修补位置**：第三阶段「CLAUDE.md 必须填实」段。

### 规则 3：design 阶段 review 子流程
**坑**：v1 状态机只说"design 阶段用户签字"没说怎么签，主窗口直接甩 ADR 链接给用户。
**修补位置**：第三阶段「关键设计决策」第 5 项「状态机驱动协作」。

### 规则 4：ADR 模板加 superseded_by + revision_history
**坑**：ADR-001 v1 / v2 共存同一文件，第三方读者不知哪份当前。
**修补位置**：第三阶段「ADR 模板规范」段。

### 规则 5：lite/full 主动切换提示
**坑**：用户没主动切 full → reviewer 不在岗 → 主窗口只能让用户审 ADR，恶性循环。
**修补位置**：第三阶段「关键设计决策」第 2 项「两档模式开关」。

### 规则 6：assumptions.md 先行
**坑**：用户在 backlog v0.2 之后才陆续修正"智阳是 APP 公司不是 ODM"/"V536 是全志不是联咏"/"12 颗芯片清单"等事实假设，每次都触发产物全面返工。
**修补位置**：第二阶段末尾。

### 规则 7：关键事实独立验证
**坑**：架构师 ADR-001 v1 把父类命名 `V536CdrBaseAdapter`，因为协议联络员一轮把 V536 当作联咏；架构师没独立验证就写了，三轮调研才发现 V536 实际是全志的 SoC，命名错误。
**修补位置**：第三阶段「主窗口派单规范」段。

### 规则 8：派单 prompt ≤ 1500 字
**坑**：主窗口每次派单 prompt 2000+ 字，大量复述背景，浪费 agent token + agent 难抓重点。
**修补位置**：第三阶段「主窗口派单规范」段。

### 规则 9：PLAN.md 更新原则
**坑**：PLAN.md 一会话被改十几次，每次微改噪音大；多个 subagent 越权改 PLAN.md（虽然内容质量高但违反硬规则）。
**修补位置**：第三阶段「关键设计决策」第 5 项「状态机驱动协作」。

### 规则 10：agent 产物 depends_on_artifacts 字段
**坑**：协议联络员一二三四轮，每次都重读所有上游已交付文档，效率低。
**修补位置**：第三阶段「agent 产物的 depends_on_artifacts 字段」段。

### 规则 11：过度工程自查
**坑**：架构师 ADR-001 v2 写 746 行约 30% 过度设计（开放问题段、稳定性分级等可精简）。
**修补位置**：第三阶段「Agent 设计规范」段五段式扩展为七段式（新增第 6 段）。

### 规则 12：subagent owner 边界自查
**坑**：协议联络员、架构师在交付时直接改 PLAN.md（虽然内容质量高，主窗口接受了，但违反硬规则）。
**修补位置**：第三阶段「Agent 设计规范」段五段式扩展为七段式（新增第 7 段）。
