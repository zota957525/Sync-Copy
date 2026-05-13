---
status: SPEC_REVIEWED
owner: product-strategist
related_adrs: [ADR-001, ADR-003]
created: 2026-05-06
updated: 2026-05-08
revised: 2026-05-08 — ADR-003 项目层骨架 covers 第 5.4 节 待挑战项的项目层部分（模块切分 / AppState / 协议骨架）
---

# 00 — Sync Copy 产品总览（v2 重写起点）

## 1. 问题（为什么做整个产品）

一个人拥有多台电脑（家里 iMac + 公司 MacBook + 测试 Windows），每天频繁需要把"刚才在 A 上看到/截到/拷贝到的东西"挪到 B 上：复制一段命令、保存一张截图、传一份小文件。系统级的 iCloud 通用剪切板对苹果生态外的设备零支持；云剪切板服务（Snippet/Pastebin/聊天软件给自己发）有上传延迟、隐私顾虑、依赖外网、且要手动操作。用户要的是"两台机器像一台机器"——同一 LAN 下，复制即同步、截图即同步、拖拽即同步，不签账号、不付费、内容不出局域网。

## 2. 用户故事（对谁做）

- **多机用户（核心）**：As a developer with Mac + Windows on the same WiFi, I want copy on one to instantly appear on the other's clipboard, so that I can paste with the system shortcut without a relay app.
- **截图工作流**：As a content creator, I want screenshots taken on my Mac to be pasteable on my Windows in any app (Word / 微信 / Keynote), so that I do not need to save-then-airdrop-then-open.
- **小文件递送**：As a single user with 2-3 machines, I want to drag a sub-5MB PDF onto the floating window and have my other machines optionally save it to Downloads, so that I skip email-to-self / chat-to-self.
- **临时加机**：As an owner of an established 2-device group, I want to add a third device with a single approval click on any one existing device, so that onboarding is one tap not N taps.
- **隐私敏感**：As a user who does not want clipboard contents leaving my LAN, I want all traffic to stay local and be end-to-end encrypted, so that even a compromised router or a malicious LAN neighbor sees only ciphertext.

## 3. 范围

### in scope（v2.0.0 计划交付）

**P0 — MVP 核心闭环（不交付即不能称作 v2）**
- `cross-platform-build` — Mac universal + Win x64 CI 构建与发布
- `floating-window` — 320×420 透明置顶浮窗主界面
- `tray-integration` — 系统托盘显隐切换
- `group-discovery` — 通过 IP:PORT 加入小组
- `group-approval` — 分布式审批（任一在线设备同意即生效）
- `e2e-encryption` — X25519 ECDH + HKDF + AES-256-GCM 端到端加密
- `clipboard-text-sync` — 文本剪切板同步
- `local-ip-display` — 浮窗底部 IP:PORT 展示与点击复制

**P1 — 完整体验**
- `clipboard-image-sync` — PNG 图片剪切板同步
- `file-transfer-drag` — 拖文件到浮窗发送（≤5 MB）
- `history-list` — 浮窗历史列表（最多 50 条 / 单击复制 / 删除）
- `settings-panel` — 设置面板（设备名 / 清除历史 / 退出应用）
- `floating-ball` — 收缩为 48×48 悬浮球

**P2 — 多机协作韧性**
- `group-trust-gossip` — 信任/封禁传播（一人审批全组生效）
- `group-leave-notify` — 主动下线广播
- `peer-heartbeat` — 被动心跳掉线检测
- `history-sync-delete` — 跨机同步删除条目

### out of scope（v2 明确不做）

- mDNS / UDP 广播自动发现（仍需手动填 IP:PORT；自动发现留 v3 视用户反馈再评估）
- 跨互联网 / NAT 穿透 / 中继服务器（与"无中心服务器、LAN 限定"定位冲突）
- 跨 VLAN 或跨网段（路由器层不在掌控范围内）
- 大文件 / 文件夹传输（>5 MB 直接拒绝；分片传输与续传暂不做）
- 富文本/HTML/RTF 剪切板格式（仅纯文本 + PNG 图片 + 文件三类）
- 持久化信任名单（重启即重新审批，是安全设计选择不是缺陷）
- 移动端（iOS / Android / iPadOS）
- 命令行 / 无人值守模式（设计为带 GUI 的桌面工具，审批必须有人在场）
- Linux 一线支持（理论可跑，但 Wayland 图片剪切板不稳定，CI 不构建 Linux 产物）
- 账号体系 / 云同步 / 跨 LAN 漫游
- 应用自定义快捷键（用户用系统的 Cmd/Ctrl+C 即可）

