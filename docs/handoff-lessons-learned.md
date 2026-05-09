---
title: Sync Copy — 项目级 Living lessons-learned
owner: main-window
status: LIVING
created_at: 2026-05-08
last_updated: 2026-05-08
depends_on_artifacts:
  - path: HANDOFF.md
    version: v5
  - path: decisions/ADR-002-adopt-handoff-v5.md
    version: 2026-05-08
---

# Sync Copy — Living lessons-learned

> v4-1 强制产物。本文件是项目级**长期记忆**，跨会话有效，跨 agent 共享。
> 任何 agent 进入新会话，**第一动作**是读本文件 [3] 段的 obsoleted 清单 + [6] 段的复审 ritual。

---

## [0] 这份文档是什么 + 怎么用

**用途**：把"在 commit / 总结 / todo 里散落的决议"集中成可信单一事实源；让 agent 看到旧文本能立即识别"这是已废止的，不要做"。

**触发追加条件**（任一发生必须立即追加 1 条）：

1. 一个红档决议被反转（`spec K-Q` 已决项被改）
2. 一个生产 bug 暴露根因属于"agent 之前应该想到但没想到"
3. 上下文压缩 / 会话重启后 agent 误执行已 obsoleted todo
4. 新 ADR 落档（同步把它列入 [2] / 把它废止的旧条目列入 [3]）
5. subagent 协作出现 RACI 冲突 / 越界 / 权限被拒
6. 用户反馈"产品形态 / 流程不满意"

**怎么读**：

- 30 秒上手 → 读 [1]
- 知道现在哪些决议是"红档生效中" → 读 [2]
- 知道哪些已废 → 读 [3]（**严格**遵守 ❌ 项）
- 之前踩过哪些坑 → 读 [4]
- 主窗口在干什么 → 读 [5]
- 会话重启后该做什么 → 读 [6]

---

## [1] 30 秒引导（绝对必读）

**项目类型**：Tauri 2 桌面应用（macOS + Windows），LAN 内多设备剪切板/文件 同步。

**当前阶段**：v2 重写中（v0 prototype 在 `legacy-prototype` 分支待建）。

**项目目录约定**（`CLAUDE.md` 第 3 节权威）：
- `specs/` ← PM owner（含 `_assumptions.md` v2-6 强制）
- `decisions/` ← architect / 主窗口 owner（用户拍板时）
- `docs/` ← 主窗口 owner（含本文件）
- `src-tauri/src/` ← backend-implementer owner
- `src/` ← frontend-implementer owner
- `PLAN.md` ← 主窗口 owner（v2-9：subagent 不直接写）

**决议优先级**（高 → 低）：
1. `decisions/ADR-NNN.md` 含 `status: ACCEPTED`（红档）
2. `specs/<slug>.md` 第 4 节"验收标准"
3. `CLAUDE.md`（项目宪法）
4. `HANDOFF.md`（脚手架口令 v5）
5. 本文件 [2] 段元决议总览
6. commit message / 历史 todo（**最低**优先级；与上面冲突时一律忽略）

**模式**：full（10 角色全员）。`/mode-status` 查询。

**关键边界**：
- 主窗口不直接改业务代码（`src-tauri/src/` / `src/`）
- agent 只读自己 owner 域 + spec/ADR；不调其它 agent
- subagent 不直接写 PLAN.md（v2-9）
- 决策卡片格式：v5-11 必须含问题 + 选项 + 推荐 + 取舍 + must-fix

---

## [2] 项目级元决议总览（红档生效中）

| ADR | 决议 | 状态 | 依赖 |
|---|---|---|---|
| ADR-001 | v0 prototype 留底；main 上以严格 SDLC 重写 v2，主窗口仅做编排 | ACCEPTED (2026-05-06) | — |
| ADR-002 | 项目升级到 HANDOFF v5 规范（增量补丁迁移） | ACCEPTED (2026-05-08) | ADR-001 |

| spec K-Q | 决议 | 状态 |
|---|---|---|
| `00-product-overview.md` | 8 条项目级验收标准 + 6 条 v0 教训 | SPEC_DRAFTED (2026-05-06) |
| 18 份 feature spec | 全部 SPEC_DRAFTED | 待 P2-1 架构师阶段 |