## 4. 项目级验收标准（Definition of Done for v2.0.0）

> 这些是 v2 整体能否发布的"观测项"，不是某个 feature 的标准。

- [ ] **双平台分发**：CI 在每次 main 分支 push 后产出 `SyncCopy-v2.0.0-macOS-universal.dmg` 和 `SyncCopy-v2.0.0-windows-x64-setup.exe`，下载后双击即可安装运行
- [ ] **三机集成场景全过**：`tests/integration-checklist.md` 中的 Mac×2 + Win×1 三机互联测试单全部 pass（含加入、文本同步、图片同步、文件传输、删除同步、心跳掉线、主动 leave）
- [ ] **MVP 端到端**：在零配置环境下，两台同 LAN 设备能在 60 秒内完成"安装 → 启动 → 一次审批 → 双向文本剪切板互通"
- [ ] **加密路径有 ADR + security-reviewer 签字**：`crypto.rs` 与 `network/protocol.rs` 的所有改动经 security-reviewer 显式 ACK，且每条决定有对应 ADR
- [ ] **每份 spec 含验收标准 + 已知坑 + 未决问题**：v2 的 17 个候选 feature 各有 `specs/<slug>.md`，状态 ≥ APPROVED 才进入实现
- [ ] **SDLC 留痕**：每个 feature 在 `PLAN.md` 状态机走完 BACKLOG → SPEC_DRAFTED → ADR_ACCEPTED → IMPL_DONE → REVIEW_PASSED → TEST_PASSED → DOCS_DONE → RELEASED，无跳步
- [ ] **新人 30 分钟上手**：只读 `specs/` + `decisions/` + `PLAN.md` + 重写后的 `项目架构.md` 即能解释任意一处实现的"为什么"
- [ ] **回归对比 v0**：v0 中已经能跑通的所有用户场景（见 `使用说明.md`）在 v2 上至少同样能跑通；不允许"v2 把功能做小了"

## 5. v0 历史 / 已知坑（v2 必须避免）

### 5.1 v0 验证为对的产品决定

- **去密码化 + 人工审批认证**（M3→M4 演进）：密码体验差，审批弹框直觉、可控、且能承载分布式 trust gossip。**这条路径在实战中证明优于密码模型，v2 继承。**
- **端到端加密但不持久化密钥**：每次会话现协商，进程退出即丢，等价于一次性密钥；用户不需要"密钥管理"心智，且天然限制了被盗设备的攻击窗口。
- **审批是分布式的，"一票通过"**：任意一台已加入设备点同意即生效，其它设备弹框自动消失。这一设计避免了"主审 vs 从审"的角色复杂度，也比"全员一致同意"更人性化。
- **5 MB 文件硬上限 + Downloads 目录**：避免大文件内存暴涨、避免目录权限引战，简单且够用。
- **优先 192.168/16 LAN IP 选择策略**：实战发现 Clash fake-IP（198.18/15）和 WSL/Hyper-V 虚拟网卡是常见噪声，过滤策略经多次迭代收敛。
- **磨砂玻璃 + 透明 + 置顶 + 圆角的浮窗形态**：用户接受度高；浮窗形态适合"轻度伴随"工具的产品定位。

### 5.2 v0 暴露的根本问题（设计层教训，不是 bug）

1. **零文档化的隐式不变式**：例如 `clipboard.rs` 中"写入图片时必须把 `last_text` 置 None 否则死循环"、`forwarded_approvals` 与 `pending_approvals` 必须配对清理、`approved_device_ids` 与 `banned_device_ids` 是互斥覆盖关系——这些规则只活在作者头脑里，下次返工有 100% 概率被打破。**v2 必须在 spec/ADR 里点名每条不变式。**
2. **单文件膨胀**：`src/routes/+page.svelte` 1483 行（含 UI + 状态 + 业务逻辑 + CSS）、`src-tauri/src/network/server.rs` 784 行（11 个端点 handler 同文件）。**v2 必须在 spec 阶段就规划组件/模块拆分，禁止"先堆一个文件，以后再拆"。**
3. **架构演化无记录**：M3 用密码 → M4 早期想做 PBKDF2 → M4 最终用 X25519+ECDH，这种关键变更只在 commit message 里留只言片语，没有 ADR 论证否决路径。**v2 任何技术选型必须 ADR 记录至少 2 个被否决的选项。**
4. **UX 反复折腾**：边缘吸附做了又删（`34ace33` 加，`a09ef6c` 删）；浮球图标从 emoji 换 SVG 又微调；审批弹框单点 → 分布式 → 一票通过——这些迭代都是因为没有事前 UX spec 强制思考用户场景。**v2 在 implementer 写代码前必须有 ux-designer 的 UX 段落地。**
5. **测试覆盖率 0%**：所有验证靠"两台真机手测"。一旦协议演进，回归成本飙升。**v2 每个 feature 必须有 qa-tester 写的可执行 checklist 或单元测试。**
6. **审批流程的设计反复**：从"单点弹框"→"所有设备弹框 + 谁先按谁作数"→"还要广播 dismiss 关掉其它弹框"→"trust gossip 让以后免审"——4 次迭代才稳定。**v2 必须在 spec 阶段就把"分布式状态收敛"这件事说清楚。**

### 5.3 v0 的决定 v2 应该**继承**

- X25519 临时密钥对 + HKDF-SHA256 + AES-256-GCM 的密码学栈
- 每对 peer 独立一把密钥；密钥仅内存
- 5 MB 文件上限
- 内存态 `approved_device_ids` / `banned_device_ids`（重启清空 = 重置信任）
- 分布式审批 + 一票通过 + 自动 dismiss 其它弹框
- gossip mesh：握手响应里带 peers 列表，新成员自动连接所有已知成员
- "no_proxy + OS 原生 TLS" 网络客户端基线
- LAN IP 选择优先级（192.168 > 10 > 172.16-31）
- 心跳：10s 间隔，连续 2 次失败才剔除（容忍单次抖动）
- 离线主动广播 leave + 心跳被动兜底的双层机制
- Tauri 2 + Svelte 5 runes + adapter-static SPA 的技术栈

### 5.4 v0 的决定 v2 应该**挑战**

- **每秒轮询剪切板**：耗电、有延迟下限、与系统级 clipboard event API（macOS NSPasteboard `changeCount`、Windows `AddClipboardFormatListener`）相比是粗暴方案。v2 应评估是否切到事件驱动。
- **单 `+page.svelte` 1483 行**：v2 必须拆分组件树（FloatingWindow / FloatingBall / HistoryList / ApprovalDialog / SettingsPanel / JoinDialog 至少 6 个独立组件）。
- **单 `network/server.rs` 784 行**：v2 应按端点族拆模块（handshake / clipboard / file / approval / gossip / health 各一文件）。
- **`AppState` 上帝结构**：14 个字段聚合在一个 struct，锁粒度粗。v2 应评估按职责拆 ClipboardState / NetworkState / GroupState。
- **base64 over JSON 的密文传输**：膨胀 33%，对图片/文件不友好。v2 应评估二进制 body（multipart 或自定义二进制协议）。
- **HTTP body 8 MB 上限是 5 MB 文件 + 编码膨胀的产物**：换协议后这个数字应该重新推导而非沿用。
- **gossip mesh 在握手响应里同步整张 peers 表**：N 增长后会变成 O(N²) 的连接风暴。v2 应在 ADR 里论证 N 上限假设（用户多机一般 ≤ 5 台，仍然可接受？）。
- **接收文件先整体加载入内存再加密**：5 MB 不致命但思路不洁。v2 应评估流式加密。
- **没有 `seq` 回绕处理**：u64 几乎不会回绕，但仍是隐式假设；v2 应明确文档化或加 wrap 逻辑。
- **`device_id` 持久化但 trust 不持久化**：用户如果用 `git clone` 或镜像复制磁盘会撞 device_id 冲突（v0 返回 409）。v2 应评估是否在启动时检测复制场景。

## 6. UX 段（占位，等 ux-designer）

> 待 `ux-designer` 在 `specs/ux/00-overview.md` 或后续每个 per-feature spec 的 第 6 节 中补全。本 spec 此段不填。

## 7. 已知风险 / 未决问题

> 这些是 Phase 2（架构 + 安全决策）的弹药。每条标注问哪个角色。
>
> **优先级统计**：[P0 7 条] [P1 5 条] [P2 2 条]
>
> 优先级语义：[P0] 阻塞 v2 实现，必须 ADR 阶段答；[P1] 影响实现质量但不阻塞，可 ADR 中默认值 + 标注遗留；[P2] 可推迟到 v2.1+ 决策