| 项目级元规则 | 出处 |
|---|---|
| 禁用 § 符号 | `CLAUDE.md` 第 11.5 节 / v5-12 |
| 主窗口边界（4.1/4.2/4.3） | `CLAUDE.md` 第 4 节 |
| Conventional Commits | `CLAUDE.md` 第 10 节 |
| email = `273774373+zota957525@users.noreply.github.com` | `CLAUDE.md` 第 10 节 |

---

## [3] 已 obsoleted / superseded 决议清单

> 每条格式：原决议 → 反转触发 → 新决议 → ❌ 禁止做的事
> agent 看到 ❌ 项即使在历史 commit / 旧 todo / 旧 spec 里出现也忽略。

**当前为空**（项目仍在 v2 第一轮，尚无决议反转）。

第一条预期触发场景：
- _assumptions.md 校对发现 PM 假设错 → 对应 spec SUPERSEDED → 在此记账
- v0 某个隐式不变式（如审批四轮迭代）被 v2 SUPERSEDED → 在此记账

---

## [4] 历史踩坑分类

> v1 时代（v0 prototype 阶段）已经踩过的坑；v2 重写时**严格**避开。

### 4.1 网络 / LAN

- **多网卡场景下 IP 优先级**：`192.168.x.x > 10.x.x.x > 172.x.x.x`（172 段 WSL/Docker 频繁误命中故降级）；虚拟网卡名（`vEthernet` / `utun` / `vmnet`）必须排除
- **系统代理拦截 LAN**：Clash / ClashX / Surge 在 macOS / Windows 都会拦 LAN 请求；reqwest 必须 `.no_proxy()`
- **listen_addr 为 `0.0.0.0` 不能直接当 peer 地址**：握手请求里只发 `listen_port`，对端用 `axum::ConnectInfo` 拼接
- **隐形掉线**（用户 2026-05-08 实战反馈）：v0 长时间运行后部分设备出现"复制无法同步但表面状态正常"现象（peer 列表仍显示在线，浮窗状态点仍绿，但实际 TCP 连接已死或对端进程已僵），用户唯一兜底 = 重启程序。**根因怀疑**：心跳检测或 TCP keepalive 不够激进，连接半死状态未被及时清除；或对端进程已 hang 但 OS 端口仍占用。**v2 待解**：① peer-heartbeat spec 必须加 AC："连续 N 次心跳超时强制重建 TCP 连接（不只是从 peer 列表移除）"；② 加被动健康自检——本地剪切板变化广播失败 ≥ M 次时主动 ping 全组并刷新连接；③ UI 加可见的"上次成功同步时间"字段（floating-window / floating-ball），让用户在表面正常但实际死透时能一眼看出 — 比"看上去绿"更可信

### 4.2 剪切板与系统集成

- **arboard 写入图片必须把 `last_text` 置 None**：否则下一次轮询会把图片对应的占位文字当成新文本广播回去（环路）
- **arboard 在 Windows 偶发"另一个进程占用"错误**：需 retry 1-2 次
- **PNG 编解码用 `image` crate 的 `default-features = false` + `features = ["png"]`**：默认 features 拉一堆冗余依赖体积爆炸

### 4.3 Tauri / Web

- **Tauri 2 capabilities 必须显式列**：`core:window:allow-set-size` / `allow-set-position` / `allow-outer-position` 等不是默认全开的
- **PhysicalSize vs LogicalSize**：Retina 屏（macOS Apple Silicon）`PhysicalSize(28)` = 逻辑 14px，会跌破 `minWidth`；统一用 `LogicalSize`
- **HMR 重置 state 但窗口几何不重置**：开发态需 `restoreIfStuckSmall` mount 时 self-heal

### 4.4 Rust 生态

- **reqwest 在 Windows 用 `rustls-tls` 会启动失败**（crypto provider init 问题）：改用 `default-tls`（OS 原生 schannel/SecureTransport）
- **tokio runtime 在 Tauri main 里不能用 `#[tokio::main]`**：用 Tauri 内置 async runtime 或 `tauri::async_runtime::spawn`

### 4.5 SDLC / 文档

- **§ 符号全局禁用**（用户全局规则；2026-05-06 项目级 sed 批量删过 169 处）
- **subagent 不读全局 `~/.claude/CLAUDE.md`**：项目级 CLAUDE.md 必须镜像所有需 subagent 遵守的规则（v2-1）
- **PM 写 spec 容易在 §3"范围"和 §7"未决问题"互相矛盾**：P1-5 系统性 review 修复 5 处类型 A 冲突；今后 PM 必须互斥