- **[P1] [架构师]** 剪切板监听是否从"每秒轮询"切到 OS 事件驱动（macOS `changeCount` + Windows `AddClipboardFormatListener`）？跨平台抽象层成本与电池/响应度收益如何权衡？
- **[P0] [架构师]** v2 前端组件拆分粒度——最少 6 个独立组件还是更细？状态管理用 Svelte 5 runes 局部 state 还是抽出 stores？
- **[P0] [架构师]** `AppState` 是否拆分？拆成几块？锁的粒度（`RwLock` vs `Mutex` vs `parking_lot::RwLock` vs Actor 模型）如何选？
- **[P1] [架构师]** 网络层是否仍用 axum 0.8 + reqwest，还是切到二进制协议（如自定义 framed TCP）以避免 base64 膨胀？保留 HTTP 的好处是 curl 可调试，代价是 33% 流量膨胀——值不值？
- **[P0] [架构师]** gossip mesh 的 N 上限假设是多少？是否需要 ADR 明确"≤ 8 设备"的设计目标，超过即降级或拒绝？
- **[P0] [架构师]** `seq` 回绕、`last_seen_seq` 内存增长（每个 origin 永久占一行）、`forwarded_approvals` 清理时机这些隐式不变式是否每条都进 ADR 显式标注？
- **[P1] [UX]** 浮窗在 Mac vs Windows 的视觉差异（毛玻璃在 Win 上是否退化为半透明纯色？阴影/圆角原生支持？）需 mockup 对照。
- **[P1] [UX]** 审批弹框是覆盖在浮窗内（v0 做法）还是用系统级原生弹窗 / 通知中心？前者必须浮窗在前台才看得见，后者更醒目但脱离工具的"轻量"调性。
- **[P2] [UX]** 悬浮球在被遮挡或拖出屏幕外时的恢复策略（v0 用 `ensure_on_screen` 在显示时校正）是否需要更主动的"边缘提示"？
- **[P0] [安全]** 明文剪切板内容在内存中的生命周期：是否在分发完成后立即 `zeroize`？`arboard` 持有的 buffer 是否在我们控制范围内？
- **[P0] [安全]** 握手过程不加密（仅交换 X25519 公钥）是否仍接受？是否考虑预共享 PSK 来防主动 MITM（即使 LAN 通常被信任）？
- **[P2] [安全]** `device_id` 跨设备克隆（用户镜像磁盘或 `cp -r ~/Library/Application Support/com.synccopy.app/`）的检测与处理：v0 仅返回 409，v2 是否做更主动的提示？
- **[P1] [安全]** 文件 5 MB 上限是抗 DoS（防止单条 RAM 暴涨）还是仅 UX 限制？v2 应明确威胁模型并在协议层强制（拒绝接收超限请求 body）。
- **[P0] [安全]** trust gossip 的传染性风险：如果某成员被攻陷，它能 `/peers/trust` 任意 device_id 让全组接受陌生设备。是否需要"trust 必须本机也确认"的二次验证？

## 8. Review 段（占位）

> code-reviewer 与 tech-architect 在后续阶段填写。

---

## Code Review — PR-FE-0 Tauri Commands 层 / 2026-05-13 commit 837fd55

**结论**：CHANGES_REQUESTED

PR-FE-0 整体设计干净（11 命令 + DTO 边界明确 + AppHandle 真零 lifecycle 改动 + 140 单测全过 + clippy/fmt 0 warning），但有 **2 条必修** 触及 ADR-008 安全必修条款 + spec AC 缺口，必须在 PR-FE-1 接入前修。

### 5 聚焦点验证
- 命令参数 sanitize：❌（MUST-8 未在 set_config 调用 `sanitize_device_name`，仅做 trim+len 截断）
- 错误边界：❌（set_config / join_group / approve_peer 等命令 Err(String) 拼接 anyhow::Error::Display，泄露文件路径 / 内部错误链，违反 ADR-008 第 4.1 节"通用 body"原则）
- history/config store 设计：✅（VecDeque + RwLock + MAX_HISTORY=50 / ProjectDirs config.json / async tokio::fs::write 符合 spec settings-panel 第 3 节 + history-list 第 3 节）
- AppHandle 注入 + emit 时序：⚠️（注入 OK，emit 时序整体 OK — peers/history 操作均在 PeerRegistry/HistoryStore 写完之后再 emit；但 **peer-pending 事件 emit 接入未落地** — `PeerPendingPayload` struct 定义存在但 handshake handler 不发，spec group-approval AC #1 "A 与 B 同时弹审批框"链路在 PR-FE-0 边界外但需明确挂在 PR-FE-1 否则 group-approval 整条不通）
- recopy_history_item 占位：⚠️（text 路径 OK；image/file 返 Err — 前端会让用户困惑点击无反应，应在 PR-FE-1 前对齐 clipboard_apply_tx 扩展为枚举或先在前端禁用图片单击）