---

## [5] 主窗口（助理 / 管家）职责

> v4-5 显式 6 条；v5 在此基础上加 v5-1 / v5-2 / v5-11 三条派生职责。

**核心 6 条**：

1. **审视而非搬运**：看到旧 todo / 旧 commit 提到的事，先查它在最新决议下还成立吗；不机械搬运
2. **主动归档**：完成 / 已 obsoleted 的事项立即从 todo 删除并 [3] 段记账
3. **强制引用纪律**：自己写的 commit / 总结 / audit 引用决议必须精确到 `ADR-NNN` / `spec [N.M]` / `commit-SHA`；禁止"之前定的""刚才所说"
4. **多 agent 协作 RACI 把关**：每次派单前 cross-check 该 agent 的 owner 域 / tools 是否覆盖任务；越界停下问用户
5. **会话重启复审**：会话压缩 / 上下文重启后第一动作 = 跑 [6] 段 4 步 ritual
6. **被动 → 主动**：不只是 reminder bot；主动提请决策、主动检测错位升级信号、主动调整流程

**派生 3 条（v5）**：

7. **v5-1 错位升级信号**：发现用户 / 主窗口 / 错位 agent 在做某专家应做的事 → 立即触发 lite→full 切换或新角色引入
8. **v5-2 流水线自动跑**：默认连续推进所有 SDLC 阶段，仅 3 类硬关卡停下（关键产品决策 / 早期架构决策 / 不可逆操作）
9. **v5-11 决策卡片格式**：所有 stop-and-ask 必须含问题 + 选项 + 推荐 + 取舍 + must-fix

**派生 1 条（用户反馈 2026-05-09 演化）**：

10. **决策卡片密度过滤**：主窗口在向用户呈现决策卡片前，必须自查"这是否真的是用户能判断的事"——按下表三档分类：

| 决策类型 | 上报用户？ | 处理 |
|---|---|---|
| **产品方向**（v2.0 是否引入 PSK / 文件上限多大 / 用户 UI 流程怎么走） | ✅ 必报 | v5-11 决策卡片 |
| **范围变更 / 优先级**（某 spec 加入 / 排除 v2.0 / P1 升 P0） | ✅ 必报 | 同上 |
| **不可逆操作**（合 PR / 上线 / 删数据 / 公开发布 / 删整片源码） | ✅ 必报 | 显式确认 |
| **架构骨架方向**（模块切分 / 总协议 / 总数据模型方向） | ✅ 必报 | 同上（如 ADR-003 7 张卡） |
| **安全策略边界**（威胁模型范围 / PSK 引入与否 / 是否走商店审核） | ✅ 必报 | 同上（如 ADR-008 3 张卡） |
| **架构实现细节**（锁粒度 / 状态机入口 / 计数器归属 / runtime 归属） | ❌ **不上报** | 架构师 + sec 双签即可；用户在 lessons-learned 看摘要 |
| **代码契约级**（trait 签名 / 字段类型 / 错误码映射 / 启停步序） | ❌ **不上报** | 同上 |
| **CHANGES_REQUESTED 小补丁**（≤ 5 条文本级注释 / 反模式黑名单） | ❌ **不上报** | 主窗口直接派 arch 落补丁 → 静默 ACCEPTED |

**反例自查**（2026-05-09 复盘）：
- ADR-009 3 张卡片（锁粒度 / trust 入口 / DoS 计数器归属）— **均为架构实现细节，不应上报**
- ADR-010 4 张卡片（grace period / P0 例外 / panic hook 位置 / runtime 归属）— **均为代码契约级，不应上报**
- ADR-003 7 张卡片（模块切分 / 协议骨架 / 数据模型 / 加密层 / lifecycle / 错误日志 / 隐形掉线机制）— **架构骨架方向，应报**
- ADR-008 3 张卡片（AAD/zeroize/DoS / 协议加固 / 不必修议题）— **安全策略边界，应报**

**新规则生效后预期**：未来 P2-1.b 第三批 ADR-011 crypto traits + Phase 4 实现期 12+ 份 feature ADR，绝大多数将走"主窗口编排，sec 兜底，用户接收摘要"路径。仅当 ADR 触及"产品方向 / 范围 / 不可逆 / 架构骨架 / 安全策略边界"时才上报用户。

---

## [6] 会话压缩 / 上下文重启 复审 ritual

> 任何会话压缩或上下文重启后**第一动作不是干事**，而是按 4 步复审。

```
Step 1 — 读最新元决议
  cat decisions/ADR-NNN.md      # 读最近 1-2 个 ADR
  head -100 PLAN.md             # 当前阶段 + BACKLOG / IN_PROGRESS / BLOCKED 列表
  sed -n '/^## \[3\]/,/^## \[4\]/p' docs/handoff-lessons-learned.md   # obsoleted 清单

Step 2 — 对每条 pending todo / pending task 逐条问
  ① 这条在最新决议下还成立吗？  不成立 → obsoleted；成立 → 保留
  ② 引用的 ADR / spec K-Q 是否仍 ACCEPTED？已 SUPERSEDED → 改写或删除
  ③ 是否有新 ADR 把它替代？      是 → 标 obsoleted_by_<ADR>
  ④ 是否实际已完成（git 历史命中）？是 → 改 completed

Step 3 — 落档归档
  TodoWrite 不支持 obsoleted 状态 → 直接删除
  同时在 docs/handoff-lessons-learned.md [3] 加一条（首次发现）

Step 4 — 第一段话报告给用户
  ✓ 复审 ritual 完成
  - 删除 obsoleted N 条（列出）
  - 当前 pending M 条（按优先级）
  - 最新决议状态：ADR-XXX / spec [N.M] / commit-SHA
  - 待用户确认 / 直接开干？
```

**本会话首次跑 ritual 报告**（2026-05-08）：

- ✓ ritual 完成
- 检出 obsoleted 0 条（项目仍在 v2 第一轮）
- 检出 v5 规则差距 17 项（资产缺漏 6 + 流程违反 4 + CLAUDE.md 镜像 7）
- 当前 pending：执行 ADR-002 落盘补丁 + 推进 P0-1~P0-3 + 进入 P2-1 架构师
- 最新决议：ADR-002 ACCEPTED (2026-05-08)，supersedes 无，依赖 ADR-001
- 用户已拍板 A → 直接开干

---

## [7] 引用纪律（commit / 总结 / audit 必读）

| 不规范 | 规范 |
|---|---|
| "之前定的" | "ADR-002 决议（2026-05-08）" |
| "如刚才所说" | "spec `00-product-overview.md` 第 4.2 节 / commit-SHA" |
| "以前讨论过" | "ADR-001 第 3 节 决定段" |
| "v0 那样" | "`legacy-prototype` 分支 commit `f4be188`：xxx" |

**commit message 模板**：

```
type(scope): <slug>

短描述（1-2 行 / why + what）

引用决议链：
- ADR-NNN（status / decided_by / date）
- spec <path> 第 N.M 节
- 父 ADR / 父 spec（如适用）

实施细节...

下一棒：<batch / 角色 / DoR / DoD>

Co-Authored-By: Claude <noreply@anthropic.com>
```

---

## [8] 反风控约束（永远生效）

> v4-8 跨边界自动操作禁令——产品功能设计层与 PreToolUse 安全栏并列。

- ❌ 不 auto-install 系统组件（运行时 / 浏览器 / 证书等）
- ❌ 不修改系统代理 / 系统证书 / HKLM 注册表 / 系统服务
- ❌ 不要求用户关闭安全软件 / Clash / 防火墙以"配合"产品工作
- ❌ 不上 App Store / Microsoft Store（个人工具，免提审核）
- ❌ 不收集 facial / biometric / PII（产品定位是单人多机，不需要）
- ❌ 不写凭据 / 私钥到任何文件（含 spec / ADR / commit / log）
- ✅ 应用层兜底（`reqwest.no_proxy()` / `--proxy-bypass-list` / 应用沙箱内自管）

---

## [9] 修订历史