### 必修补丁（CHANGES_REQUESTED）

#### [严重] MUST-8 sanitize_device_name 未在 set_config 调用 — commands.rs:324-336
- 现象：spec settings-panel 第 4 节 AC "device_name > 64 字符时 input 截断或 banner 提示" 后端只做 `chars().take(64)`；ADR-008 MUST-8 必修"字符集白名单 + Bidi/控制字符黑名单"未应用
- 风险：恶意 device_name 含 U+202E RTL override 写盘 → 下次 broadcast 给对端 → 对端 UI 渲染时即使 handshake 端 sanitize 也无法救（本机 device_name 是本机自定的）
- 修法：`commands.rs::set_config` 在截断前调 `crate::peer::sanitize::sanitize_device_name(&trimmed)`；该函数已在 peer/sanitize.rs 实现且包含 Bidi+控制字符+64 codepoints 三件套

#### [严重] 错误 body 泄露内部 anyhow 链 — commands.rs:257 / 280 / 350 / 487
- 现象：`format!("入组目标格式不对，应该是 ip:port：{e}")` / `format!("连接 {normalized} 失败：{e}")` / `format!("配置保存失败：{e}")` / `format!("历史条目不存在：{id}")` 全部把 anyhow::Error::Display 拼到 Err(String) 返前端；其中 `Config::save` 失败的 Err 含完整 ProjectDirs path（"~/Library/Application Support/com.synccopy.app/config.json"）
- 风险：违反 ADR-008 第 4.1 节"403 通用 body""422 统一 body 串"原则；前端拿到的字符串泄露内部路径 / device_id 字面值 / network::client 错误链
- 修法：boundary 处统一返用户友好通用串（"配置保存失败" / "入组目标格式不对" / "历史条目不存在"），具体错误细节用 `tracing::warn!(error = %e, ...)` 写入日志即可；ADR-003 第 3.6 节 CommandError 设计正是为了避免这种"e.to_string() 直接出 boundary"

### 新发现问题（[低] nit 可挂下批扫尾）

- [低] `set_config` listen_port=0 拒绝（commands.rs:340）— `port: u16` 类型已保证 0..=65535，但 v2 P1 spec 第 3 节明确 "端口字段 P1 不开放修改"，UI 不会传 port，后端是否该直接 ignore listen_port 字段而非接受？建议改文档或加 `tracing::warn!("set_config port=... ignored at P1")`
- [低] `now_ms()` 公开导出但 commands.rs 内未使用（pub fn 给谁用？）— 若给 PR-FE-1 后续 handler 用应有调用点 doc 注释；否则改为 pub(crate)
- [低] `recopy_history_item` image 分支 `let _ = data_b64;` 显式丢弃但下行就直接 Err — 该写法触发 `unused_variables` 但 clippy 没报；可省略 let 绑定（HistoryPayload::Image { .. } 即可）
- [低] `format_last_sync` 用 `Instant::elapsed()` 不可序列化 — peer.last_successful_sync_at 在 snapshot 拷贝后才转 String，时序正确，但若 snapshot 与 commands 调用间隔较长 elapsed 会偏大；可接受（用户感知秒级误差无意义）

### 测试覆盖评估

- 现有 8 单测：覆盖 normalize_addr / approve_peer unknown / set_config 截断 / entry_to_item / relative_time / DTO 构造
- 未覆盖（建议 PR-FE-1 前补）：
  - set_config 含 Bidi 字符 → 出来必须不含 U+202E（修必修 1 后追加）
  - 错误 body 不含 anyhow 内部细节（修必修 2 后追加：set_config 写盘失败的 Err 不含 path）
  - HistoryStore 并发 push（多线程同时 push 同 hash → 不应 panic + 计数 = 1）
- spec history-list AC #11 "50 条全部为图片（每张 5 MB）时浮窗滚动流畅" — 属前端验收，PR-FE-1 落地后由 qa-tester 跑

### 结论

CHANGES_REQUESTED。2 条必修是 ADR-008 必修条款 + spec AC 安全防线，必须修；其余 [低] nit 挂 PR-FE-1 扫尾。按 lessons-learned 新策略：派 backend-impl 静默落两条补丁 → 静默通过（不需要走完整流程二次评审）。

**过度工程自查**：本 review 段约 0% 可省略 — 2 条必修是 spec/ADR 硬约束触发，[低] 4 条已折叠到 nit。8.5 todo 共 2 条主修 + 4 条 nit，控制在 8 条建议上限内。