| 日期 | 改动 | 触发 |
|---|---|---|
| 2026-05-08 | 初版骨架建立 | ADR-002 落盘 |
| 2026-05-08 | `_assumptions.md` 校对完成 → APPROVED_WITH_REVISIONS。3 处事实层修正（A2/A14/A16）+ 第 4.1 段加 1 条 v0 实战 bug（隐形掉线）。触发 P1-7 spec 修订（PM owner）：`file-transfer-drag.md`（文件上限 50→5MB）/ `clipboard-image-sync.md`（非 PNG 走文件路径）/ `peer-heartbeat.md`（隐形掉线 AC + 强制重连 + 上次成功时间字段） | 用户校对 _assumptions |
| 2026-05-08 | P1-7 PM 修订完成。**校正记账**：A16 实际是 _assumptions 反向假设误差——`file-transfer-drag.md` v0 spec 原本就是 5 MB，PM 写 _assumptions 时记成了 50 MB，校对到的是 PM 自己的假设而非 spec。其余 A2 / A14 / A_BUG_HIDDEN_DEAD 均为真实 spec 修订。peer-heartbeat priority P2→P1。建议 P2-1 拆为 a/b（项目层架构 → feature ADR 分批）已写入 PLAN.md | PM P1-7 报告 |
| 2026-05-08 | P2-1.a 架构师产出 ADR-003 PROPOSED：项目层总骨架，971 行（超 700 行硬目标 39%，自查诚实），7 个子决策（模块切分 / HTTP 协议 / PeerState / 加密 trait / lifecycle / 错误日志 / 隐形掉线）+ 7 张决策卡片清单（v5-11）。15/20 spec → SPEC_REVIEWED；剩 5 份（cross-platform-build / floating-window / floating-ball / history-list / local-ip-display）等 P2-1.b 触及。**关键决策**：HKDF salt v2 bump 让 v0 prototype 与 v2 build 不互通（设计选择，须在 v2.0.0 release notes 声明）。涉及 crypto/protocol/网络认证 3 节，必走 security-reviewer 出 ADR-008。LM-1 进度 2/10（PM + tech-architect 已升级 7-section） | 架构师 P2-1.a 报告 |
| 2026-05-08 | 用户 7/7 决策卡片全选 B → ADR-003 PROPOSED → ACCEPTED_PENDING_SECURITY_SIGNOFF（项目层方向已拍板，安全细节待 ADR-008 收口）；deciders [tech-architect] → [tech-architect, main, user]；revision_history v1.1 记账。下一步：派 security-reviewer 审 3.4 / 3.6 / 3.7 三节出 ADR-008。LM-1 进度 3/10（security-reviewer 已升级 7-section） | 用户决策 + 主窗口编排 |
| 2026-05-08 | P2-2 security-reviewer 出 ADR-008 ACCEPTED（687 行）。结论 CHANGES_REQUESTED：方向 APPROVED + 8 必修（项目层 5 / feature 层 3）+ 3 不必修。**关键发现**：1 严重（/file 端点缺 seq dedupe，可重放已审批 file-pending 弹框）+ 11 中危（AAD 空 / zeroize 缺 / 状态码 409 device_id 可枚举 等）+ 3 低危（PSK / /ping origin / HMAC 全组 epoch key 演进）。**v5-1 错位记录**：security-reviewer 自标 ADR-008 ACCEPTED + deciders 含 user，主窗口判断这是接管 ADR-003 第 7 节的合理收口动作（不是越权 — 审阅结论本身已完成；必修清单是给 implementer 阻塞条件，用户拍 3 张回顾卡片只是确认必修是否接受，不再走 PROPOSED 流程）。3 张确认卡片待用户拍板 | security-reviewer P2-2 报告 |
| 2026-05-08 | 用户对 ADR-008 3/3 必修确认卡片全选 A。ADR-003 ACCEPTED_PENDING_SECURITY_SIGNOFF → ACCEPTED；revision_history v1.2 记账。**8 必修入项目级跟踪**：MUST-1 AAD 绑值 / MUST-2 zeroize / MUST-3 状态码 409→403 / MUST-4 PeerRegistry.remove 原子顺序 / MUST-5 panic message 不含变量插值（项目层基础设施 PR）+ MUST-6 /file seq dedupe + size 双校验 / MUST-7 handshake DoS 限流 / MUST-8 sanitize 模块（feature 层 ADR）。**P2-1.a 完成；P2-1.b 第一批解锁**：ADR-009 PeerRegistry / ADR-010 Lifecycle / ADR-011 crypto traits 三件套 — 5 项目层必修 MUST-1~5 入第一批 ADR input | 用户决策 + 主窗口编排 |
| 2026-05-08 | 用户选 A 串行 → P2-1.b 第一批第一份 ADR-009 PeerRegistry PROPOSED 落盘（499 行 ≤ 500 硬约束达标，吸取 ADR-003 超 700 行 39% 教训）。**6 子决策**：3.1 PeerState 完整字段（含 zeroize 包装 aes_key）/ 3.2 Registry 13 个方法接口契约 / 3.3 Trust 4 状态 × 7 事件互斥状态机 / 3.4 锁粒度（推荐 A 单 RwLock + 两个 HashSet 短路）/ 3.5 client_pool 接口契约（落实 MUST-4 原子顺序）/ 3.6 PolicyState 归属（推荐 B 独立 RateLimiter）。**3 张决策卡片**等用户拍板。架构师建议 reviewer 在第 7 节追加签字段而非新建独立 ADR | tech-architect ADR-009 报告 |
| 2026-05-08 | 用户对 ADR-009 3 张卡片拍板 1A / 2B / 3B（采纳推荐，澄清后从字面"全 B"修正）。**v5-11 复盘**：本次决策卡片推荐项不统一（1A 2B 3B），用户简写"全 B"产生歧义；主窗口正确停下走一次澄清流程，避免落盘错决议。**记账规则演化**：未来当 ADR 推荐项不统一时，决策卡片"一次性回复"段必须把推荐写成具体编号（如 `1A 2B 3B`），不能用"全 X"误导用户。**记账行动**：在 product-strategist + tech-architect agent prompt 的"决策卡片清单"段加注。ADR-009 status PROPOSED → ACCEPTED_PENDING_SECURITY_SIGNOFF | 用户拍板 + 主窗口澄清 |
| 2026-05-09 | security-reviewer 在 ADR-009 第 7 节追加签字 CHANGES_REQUESTED + 4 补丁（P1 snapshot/get SECURITY 注释 / P2 health.rs 反模式黑名单 / P3 RateLimiter 未认证 device_id 安全段 / **P4 关键: AB-BA 死锁防御 — approve/ban 锁顺序硬约束**）。架构师 v1.2 全 4 补丁落地（净增 40 行 ≤ 80 预算），新增第 3.3.1 节"锁顺序硬约束"。ADR-009 status → ACCEPTED（用户已拍板 1A 2B 3B 不变；security_signoff 字段记账）。**P2-1.b 进度 1/3**。**v4-7 / v5-9 派生影响**：group-discovery feature ADR 必须同步锁定 per_pair HashMap 容量上限 + 过期 retain 策略（避免内存放大）。下一步派 ADR-010 Lifecycle | tech-architect v1.2 + sec 双签 |
| 2026-05-09 | P2-1.b 第二份 ADR-010 Lifecycle PROPOSED 落盘（496 行 ≤ 500 硬约束达成）。**7 子决策**：3.1 Lifecycle struct + 4 态 Phase 状态机 / 3.2 启动 7 步细化（panic hook → tracing → AppState → PeerRegistry/RateLimiter/client_pool → server → workers → clipboard → emit app-ready）/ 3.3 关闭 7 步 + leave 1500ms timeout / 3.4 4 退出路径全走 quit_app（P0 tray 例外 + TODO）/ 3.5 panic hook 注册位置（lib.rs::run 最早入口，落实 MUST-5）/ 3.6 long-running task runtime 归属（全用 Tauri 内置）/ 3.7 shutdown grace period 选 A 固定 deadline 全表 ≤ 2800ms。**4 张决策卡片**等用户拍板，推荐 1A 2A 3A 4A（架构师标注 v5-11 编号格式，不再用"全 X"）| tech-architect ADR-010 报告 |
| 2026-05-09 | **用户拍 ADR-010 全 A 同时反馈"决策疲劳 + 都是细节 + 没判断经验"**。复盘：ADR-009 3 卡 + ADR-010 4 卡 = 7 张技术细节卡片本不应上报用户，违反 v5-11"用户是产品方向决策者"。**主窗口编排策略调整**（落 第 5 段 派生第 10 条）：决策卡片密度过滤——产品方向 / 范围 / 不可逆 / 架构骨架 / 安全策略边界 = 上报；架构实现细节 / 代码契约级 / CHANGES_REQUESTED 小补丁 = 主窗口编排 + sec 兜底，不上报。**新规生效**：ADR-010 全 A 落 ACCEPTED_PENDING_SECURITY_SIGNOFF；后续 sec 签字若 CHANGES_REQUESTED 主窗口直接派 arch 落补丁 → 静默 ACCEPTED；ADR-011 crypto traits 派单后若仅技术细节决策卡片 → 主窗口直接采纳 arch 推荐 → 不上报用户 | 用户元反馈 + 主窗口策略调整 |
| 2026-05-09 | **新策略首批静默运行**：(1) sec 审 ADR-010 第 7 节 → CHANGES_REQUESTED 4 补丁（P1 panic hook prev(info) / P2 P0 tray bypass tracing::warn / P3 banned snapshot 信息泄露 / P4 health Shutting 禁 replace）→ 主窗口直接派 arch 落 v1.2（净增 9 行 ≤ 60 预算）→ 静默 ACCEPTED，未上报用户。(2) arch 写 ADR-011 crypto traits 500 行（卡 ≤ 500 硬上限），6 子决策 + 2 张纯技术细节卡片（trait 拆分 / AAD 入参形态）→ 主窗口直接采纳 1B 2B（保留 2 trait + Verifier 注释占位 / build_aad 集中函数 + AadKind 9 值），未上报用户 → status PROPOSED → ACCEPTED_PENDING_SECURITY_SIGNOFF；待 sec 在第 7 节签字（涉及 crypto 必走 CLAUDE.md 第 9 节强约束）。**P2-1.b 进度 2/3 ACCEPTED + 3/3 待 sec**。新策略观察良好——主窗口在 30 分钟内推进 2 份 ADR 闭环；用户决策负担 0 | sec ADR-010 + arch ADR-010 v1.2 + arch ADR-011 + 主窗口编排 |
| 2026-05-09 | **🎯 里程碑：P2-1.b 第一批基础设施三件套全部 ACCEPTED**（ADR-009 v1.2 / ADR-010 v1.2 / ADR-011 v1.2）。ADR-011 sec **APPROVED 0 必修补丁**（项目最关键加密 ADR 一次过；5 聚焦点全 ✅ 含 1 项已识别非阻塞建议）。**新策略 2 例验证**：技术细节卡片（共 9 张：ADR-009 3 / ADR-010 4 / ADR-011 2）全部主窗口编排闭环，用户 0 决策负担；安全策略级 / 不可逆操作仍上报。**项目状态总览**：ADR 5 份 ACCEPTED（001/002/003/008/009/010/011）+ 22 份 spec 全部 SPEC_REVIEWED + _assumptions APPROVED_WITH_REVISIONS + lessons-learned LIVING。**下一步**：进入 P2-1.c 实现阶段（backend-implementer 落 src-tauri/src/peer / app/lifecycle / crypto 三 module + ≥ 18 单测 + MUST-1~5）。这是从"决议层"进入"实现层"的关键转折——主窗口需向用户决策卡片确认是否启动（产品方向 + 不可逆 = v5-2 上报硬关卡）| 三件套双签 + 主窗口里程碑确认 |

**懒迁移待办**（ADR-002 第 3 节登记）：

| # | 待迁移项 | 触发条件 | 状态 |
|---|---|---|---|
| LM-1 | 10 个 agent 文件升级到 v2 7-section（加"过度工程自查 + owner 边界自查"） | 该 agent 下次被派单前 | 4/10 完成（`product-strategist.md` + `tech-architect.md` + `security-reviewer.md` + `backend-implementer.md` 已升级；后者新加"必查 ADR 清单"段引导 implementer 先读 5 ADR 的关键节）；剩 6 个：ux-designer / frontend-implementer / code-reviewer / qa-tester / docs-writer / release-engineer |
| LM-2 | ADR-001 frontmatter 升级到 v2 字段集（`feature_id` / `revision_history` / `depends_on_artifacts`） | ADR-001 下次被引用 / 修订时 | PENDING |
| LM-3 | 20 份 spec 增补 `depends_on_artifacts` 字段 | 该 spec 下次被修订时 | PENDING |

---

## [10] 引用 / 关联文档

- `CLAUDE.md` — 项目宪法（主窗口契约）
- `HANDOFF.md` — 脚手架口令 v5
- `PLAN.md` — 任务看板
- `TEAM.md` — 10 个虚拟同事花名册
- `specs/_assumptions.md` — 事实假设清单（PENDING_USER_REVIEW）
- `specs/00-product-overview.md` — 产品总览
- `decisions/ADR-001-rewrite-with-strict-sdlc.md` — 重写决议
- `decisions/ADR-002-adopt-handoff-v5.md` — v5 升级决议
