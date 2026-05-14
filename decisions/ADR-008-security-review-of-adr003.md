---
id: ADR-008
feature_id: project-skeleton-security-signoff
title: ADR-003 第 3.4 / 3.6 / 3.7 节安全审阅 — 加密层抽象 / 错误日志总策略 / 隐形掉线机制
status: ACCEPTED
owner: security-reviewer
date: 2026-05-08
deciders: [security-reviewer, main, user]
related_specs:
  - e2e-encryption
  - peer-heartbeat
  - diagnostic-logging
  - clipboard-text-sync
  - clipboard-image-sync
  - file-transfer-drag
  - group-discovery
  - group-approval
  - group-trust-gossip
  - history-sync-delete
  - settings-panel
  - _assumptions
related_adrs:
  - ADR-003
supersedes: []
superseded_by: []
revision_history:
  - version: v1
    date: 2026-05-08
    notes: 初版 — security-reviewer 对 ADR-003 第 3.4 / 3.6 / 3.7 节出独立审阅 ADR；接管 ADR-003 第 7 节 10 项待审议题；结论 CHANGES_REQUESTED（项目层方向 APPROVED，6 条必修在 implementer 落地前补齐）
depends_on_artifacts:
  - path: decisions/ADR-003-project-architecture-skeleton.md
    version: v1.1（ACCEPTED_PENDING_SECURITY_SIGNOFF / 2026-05-08）
  - path: specs/e2e-encryption.md
    version: 2026-05-08（SPEC_REVIEWED）
  - path: specs/peer-heartbeat.md
    version: v2 / 2026-05-08
  - path: specs/diagnostic-logging.md
    version: 2026-05-08（SPEC_REVIEWED）
  - path: specs/_assumptions.md
    version: 2026-05-08（APPROVED_WITH_REVISIONS）
  - path: docs/handoff-lessons-learned.md
    version: 2026-05-08
  - path: legacy-prototype:src-tauri/src/crypto.rs
    version: v0 / commit f4be188
  - path: legacy-prototype:src-tauri/src/network/protocol.rs
    version: v0 / commit f4be188
  - path: legacy-prototype:src-tauri/src/network/server.rs
    version: v0 / commit f4be188
---

# ADR-008 — ADR-003 第 3.4 / 3.6 / 3.7 节安全审阅

> 范围：本 ADR 是 ADR-003 第 7 节"占位"段的接管 ADR。ADR-003 自身已 ACCEPTED_PENDING_SECURITY_SIGNOFF；本 ADR 落盘后，ADR-003 第 7 节追加一行引用 `已由 ADR-008 接管，本节不再扩展`，ADR-003 状态由主窗口推进到 ACCEPTED。本 ADR 不替代 ADR-003 第 3.1 / 3.2 / 3.3 / 3.5 节决议；不替代任何 feature 层 ADR；feature ADR（P2-1.b）涉及 crypto / 协议 / 网络认证时必须显式引用本 ADR 第 7 节"必修清单"。

---

## 1. 上下文（Context）

### 1.1 触发本次审阅的输入

- **ADR-003**（项目层架构骨架，2026-05-08 用户 7/7 决策卡片全选 B），状态 `ACCEPTED_PENDING_SECURITY_SIGNOFF`。其中第 3.4 节（加密层抽象）/ 第 3.6 节（错误日志总策略）/ 第 3.7 节（隐形掉线机制）涉及密码学栈选型 / 协议错误信息边界 / 网络层信任决策；按 CLAUDE.md 第 9 节强约束须经 security-reviewer ACK
- **ADR-003 第 7 节 10 项待审议题**：AAD 绑值 / zeroize 引入 / PSK / content_hash → HMAC / filename sanitize 加固 / 文件 size 早期校验 / handshake DoS 限流 / device_name 字符集 / /ping origin 校验 / 日志 device_id / device_name / IP 记录边界
- **ADR-003 第 8 节 7 张决策卡片 user must-fix**：卡片 4（HKDF salt v2 bump + AAD 入参 trait 签名预留不锁值）/ 卡片 6（敏感字段黑名单）/ 卡片 7（last_successful_sync_at 仅在广播 200 OK 时写 / per-peer client_pool / PeerRegistry.remove 触发 client_pool.remove）三张直接落到加密 / 协议 / 网络
- **specs**：`e2e-encryption.md` 第 7 节（4 P0 安全 + 2 P1 架构师议题）/ `peer-heartbeat.md` 第 4 节 v2 新 3 条 AC + 第 7 节 [P1] 隐形掉线参数 + [P1] [安全] /ping origin 议题 / `diagnostic-logging.md` 第 7 节（敏感字段黑名单 / 导出 zip 免责 / panic hook 决议）/ `_assumptions.md` 第 4 节信任与安全 19-23 + A_BUG_HIDDEN_DEAD
- **v0 实现 read-only 对照**：`legacy-prototype` 分支 `crypto.rs`（75 行干净模块）/ `protocol.rs` DTO 11 个 / `server.rs` 784 行（含 12 个 handler 含 `sanitize_filename` / `unique_path` 已有但需加固）
- **CLAUDE.md 第 14 节** v5 规则镜像：v4-7 fatal 三件套 / v4-8 反风控约束 / v5-9 registry 完整性 cross-check（密钥生命周期与 PeerRegistry.remove 钩子一致性）/ v5-10 三向决议（spec K-Q + ADR + lifecycle 一致）

### 1.2 为何独立 ADR 而非追加 ADR-003 第 7 节

判定：**独立 ADR-008**。理由：

1. ADR-003 第 7 节列出的 10 项议题超出"签字段"体量；追加段会让 ADR-003 单文件继续膨胀（已 976 行）违反本项目对单文件长度的常识（虽 ADR 不是源码，但同样适用"拆分助理理解"原则）
2. P2-1.b 的 6 份 feature spec（e2e-encryption / clipboard-text-sync / clipboard-image-sync / file-transfer-drag / peer-heartbeat / diagnostic-logging）后续 feature ADR 会高频引用本 ADR 第 7 节"必修清单"；独立 ADR 让 `related_adrs: [ADR-008]` 比 `related_adrs: [ADR-003], section: 7` 干净
3. 安全决议未来可能独立演进（如 1 个月后引入 PSK / 切 Noise Protocol），独立 ADR 的 supersede 路径比"追加段反复改 ADR-003"清晰
4. 用户在主窗口 prompt 中已建议独立 ADR-008（"项目层加密决策值得独立留档"）；security-reviewer 同意此判断

### 1.3 本审阅不在场（边界明确）

- **不审 ADR-003 第 3.1 / 3.2 / 3.3 / 3.5 节**：模块切分 / HTTP 协议骨架 / PeerState 数据模型 / lifecycle owner 不直接涉及加密原语 / 网络认证；其中 3.2 节状态码语义在本 ADR 第 4 节有间接评估（防错误信息泄露），但不重新论证状态码表本身
- **不审 feature 层实现细节**：本 ADR 只给"项目层不变式"；feature implementer 在 P2-1.b 阶段写 feature ADR 时再细化（如 `e2e-encryption` ADR 决定具体 zeroize 调用点 / `file-transfer-drag` ADR 决定 sanitize 字符集 enum）
- **不审 LAN 信任假设本身**：_assumptions A19-A23（弹框审批 / X25519 ECDH / HKDF / AES-GCM / 无 CA / 抓包看不到明文）已用户校对 ✅；本 ADR 在此假设上做加固，不挑战假设本身

---

## 2. 威胁模型（Threat Model）

### 2.1 攻击面（Attack Surface）

| 面 | 入口 | 谁可触达 |
|---|---|---|
| LAN 抓包（被动监听） | 同 LAN 的混杂模式 / 镜像端口 / 路由器旁观者 | 任何同 broadcast domain 设备 |
| LAN 注入（主动 MITM） | ARP 欺骗 / 路由器篡改 / 同名 SSID 冒充 | 同上 + 网络控制权 |
| 恶意 peer（已握手 / 已审批） | HTTP 12 个端点之一（含加密路径 /clipboard /file 与明文路径 /handshake /ping /peers/* /history/*） | 至少完成过一次握手并被用户/审批通过 |
| 半死 TCP（隐形掉线 → 攻击面引申） | reqwest connection pool 复用旧连接 / OS 端口仍占用但对端 hang | OS 内核层 + 应用层 |
| 文件路径注入 | `/file` 端点 `filename` 字段 | 已握手的恶意 peer |
| 日志泄露（事后取证泄露） | 用户导出 zip 发开发者 / 日志文件被本机其它进程读取 / 截图含日志 | 用户 / 同机用户级进程 / 开发者 / 中间转发渠道 |
| 已被踢除但 IP 仍可达的旧 peer | 重连 reqwest pool 残留 / 缓存的 peer_keys 残留 / 短路 banned 集合状态 | 已被 ban 的设备（同 LAN 仍可达） |
| handshake DoS | `/handshake` 端点 + 30s 审批超时 + 弹框 emit 路径 | 任何同 LAN 设备（无前置认证） |
| panic / fatal 信道 | `std::panic::set_hook` + Tauri dialog + 文件日志 | 攻击者通过特定 payload 诱发 panic 路径 |

### 2.2 在场威胁主体（In-Scope Threat Actors）

1. **同 LAN 恶意设备**（A1）：同 broadcast domain 上某主机被对手控制，发起主动 MITM / 重放 / 注入 / 暴力探活 / 伪 handshake 灌弹框
2. **网络监听者**（A2）：能被动嗅探流量但无主动篡改能力（如旁观者镜像端口 / Wi-Fi 监听 / 不可信 ISP 路由器）
3. **已被踢除但 IP 仍可达的旧 peer**（A3）：曾通过握手 / 审批，后被 ban 或 leave；其 device_id 现仍在 banned_device_ids，但 IP 仍可达本机；尝试通过新 device_id / 篡改报文 / 重放老报文 / 利用残留状态绕过

### 2.3 不在场（Out of Scope，边界明确）

- **本地物理访问 A4**：攻击者拿到用户的笔记本（屏幕未锁）或 root shell。此时密钥在内存可读、日志可读、Config 可读、屏幕可看；本 ADR 不防御。_assumptions A38（配置文件不加密）已确认
- **供应链攻击 A5**：cargo crate（aes-gcm / x25519-dalek / hkdf / reqwest / axum / tokio / tracing 等）后门 / typosquatting；本 ADR 不防御，依赖 cargo 生态信任。release-engineer 在 ADR-N 决定是否引入 `cargo-deny` / `cargo-audit` 子层防御
- **用户主动泄密 A6**：用户主动把密钥 / 日志 zip 发给攻击者；用户主动开放某 peer 的 ban；本 ADR 第 6 节对"导出 zip 免责头"给建议但不强制
- **后量子密码学 A7**：X25519 不抗量子；e2e-encryption.md 第 3 节 out of scope 已声明；本 ADR 不审
- **侧信道（细粒度时间分析 / 功耗 / 电磁）A8**：同 LAN 设备通常没这种能力；标准 aes-gcm 0.10 / x25519-dalek 2 实现已是 constant-time（参考 RustCrypto 文档）；本 ADR 仅在第 4.4 节提示"已有保障，无需额外措施"

---

## 3. 加密路径分析（针对 ADR-003 第 3.4 节）

### 3.1 算法选型评级

| 项 | 选定 | 评级 | 说明 |
|---|---|---|---|
| 密钥协商 | X25519 ECDH（每次握手 EphemeralSecret 临时密钥对） | OK | RustCrypto x25519-dalek 2.x 是被广泛信任实现；EphemeralSecret 类型保证私钥消费即清；提供 forward secrecy 满足 e2e-encryption.md 第 4 节 AC 第 4 项 |
| KDF | HKDF-SHA256 | OK | RFC 5869 标准；32B 输出对 AES-256 足够；hkdf 0.12 + sha2 0.10 已广泛使用 |
| AEAD | AES-256-GCM（96-bit nonce + 128-bit tag） | OK | NIST SP 800-38D；aes-gcm 0.10 是 RustCrypto 主流实现；与 ChaCha20-Poly1305 等同级别 |
| 公钥编码 | 32 字节 raw + base64 标准编码 | OK | b64 长度 44 字符固定 |
| nonce 来源 | rand::rngs::OsRng（CSPRNG）| OK | OsRng 在 macOS 走 SecRandomCopyBytes / Win 走 BCryptGenRandom；满足 NIST nonce 唯一性的随机方案要求（96-bit 随机 nonce 在同一 key 下单调 < 2^32 报文时碰撞概率可忽略） |
| 算法不在场（被否决） | MD5 / SHA-1 / DES / RC4 / CBC-no-MAC | N/A | 全栈未引入 |

**整体结论**：算法选型完全合规；ADR-003 第 3.4 节决议 APPROVED on this dimension。

### 3.2 HKDF salt v2 bump 评级

ADR-003 第 3.4 节决议：`salt = b"sync-copy-v2-salt"` / `info = b"sync-copy-v2:aes-256-gcm"`（v0 是 v1）。

- **评级**：OK（设计选择）
- **效果**：HKDF salt / info 不同 → 派生密钥字节不一致 → v0 prototype 与 v2 build 即使共享 X25519 公钥也派生不出同一 AES-256 key → 互发报文时解密必定失败 → 协议层"自然版本互斥"
- **风险**：v0 用户升级到 v2 时若同时跑两端，会观察到"对端在线但收不到内容"——这是**预期行为**，不是 bug；release-engineer 在 v2.0.0 release notes 必须显式声明（已在 ADR-003 第 4.3 节列入"需要警惕的副作用"）
- **加固建议**：salt / info 字面量字符串作为常量在 `crypto/aes_gcm.rs`（或 `crypto/x25519.rs`）顶部 `const HKDF_SALT: &[u8] = b"sync-copy-v2-salt";` + `const HKDF_INFO: &[u8] = b"sync-copy-v2:aes-256-gcm";`，**必须**在文件级注释里写"协议版本字段；不兼容更改需 supersede ADR-008 + bump 到 v3"

### 3.3 nonce 处理评级

- **96-bit 随机 nonce 来自 OsRng**：v0 已用 `OsRng.fill_bytes(&mut nonce[12])`（见 legacy-prototype:crypto.rs L57-L58）；v2 trait 化后 `Sealer::encrypt` 内部仍用 OsRng；评级 OK
- **碰撞概率**：单 key 下 2^32 报文时碰撞概率 ~ 2^(-33)（生日界）；产品形态 N ≤ 8 设备 + 一天 100 次复制 + 5 年使用 → 单 peer-pair key 的报文数 < 2^18 远低于安全边界；评级 OK
- **重协商触发**：每次握手新密钥；peer 移除（ban / leave / heartbeat 剔除）→ 密钥 drop；强制重连（隐形掉线兜底 #1）触发 re-handshake → 新密钥；评级 OK
- **加固建议**：单元测试覆盖"同一 key 同一 nonce 加密两次明文 → 输出 nonce 字节不相等"（`e2e-encryption.md` AC #6 单测 5 条之外补 1 条 nonce CSPRNG 检查；本 ADR 第 7 节列入必修）

### 3.4 密钥生命周期

ADR-003 第 3.4 节给出生命周期表（4 行）。本 ADR 加固如下：

| 密钥 | 生命周期（ADR-003） | 加固（ADR-008） |
|---|---|---|
| 临时 X25519 EphemeralSecret | 单次握手，调用 derive 即消费 | derive 函数签名保证消费所有权（v0 已是；trait 化后保留同语义）；不需 zeroize（EphemeralSecret 自带 Drop 内置 zeroize） |
| 共享秘密（DH 输出 SharedSecret 32B） | 函数局部 | x25519-dalek 2.x 的 SharedSecret 类型 Drop 时**已自动 zeroize**（参考 RustCrypto/x25519-dalek issue #56 + 文档）；trait `KeyExchange::derive_aes_key` 内部把 SharedSecret bind 局部变量 `shared` 即可，**不需**显式 `zeroize::Zeroize::zeroize` 调用 |
| AES-256 per-peer key（[u8;32]） | PeerRegistry.inner[id].aes_key 持有 | **必修**：引入 `zeroize` crate（0.8 系列）；`PeerState.aes_key` 改为 `Zeroizing<[u8; 32]>` 包装；`PeerRegistry::remove(id)` 时 Drop 自动清零（不再依赖 HashMap remove 默认行为） |
| 长期密钥 | 无 | OK；保持 |

**关键发现 [中]**：v0 现状（plain `[u8; 32]`）在 HashMap remove 时密钥字节仍可能在内存中残留（HashMap 桶 / 旧 String 重分配 / Vec 增长 realloc 等），ADR-003 第 3.4 节生命周期表已点名但**未强制**引入 zeroize，留给 ADR-008。本 ADR 第 7 节决议：**强制引入 `zeroize`**（理由见第 3.5 节）。

### 3.5 zeroize 引入决议

**决议**：引入 `zeroize = "1.8"` 作为新依赖，仅用于 `PeerState.aes_key` 字段。

**理由**：
1. 实施成本低：只改 1 个 struct 字段类型 + 1 处构造；不影响调用点
2. 防御目标明确：进程被 dump（macOS sample / Win Process Explorer mini-dump / panic 触发 OS core dump）时，AES-256 per-peer key 不出现在 dump 文件
3. SharedSecret / EphemeralSecret 已自带 zeroize（x25519-dalek 2.x），引入 zeroize crate 是把保护链延伸到 [u8; 32] AES key 端
4. CLAUDE.md 第 9 节"密钥不写文件"约束的内存层补强；与 _assumptions A23"抓包看不到明文"配套

**反对意见的回答**：ADR-003 第 4.2 节"AAD / zeroize / PSK 三个安全决议本 ADR 不锁定"——本 ADR 现锁定 zeroize（必修）；AAD 锁定（必修，详见 3.6）；PSK 不锁定（详见 3.7）。

**实施提示**：`use zeroize::Zeroizing;` + `pub aes_key: Zeroizing<[u8; 32]>`；`PeerRegistry::remove` 不需特殊处理，Drop 自动清零。单元测试补 1 条："drop 后内存某区域字节模式不再含原 key"——但跨平台不可靠（编译器优化 / 内存分配器复用），故只在 ADR 级强制类型，不在 AC 强制行为测试。

### 3.6 AAD 入参 trait 签名预留 + 绑值决议

ADR-003 第 3.4 节：`Sealer::encrypt(&self, key, plaintext, aad)` 在 trait 签名预留 aad 入参，但**不锁定值**，留 ADR-008。

**决议**：**强制 AAD 绑值**。绑值组合 = `b"sync-copy-v2"` 协议 magic || `kind: &str` || `origin_device_id: &str` || `seq: u64 (big-endian 8 bytes)`。

**理由**：

1. **防跨 kind 重放**：v0 现状 AAD 为空，理论上一条加密的 `text` clipboard 报文可被攻击者把 JSON 字段 `kind` 改成 `image_png` 重放（GCM tag 仍校验通过因为 ciphertext 字节没动）；虽 v2 后 image_png 路径需要 image_width / image_height 字段不会无脑解释，但 `is_snapshot` flag（`clipboard-snapshot-sync` 在 ADR-003 第 3.2 节决议复用 /clipboard）使 snapshot 报文与正常 broadcast 报文密文层无法区分。AAD 绑 kind 让"快照变正常推送"在密码学层即被拒
2. **防跨 peer 重放**：A → B 的报文在 LAN 内被 C（同样已握手过 A）截获后**重放给 B**——若 A↔B 与 A↔C 共享 origin_device_id（A）而 AES key 不同，则 GCM tag 校验失败（密钥不同），AAD 不绑值此场景已挡；但若 C 截获 A → B 后**伪造 origin_device_id 为 C** 改 JSON 字段重发给 B（C 与 B 已握手，有 AES key），tag 用 A↔B 的 key 算的 → C↔B 的 key 算不通过 → 攻击失败。这条 v0 已隐式有保护。但 AAD 绑 origin_device_id 让"middlebox 改 origin 字段"在密码学层立即被拒，多一道防御
3. **防 seq 重放（密码学层而非 dedupe 层）**：v0 / v2 都靠 PeerRegistry.seen_seq_and_update 应用层 dedupe；AAD 绑 seq 是密码学层第二道闸——攻击者把密文 / nonce 抄出来改 JSON `seq` 字段重发，AAD mismatch 立即拒绝（不依赖应用层 dedupe）
4. **协议 magic `sync-copy-v2`**：与 HKDF salt v2 配合，让 v0 prototype 与 v2 build 的报文在 AAD 层也不互通；冗余防线

**反对意见的回答**：

- "AAD 绑 origin_device_id 让 origin 字段被密码学锁死，未来如果想做 anonymous 模式不行了"——anonymous 模式不在 v2 范围（_assumptions 19/20 信任靠审批 + device_id 显式标识，反 anonymous）；future ADR-N supersede 时再讨论
- "AAD 拼接需 implementer 写一段 builder 代码，比 v0 的 aad: &[] 复杂"——总成本 < 10 行 Rust；trait 设计已预留入参，落地是 impl 的事

**实施提示**：在 `crypto/aes_gcm.rs::AesGcmSealer::encrypt` 内部，把 4 段拼到一个 `Vec<u8>` 作为 aad 传给 aes-gcm crate；调用方（network/handlers/clipboard.rs / file.rs）把 kind / origin_device_id / seq 按 caller 上下文传入。单元测试补 1 条："改 aad 中任一字节后 decrypt 失败"。

### 3.7 PSK / 短口令认证决议

ADR-003 不锁定；e2e-encryption.md 第 7 节 [P0] 议题；本 ADR 决议。

**决议**：**v2.0 不引入 PSK**。

**理由**：

1. **威胁模型不充分**：PSK 防御目标是"主动 MITM"（A1 同 LAN 恶意设备做 ARP 欺骗篡改公钥）。在 _assumptions A19 已确立"信任靠弹框审批 = 用户在场识别设备名"前提下，主动 MITM 攻击者的设备名会显示在审批弹框上让用户人工拒绝（社会工程层防御）。PSK 把"用户输入 6 位口令"作为额外认证锚点确实更强，但
2. **产品体验回归**：用户校对 _assumptions A19 ✅，明确接受"靠审批弹框"；引入 PSK = 每次入组多一步"输 6 位码"；与产品定位"不需要管理密钥 / 密码"（e2e-encryption.md 第 2 节 user story #2）冲突
3. **威胁主体能力不对称**：A1 主动 MITM 在同 LAN 物理可行但**实际门槛**= 攻击者能持续控制 ARP 表 + 同时控制双方与攻击者各自握手 + 在用户审批阶段同时把"伪装的 device_name"放上去**让用户审批通过**——与"用户被钓鱼到点 OK"等价。PSK 在用户都会被钓鱼审批通过的场景下并不真正提高门槛
4. **v2.0 焦点**：v0 实战 bug（隐形掉线 / fatal 错误 / 日志取证）是真实痛点；PSK 是"理论上更强但用户感知收益低"的项；保持 v2.0 范围聚焦
5. **可演进**：trait `Verifier::verify_origin(&self, claim: &str)` 已预留；未来 ADR-N 引入 PSK 时只改 impl + 增握手字段，不破坏 trait 边界

**保留的不变式**（弹补强）：

- 握手公钥不加密但**审批弹框必须显示 device_name 并要求用户主动点同意**——这条已在 group-approval / group-discovery spec；本 ADR 不重论证，仅引用
- 审批通过后 trust gossip 让被信任的 peer device_id 入 approved_device_ids；下次握手短路审批——这是产品已定能力；MITM 在这条路径上**只能在第一次审批时偷过**，无法事后补伪装

---

## 4. 协议层分析（针对 ADR-003 第 3.6 节 + 第 7 节 10 项待审议题）

### 4.1 7 状态码语义不泄露内部状态评级

ADR-003 第 3.2 / 3.6 节 7 状态码（200/400/403/408/409/413/422/500）。

| 码 | ADR-003 语义 | 安全审阅 | 备注 |
|---|---|---|---|
| 200 | OK | OK | 全部成功；含 dedupe 命中静默 200（v0 行为，保持） |
| 400 | 请求格式错 / size 字段不通过 | OK | 不能在 error message 中嵌入 internal Rust panic 信息 / file path / config 字段；仅 "invalid request" 字面 |
| 403 | 鉴权失败 / 用户拒绝 / 在 banned 列表 | **加固**：当前 v0 server.rs 在 handshake 拒审批时返 403（见 legacy:server.rs L223-L243）；区别于"未握手 + 不在 peers 表"的 403；但**对外不应区分这两种**——攻击者从外部观察 403 不应能区分"我被 ban 了"vs"对方还没认识我"。v2 实现层必须使 403 对外不可区分（不同 internal 路径返同一 statuscode + 同一 body 串） | 否则 ban 探测变可能（攻击者反复改 device_id 探"哪个 id 能过初步 403"） |
| 408 | 审批超时 30s | OK | 状态码外部观察含义清晰（不会泄露用户在弹框上点了什么）；用户拒绝走 403 / 用户超时不点走 408 / 区分对应攻击者价值低 |
| 409 | device_id 冲突 | **加固**：v0 在自己 device_id == req.device_id 时返 409（见 legacy:server.rs L113）；这条让攻击者枚举本机 device_id 变可能（用 1000 个 device_id 探，命中即知本机 id）。v2 实现层必须**返 200**或**返与 403 同样的"forbidden"**而非 409 区分。本 ADR 第 7 节列入必修 |
| 413 | size > MAX_FILE_SIZE = 5 MB | OK | 见 4.6 节 |
| 422 | 解密失败 / plaintext.len != size mismatch | **加固**：v0 在 decrypt 失败时返 401 UNAUTHORIZED（见 legacy:server.rs L312 / L654）；ADR-003 改成 422 是合理（不暗示"401 鉴权"语义）；但实现层必须**只返 statuscode 不带 body 描述**（"decrypt failed" vs "size mismatch" 不应区分对外可见）；统一返"crypto or size mismatch"通用串 | 时序信息泄露最小化（ADR-003 第 3.6 节"错误信息泄露"评估） |
| 500 | 写盘失败 / 不可恢复内部错 | OK | 不返路径 / 不返 errno 字符串 / 仅"internal error" |

**关键发现 [中]**：

1. **状态码 409 + body 暗示 "device id conflict" 让本机 device_id 可枚举**：本 ADR 第 7 节必修——此场景统一为 403 + 通用 body
2. **403 须不可区分内部三种路径**（ban 命中 / 不在 peers 表 / 用户拒绝）：本 ADR 第 7 节必修——所有 403 返同一 body 串

### 4.2 重放保护（seq）覆盖度

| 端点 | seq 校验？（v0 现状） | 评级 |
|---|---|---|
| /handshake | 不需（每次新建会话，pubkey 是 anti-replay）| OK |
| /clipboard | seen_seq_and_update（见 legacy:server.rs L300）| OK |
| /file | **未做** seen_seq_and_update（v0 legacy:server.rs L632-L649）| **加固**——见下文 |
| /delete_history | seen_seq_and_update | OK |
| /history/clear | seen_seq_and_update | OK |
| /peers/trust | seen_seq_and_update | OK |
| /peers/ban | seen_seq_and_update | OK |
| /peers/leave | seen_seq_and_update | OK |
| /peers/approval/forward | seen_seq_and_update | OK |
| /peers/approval/decide | seen_seq_and_update | OK |
| /peers/approval/dismiss | seen_seq_and_update | OK |
| /ping | 无（GET，不带 origin）| OK（探活无敏感 effect） |

**关键发现 [严重]**：v0 `/file` 端点缺 seen_seq_and_update（legacy:server.rs L632 起的 handle_file）。攻击者重放一条已成功被审批接受的 /file 报文 → 在用户接受第二次审批时再次写盘到 Downloads（虽 unique_path 改文件名，但仍是用户被骗保存了"看似全新的文件传输"）。即使 AAD 绑 seq（本 ADR 决议）让密码学层防 seq 改动，但**重放原报文不改 seq 仍能通过 AAD**——靠应用层 seq dedupe 是必须的。**本 ADR 第 7 节必修：v2 `/file` handler 必须先做 seen_seq_and_update + 命中即返 200 静默丢弃**（与其它端点保持一致）。

### 4.3 handshake DoS 限流

ADR-003 第 7 节 议题 7（同 group-discovery / group-approval 第 7 节 [P0]）。

**威胁**：A1 同 LAN 恶意设备每秒发 N 个伪 handshake → 本机弹 N 个审批框 → 用户被淹没；甚至触发"forwarded approval"广播给所有 peers 让全组都被淹

**决议**：**必修**——增加 handshake 端点限流。

**实施约束**（不锁实现细节，留给 implementer 在 group-discovery / group-approval feature ADR）：

- **每对 (remote_ip, device_id) 在 60s 窗口内最多 3 次 handshake 尝试**；超过返 429 Too Many Requests（轻量限流，本 ADR 在第 4.1 节扩展状态码表为 8 项）
- **每 LAN 60s 内最多 10 个不同 device_id 的握手尝试**（防 attacker 用不同 device_id 绕第一条）；超过暂时拒绝新 handshake（仅返 429）
- **forwarded approval 不能再触发新的 forward**（防级联放大）；ADR-003 第 3.5 节 / handshake handler 已隐含这条，本 ADR 显式声明
- **指数退避 / token bucket** 等具体策略由 implementer ADR 决定；本 ADR 仅给阈值上限和外部行为约束

**配套**：429 不应在 body 区分"per-pair 限"vs"全局限"（同 4.1 节"403 不可区分"原则）；统一 "too many requests" 串。

### 4.4 device_name 字符集 / 长度限制

ADR-003 第 7 节 议题 8。

**威胁**：A1 在 handshake 字段 `device_name` 注入 Unicode RTL 反向覆盖字符（U+202E）/ NUL / 控制字符 / 大量 emoji / 超长 8KB 字符串 → 在审批弹框 / floating-window peers 列表 / 设置面板 / 日志中渲染异常 / 引发 UI 截断 / 隐藏部分内容欺骗用户

**决议**：**必修**——device_name 在 handshake handler 接收处 sanitize。

**约束**：

- 长度 ≤ 64 字符（Unicode codepoints；不是 byte）
- 字符集 = Unicode L\* / N\* / S\* / P\*（字母 / 数字 / 符号 / 常见标点）+ 空格；**禁止**控制字符（U+0000 - U+001F + U+007F-U+009F）+ **禁止** Bidi 控制字符（U+202A-U+202E + U+2066-U+2069 + U+200E / U+200F）
- 不通过则替换为 `"<unnamed>"` 或拒绝（403）；implementer 决定
- 同样规则用于：日志记录前 sanitize / 弹框 emit 前 sanitize / floating-window peers 列表渲染前 sanitize（**Rust 后端层就 sanitize**，前端不能信任）

**理由**：单点 sanitize 在握手 handler / forward handler / trust handler / approval handler 接收处一次性；不在前端再做（UI 层已有 v0 教训"前端展示用户输入需事先 sanitize"）

### 4.5 filename sanitize 加固

ADR-003 第 7 节 议题 5；v0 已有 `sanitize_filename`（legacy:server.rs L753-L768）但需加固。

**威胁**：

- 路径穿越：`../../etc/passwd`（v0 已用 `Path::file_name()` 取 basename 防住）
- 绝对路径 / drive letter：`C:\Windows\system32\xxx`（Win） / `/tmp/xxx`（Unix）（v0 用 `Path::file_name()` 防住）
- Windows 保留名：`CON`, `PRN`, `AUX`, `NUL`, `COM1-COM9`, `LPT1-LPT9` 不能作为文件名（v0 **未防**）
- Unicode RTL 反向覆盖：`exploit.‮gpj.exe` 显示为 `exploit.exe.jpg`（v0 **未防**）
- 控制字符 / NUL：v0 仅过滤 `/ \ \0`（legacy:server.rs L760）
- 超长名：v0 限 200 字符（legacy:server.rs L763-L765）OK

**决议**：**必修**——v2 `sanitize_filename` 增强：

1. 已有的 basename + 200 字符限制保持
2. 增加过滤集：所有控制字符 U+0000-U+001F + U+007F-U+009F；Bidi 控制字符（同 4.4 节）；保留字符集 `< > : " | ? *`（Win 文件名禁用字符 / 防 NTFS ADS 等）
3. **Windows 保留名**：basename 去 ext 后大写比对 8 个保留前缀（CON / PRN / AUX / NUL / COM[0-9] / LPT[0-9]）+ 末尾"."或" "（Win 不允许）；命中则前缀 `_`
4. 空字符串兜底"file"（v0 已有）
5. 单元测试补：路径穿越 / Win 保留名 / RTL 字符 / 超长 / NUL 各 1 条

**实施提示**：单点函数实现于 `network/handlers/file.rs`（可考虑提到 `peer/sanitize.rs` 复用 device_name sanitize）；filename sanitize **必须在审批弹框 emit 前**完成，前端不能信任

### 4.6 file size 在解密之前 validate

ADR-003 第 7 节 议题 6；v0 已实施（legacy:server.rs L644-L646），但**需加固**。

**v0 现状**：在 decrypt 之前 `if req.size > MAX_FILE_SIZE { return PAYLOAD_TOO_LARGE }`；但 `req.size` 是攻击者声明字段——攻击者声明 size=1KB 但 ciphertext 实际 6MB，仍能让本机做 6MB decrypt（CPU + 内存）。decrypt 后 v0 第二道闸 `if plaintext.len() != req.size` 防 size lie（legacy:server.rs L656）。

**关键发现 [中]**：v0 现有顺序是"声明 size 早期校验 + decrypt + 实际 size 二次校验"。问题：第一道闸只看声明值，攻击者可声明 size=1KB 让本机 decrypt 6MB ciphertext。**轻量 DoS 攻击面**。

**决议**：**必修**——v2 `/file` handler 在 decrypt 之前**双道**校验：

1. **声明 size 校验**：`req.size > MAX_FILE_SIZE` → 413（保留 v0）
2. **新增：实际 ciphertext base64 解码后字节长度校验**：`base64_decode(ciphertext).len() > MAX_FILE_SIZE + GCM_TAG_SIZE(16)` → 413（先校验 + 再 decrypt）。也可在 axum DefaultBodyLimit 层先挡（v0 设 8MB body limit on legacy:server.rs L57，已防最外层）；**但**应用层补一道更严的"按业务上限 5MB + 16B tag + base64 33% 膨胀 ≈ 6.7MB"校验
3. **decrypt 后**：保留 `plaintext.len() != req.size → 422`（v0 已有）

**配套**：DefaultBodyLimit 也应从 v0 8MB 收紧到 7MB（`5MB + 16B tag + base64 33% + JSON header overhead 50KB ≈ 6.7 + 0.05MB`），与应用层校验对齐；implementer 在 file-transfer-drag feature ADR 落实细节

### 4.7 content_hash → HMAC 决议

ADR-003 第 7 节 议题 4；clipboard-text-sync / clipboard-image-sync / history-sync-delete 第 7 节 [P0]。

**v0 现状**：`history::sha256_hex(plaintext)`（明文 SHA-256），用于 history dedupe（同内容只存一条）和 `delete_history` 跨机器删除（按 hash 发到对端，对端按 hash 找历史条目删）。

**威胁**：A1 已握手为 peer，能枚举常见 secret 字符串 SHA-256 值（如 6 位数字密码 hash 暴力）→ 在 `delete_history` 端点用枚举的 hash 发请求 → 探"对端历史中是否含该 secret"；攻击成功 = 信息泄露（"目标曾复制过 password123 这串")。SHA-256 + plaintext 无 key，hash 可全网共享 → 字典攻击可行

**决议**：**必修**——`content_hash` 改为 **HMAC-SHA256(per-peer-key, plaintext)**。

**理由**：

1. HMAC 引入 per-peer-key（即 PeerState.aes_key 同源派生或同 key 直接用）→ 攻击者无 key 无法枚举常见字符串的 hash
2. 不同 peer-pair 的同一明文 HMAC 不同 → 跨 peer dedupe 仍生效（v0 已是按本机 hash 跨设备同步删除：A 算 hash 发给 B，B 用同一 hmac 算）；但**前提**是 A 与 B 的 HMAC key 相同——这违反 per-peer-pair key 模型
3. **关键**：HMAC key 应**全组共享**而非 per-pair。但 v2 当前架构仅有 per-pair 密钥；**全组共享密钥**会破坏 forward secrecy 与 group-approval 设计

**修订决议**：

- **v2.0 不强制 HMAC**；保留 SHA-256(plaintext) 作为短期方案
- **必修**：在 history-sync-delete 端点接收 `content_hash` 后做**仅本机历史范围匹配**（v0 已是；legacy:server.rs L378-L390）；`peer.is_known()` 校验保留；**不允许**未知 origin 的删除请求
- **加固**：把 dedupe / 跨设备删的真实 schema 改为 `(origin_device_id, seq_at_creation)` 复合主键而非纯 hash（即跨设备删时发 origin+seq 不发 hash），但这是 history-sync-delete feature ADR 的实施细节；本 ADR 第 7 节作为"建议非必修"列入
- **未来 ADR**：当 v2.x 引入"全组 epoch key"（如 group rekeying 协议）时再切到 HMAC；现阶段成本不值

**安全审阅诚实声明**：原议题（content_hash → HMAC）在 v2 当前架构下**实际不可行**（缺全组共享 key）；本 ADR 决议给"短期保留 SHA-256 + 加固应用层校验 + 未来 ADR 评估全组 key 与 HMAC 联动"的折中方案。这条 PM 的 P0 议题在威胁模型实际分析后降级为"非必修，feature ADR 补"。

### 4.8 /ping origin 校验

ADR-003 第 7 节 议题 9；peer-heartbeat.md 第 7 节 [P1] [安全]。

**威胁**：A1 同 LAN 任意设备 GET /ping → 200 OK + "pong" → 探活成功 → 知道"本机跑 Sync Copy 服务"。信息泄露面 = 同 LAN 已知。

**决议**：**不必修**——保持 v0 / ADR-003 现状（/ping 无 origin 校验，纯探活）。

**理由**：

1. 同 LAN 端口扫描已是基础能力（nmap）——本应用绑 5858（默认）已暴露于扫描；/ping 是否有 origin 校验对扫描者价值差异 = 0
2. 加 origin 校验会让 PeerRegistry 记录的对端在重启自身后**仍能 ping 通**（因为 origin 仍在 peers 表）但实际对端密钥已丢——这是 peer-heartbeat.md 5.4 节"B 重启后 device_id 不变"边界 case；增加 ping origin 校验不解决该 case
3. 隐形掉线 v0 实战 bug 的核心是"心跳成功 ≠ 真同步"——对此问题，AAD 绑值 + last_successful_sync_at 仅在广播 200 OK 时写（ADR-003 第 3.7 节决议）已是更强防御；origin 校验对此 bug 无贡献
4. /ping 必须保持简单（peer-heartbeat 单文件 ≤ 100 行的 v2 设计目标），加校验逻辑增加复杂性

**保留**：implementer 可在 implementer ADR 中**增加来源 IP 限制**（例如非 RFC1918 网段的 ping 拒绝），属低优化。本 ADR 不列必修。

---

## 5. lifecycle / 隐形掉线安全（针对 ADR-003 第 3.7 节）

### 5.1 per-peer Client drop 时密钥也 drop（zombie key 防御）

ADR-003 第 3.7 节决议 `client_pool.rs` 维护 per-peer `Arc<reqwest::Client>`，强制重连 = `pool.replace(peer_id)` 让旧 Client + connection pool 一起 drop；4.3 节"client_pool.rs 的 Client 生命周期与 PeerRegistry 同步"已点名"PeerRegistry.remove 触发 pool.remove"。

**安全审阅评级**：APPROVED + 加固。

**关键发现 [中]**：当前 ADR-003 决议方向正确但**未明示密钥 drop 的钩子顺序**——若 client_pool.remove 早于 PeerRegistry.remove，可能存在窗口期"密钥仍在 PeerRegistry 但 Client 已 drop"或反之；A3（已被踢除但 IP 仍可达的旧 peer）可在窗口期发请求触达（虽然 ban 集合可挡住，但密钥语义不一致）。

**必修**：PeerRegistry.remove(id) 内部按以下顺序原子执行（在 RwLock 写锁内或单元化函数）：

1. `inner.remove(&id)`（让 PeerState drop → Zeroizing<aes_key> 自动清零）
2. `client_pool.remove(&id)`（drop reqwest::Client → connection pool drop）
3. **不要**在 lookup miss 时自动 add Client：v0 现状 `client.rs::build_client()` 是按需 build（每次广播构造）；v2 的 client_pool 必须在**握手成功 / 重新握手**时 insert，**peer remove 时同步 remove**——禁止按需 lazy add 让 zombie peer 复活

### 5.2 last_successful_sync_at 不写心跳成功的语义安全

ADR-003 第 3.7 节决议：`last_successful_sync_at` 仅在广播报文（clipboard / file / trust / leave / delete_history）拿到 200 OK 时写；不在心跳 200 OK 时写。

**安全审阅评级**：APPROVED。

**理由**：心跳 200 OK 仅证明对端 axum runtime 在跑 + 网络可达；不证明加密路径完好（密钥同步 / decrypt 工作 / history push 工作）。隐形掉线 bug 根因是"心跳成功 ≠ 真同步"；ADR-003 这条语义是 bug 解决方案的核心不变式。

**配套加固**：UI 显示"上次同步：相对时间"（peer-heartbeat.md 第 4 节 AC #11）让用户在表面绿但实际死透时一眼识破——这是用户层防御；密码学/协议层无对应措施。本 ADR 不补充必修，仅引用。

### 5.3 强制重连后的重新握手是否需要新协商密钥

ADR-003 第 3.7 节：连续 N=3 次心跳失败 → `client_pool.replace(peer_id)` + `spawn re_handshake(peer)`。

**安全审阅评级**：APPROVED + 加固。

**问题**：re_handshake 协议层就是普通 /handshake 调用——v0 / v2 的 handshake handler 在已知 peer（known = peers 表已含 origin device_id）时直接覆盖密钥（legacy:server.rs L120-L141 "re-handshake with known peer, key refreshed"）。这意味强制重连后**密钥确实会换新**（新 EphemeralSecret + 新 DH + 新派生）→ 旧密钥 drop（Zeroizing 清零）→ 强 forward secrecy 满足。

**关键发现 [低]**：known peer re-handshake 路径**跳过审批弹框**（v0 行为，legacy:server.rs L132-L141）。在 A3 威胁主体下：已被踢除（在 banned）的旧 peer 通过新 device_id 来 → 走"unknown peer"路径正常审批 OK；但**已被踢除的旧 device_id 在 banned + 不在 peers**，handshake handler 先查 banned 直接 403 OK；那么 A3 的**旧 device_id 若仍在 peers 表**（即 PeerRegistry.remove 未发生但用户在 settings 主动让它再握手）→ re-handshake 静默跑过——这是 group-approval 已确立的 UX，不视为安全问题。但**强制重连触发的 re-handshake**虽然按 ADR-003 是 best-effort，但需要确保：

**必修**：强制重连触发的 re_handshake 调用前必须**校验 peer 仍在 PeerRegistry**且**不在 banned**（A3 防御）；若已被 ban / 已被 leave 移除则不触发 re-handshake（避免 zombie peer 通过 force-rebuild 路径复活）。这条钩子由 health worker 实现（network/health.rs）。

---

## 6. fatal error 三件套（ADR-003 第 3.6 节 / v4-7）

ADR-003 第 3.6 节："std::panic::set_hook → tracing::error 入文件 + Tauri dialog（runtime 死时 OS 原生 MessageBox 兜底）+ process::abort 不静默"。

### 6.1 panic message / backtrace 信息泄露评估

**威胁**：panic payload / backtrace 可能含敏感字段（明文剪切板 / aes_key 字节 / 内存地址 / 配置路径 / 用户名）

**评级**：

- **panic payload 字面**：Rust 默认 panic format 含 `panicked at 'msg', src/foo.rs:N` —— msg 是 `panic!("...")` 字面；如代码不慎 `panic!("decrypt failed: key={:?}", key)` 则密钥进 backtrace。但实际 v0 / v2 的 unwrap / expect 不该带 key 参数 → 风险来自**未来 implementer 误用**
- **backtrace**：含函数符号 + 源码行号 + 栈变量地址；通常**不**含变量值字面，除非 RUST_BACKTRACE=full + 特殊 hook

**必修**：

1. **panic hook 实现层强制 sanitize**：在 `set_hook` 闭包内，**只**记录 `panic_info.location()`（文件 + 行号）+ `panic_info.payload().downcast_ref::<&str>() / String` 的字面 + backtrace；不包括栈变量值
2. **强制约定**：所有 `panic! / unwrap / expect` 调用点的 message **不得**含运行时变量插值（`format!("{:?}", key)`）；implementer code review 时检查；本 ADR 列入"敏感字段黑名单"配套
3. **dialog 文案**：用户面对的 dialog 不显示 panic message 字面（攻击者无法通过特定 payload 让用户截图发出敏感数据）；统一文案"Sync Copy 遇到致命错误，已写入日志：~/Library/Logs/com.synccopy.app/sync-copy.log，请在设置 → 导出日志后联系开发者"

### 6.2 日志写盘的隐私评估

ADR-003 第 3.6 节"敏感字段黑名单：剪切板明文 / AES key / X25519 私钥 / shared secret / HKDF 中间值 永不进 tracing fields"。

**评级**：APPROVED + 加固。

**diagnostic-logging.md 第 7 节 [P0] [安全]** 议题：device_id / device_name / peer IP 是否记？

**决议**：

| 字段 | v2 决议 | 理由 |
|---|---|---|
| 剪切板明文（text / image bytes / file bytes） | **绝不记** | _assumptions A23 ✅ |
| AES key / X25519 SharedSecret / HKDF 中间值 / EphemeralSecret 字节 | **绝不记** | 同上 |
| device_id（UUID 形式） | **可记** | UUID 形式跨设备 stable 标识；用户导出 zip 给开发者时帮助定位"哪台设备出问题"；不是 PII（不含真名、不含 MAC）；v0 已记（legacy:server.rs 多处 `peer = %req.device_id`）保留 |
| device_name（用户自定义） | **截短记**：仅记前 20 字符 + sanitize（同 4.4 节字符集）；超过截断 + "..."；理由：device_name 可能含个人信息（"张三的 Mac"），日志导出场景下 leak 给开发者；用户可能不愿暴露姓名 |
| peer IP（LAN 内 192.168.x.x） | **可记**：LAN IP 不是 PII（与公网 IP 不同）；调试网络问题必需；用户场景导出 zip 给开发者时知 IP 帮助定位"哪个网段问题" |
| port | 可记 | 同 IP |
| filename（file-transfer 路径） | **截短记 + sanitize**：仅记前 50 字符 + sanitize（同 4.5 节）；用户的文件名可能含个人信息（合同_张三.pdf）；同 device_name 处理 |
| Config 文件路径 | 可记（路径 stable）| 标准 OS 目录 |
| backtrace 中的栈变量值 | **绝不记** | 同 panic 评估 |

**必修**：建立 `log/sanitize.rs`（或集成到 tracing 自定义 layer）作为单点 sanitize 函数，所有 tracing! 调用前 device_name / filename / panic message 走该函数。implementer 在 diagnostic-logging feature ADR 决定具体实现（自定义 Subscriber Layer / 或调用前手动 sanitize）；本 ADR 仅强制行为。

### 6.3 dialog 是否泄露内部状态

`commands::group::quit_app` 路径的 fatal dialog：fatal 后不再静默（process::abort）；用户看到原生 MessageBox 提示"已写入日志请联系开发者"。

**评级**：OK。dialog 文案不含 panic message 字面（见 6.1）；不含密钥 / 路径细节；不含 internal state；仅指引用户找日志位置。

---

## 7. 关键发现汇总 + 必修清单

### 7.1 关键发现分级

**[严重] 严重发现（必修）**

1. **/file 端点缺 seq dedupe**（v0 现状 + ADR-003 未点名）：攻击者重放已审批接受的文件传输报文 → 用户被骗二次接受。复现：在 A 上 send_files 后抓包 → 在攻击者控制的 peer 上重放原报文 → 用户在 B 端再次看到 file-pending 弹框

**[中] 中危发现（必修）**

2. **HKDF 派生 AES key 缺 zeroize**：进程 dump 时 [u8; 32] 残留内存可读。改 `Zeroizing<[u8; 32]>` 即可
3. **AES-GCM AAD 为空**（v0 + ADR-003 入参预留未锁值）：跨 kind / 跨 origin / 跨 seq 重放在密码学层无防御，仅靠应用层 dedupe。AAD 绑 `b"sync-copy-v2" || kind || origin_device_id || seq_be8` 闭环
4. **状态码 409 device_id conflict 让本机 device_id 可枚举**：改返 403 + 通用 body
5. **状态码 403 内部三种路径不可区分**（ban / 不在 peers / 用户拒绝）：所有 403 返同一 body 串
6. **/file decrypt 前仅校验声明 size 不校验实际 ciphertext byte 长度**：轻量 DoS 攻击面（声明 1KB 实际 6MB ciphertext 仍解密）。在 decrypt 前补 base64 解码后字节长度校验
7. **handshake DoS 限流缺失**：每对 (remote_ip, device_id) 60s 内 ≤ 3 / 全局 60s 内 ≤ 10 个不同 device_id；超返 429
8. **device_name 字符集 / 长度未约束**：Bidi 控制 / 控制字符 / 8KB 长字符串污染 UI 与日志。后端单点 sanitize（≤ 64 codepoints + 字符集白名单 + Bidi 黑名单）
9. **filename sanitize 未防 Win 保留名 + Bidi + 控制字符**：v0 仅过滤 `/ \ \0` + 200 字符限。补 Win 保留前缀检测 + 控制字符 + Bidi 字符 + 末尾 "."/" "
10. **panic / unwrap / expect 调用点 message 未约束**：implementer 误用可让 key / 明文进 backtrace。约定 message 不得含运行时变量插值
11. **PeerRegistry.remove 与 client_pool.remove 钩子顺序未明示**：A3 威胁主体在窗口期可触达。原子顺序：先 inner.remove → 后 pool.remove；禁止 client_pool 按需 lazy add
12. **强制重连触发的 re_handshake 未校验 banned 状态**：A3 zombie peer 复活路径。health worker 内 force-rebuild 前先查 PeerRegistry + banned

**[低] 低危发现（建议非必修）**

13. **HMAC 替代 SHA-256(plaintext) 跨设备删除**：在 v2 当前 per-pair key 架构下不可行；保留 SHA-256；未来 ADR 评估"全组 epoch key + HMAC"联动
14. **/ping origin 校验**：同 LAN 端口扫描已暴露服务存在；不必修
15. **PSK / 短口令认证**：v2.0 不引入；社会工程层（审批弹框 + device_name）已是当前威胁模型下的可接受防御

### 7.2 必修清单（implementer 落地前必须做）

> 以下 8 条按"项目层"+"feature 层"分类。"项目层"指应在 P2-1.b 第一批基础设施 PR（PeerRegistry / Lifecycle / crypto traits / network::error / network::handlers boundary）落地。"feature 层"指对应 feature ADR 阶段细化。

**项目层（基础设施 PR 必须做）**

- [ ] **MUST-1**：`crypto/aes_gcm.rs::AesGcmSealer::encrypt/decrypt` 实现层把 AAD 绑值 `b"sync-copy-v2" || kind.as_bytes() || origin_device_id.as_bytes() || seq.to_be_bytes()`；trait `Sealer` 签名保持 ADR-003 第 3.4 节预留入参；调用方（network/handlers/clipboard.rs / file.rs）传值。单元测试补"改 aad 任一字节后 decrypt 失败"。【对应发现 #3】
- [ ] **MUST-2**：引入 `zeroize = "1.8"` crate；`peer/state.rs::PeerState.aes_key` 类型改为 `Zeroizing<[u8; 32]>`；其它密钥（EphemeralSecret / SharedSecret）保持 x25519-dalek 自带 zeroize。【对应发现 #2】
- [ ] **MUST-3**：`network/error.rs::NetworkError → IntoResponse` 实现层把 `DeviceIdConflict` 改返 403 + 通用 body；`Forbidden(reason)` 的 reason 不出现在 HTTP body（仅入日志），所有 403 对外返同一 body 串"forbidden"；422 同样统一 body 串。【对应发现 #4 / #5】
- [ ] **MUST-4**：`peer/mod.rs::PeerRegistry::remove(id)` 实现层先 `inner.remove(&id)` 后 `client_pool.remove(&id)` 原子（在写锁内或 Mutex 内一次完成）；client_pool 不支持 lookup miss 自动 add（lazy add 仅在握手成功路径触发）。【对应发现 #11】
- [ ] **MUST-5**：`std::panic::set_hook` 实现内只记 `location() + payload()` 字面；约定 implementer 写 panic / unwrap / expect 的 message **不得**含运行时变量插值（`format!("{:?}", x)`）；code-reviewer 在 PR 阶段检查。【对应发现 #10】

**feature 层（对应 feature ADR / implementer ADR 必须做）**

- [ ] **MUST-6**：`network/handlers/file.rs::handle_file` 入口先做 `seen_seq_and_update`，命中即返 200 静默丢弃；decrypt 前增加 base64 解码后字节长度校验 `≤ 5MB + 16B + base64 33% ≈ 6.7MB`；DefaultBodyLimit 收紧到 7MB。落实于 `file-transfer-drag` feature ADR。【对应发现 #1 / #6】
- [ ] **MUST-7**：handshake DoS 限流——每对 (remote_ip, device_id) 60s 内 ≤ 3；全局 60s 内 ≤ 10 个不同 device_id；超返 429（状态码表新增第 8 项）；body 不区分原因。落实于 `group-discovery` 或 `group-approval` feature ADR。【对应发现 #7】
- [ ] **MUST-8**：建立 `peer/sanitize.rs`（或合 `network/handlers/util.rs`）单点 sanitize 函数：`sanitize_device_name(s: &str) -> String`（≤ 64 codepoints + 字符集白名单 + Bidi/控制字符黑名单）；`sanitize_filename(s: &str) -> String`（v0 已有 + Win 保留名 + Bidi + 控制字符 + 200 字符限）；`sanitize_log_field(s: &str) -> String`（截短 + 上述黑名单）。所有 handler 接收外部字符串后**首动作**调 sanitize。落实于 `group-discovery` / `file-transfer-drag` / `diagnostic-logging` 三 feature ADR 共同引用。【对应发现 #8 / #9】

> **追加非必修但建议**（可在 feature ADR 中落实）：
> - 强制重连前校验 banned 状态【对应发现 #12】落实于 `peer-heartbeat` feature ADR
> - 日志 device_name / filename 截短 + sanitize（log/sanitize.rs）落实于 `diagnostic-logging` feature ADR

---

## 8. 后果（Consequences）

### 8.1 正面

- **加密路径密码学层防御深化**：AAD 绑值闭环（防跨 kind / origin / seq 重放）+ zeroize 密钥（防 dump 残留）→ _assumptions A23 "抓包看不到明文" 不变式有了真正的加固
- **协议层错误信息边界明确**：状态码 409 不可枚举本机 device_id + 403 三路径不可区分 + 422 通用串 → 攻击者外部探测信号变弱
- **ADR-003 第 7 节 10 项议题全部决议**：6 项必修（含 1 严重 + 5 中 + 4 中跟进）+ 3 项不必修（PSK / /ping origin / HMAC）+ 1 项延后（HMAC 全组 key 演进）；implementer 可直接落地
- **威胁模型显式落档**：3 类在场主体 + 5 类不在场边界明文 → 未来安全审阅基线
- **6 份 feature spec 第 7 节 [P0] [安全] 议题闭环**：e2e-encryption / clipboard-text-sync / clipboard-image-sync / file-transfer-drag / group-discovery / diagnostic-logging 直接引用本 ADR 第 7 节"必修清单"对应条目；feature ADR 不重复论证

### 8.2 负面 / 妥协

- **AAD 绑值后协议字段不可改**：未来若想改 `kind` / `origin_device_id` / `seq` 字段语义，需 supersede 本 ADR + bump v3
- **zeroize 增加 1 个依赖**：`zeroize = "1.8"` 是 RustCrypto 维护，无 CVE 历史；与 aes-gcm / x25519-dalek 同生态——可接受
- **DoS 限流可能误伤合法用户**：在某些场景下用户可能被自家网络误判（如频繁切 Wi-Fi 触发多次 handshake）；implementer 在 group-discovery feature ADR 给阈值具体细化时需考虑
- **PSK 不引入**：用户在主动 MITM 场景下仅靠"审批弹框看 device_name"识别——如未来用户实际遭遇 MITM 攻击，本 ADR 决议需 supersede
- **HMAC 不替代 SHA-256(plaintext)**：history-sync-delete 端点的"按 hash 探有无"信息泄露面**仍存在**；威胁主体限于"已握手 peer"——可接受但需在 history-sync-delete feature ADR 文档化
- **filename / device_name sanitize 在后端单点**：前端 / UI 层不再做 sanitize 等于把责任归集后端；若后端 sanitize 漏过会全链路传染——必须高质量单元测试覆盖

### 8.3 需要警惕的副作用

- **AAD 绑值后 e2e-encryption.md AC #6 单元测试需扩到 6 条**（增 1 条 AAD bind 测试）
- **zeroize 引入后 PeerRegistry.snapshot() 返 Vec<PeerState> 的 clone 路径**：Zeroizing<[u8;32]> Clone 会 clone 字节而非清零；这是预期，但 implementer 须确保 snapshot 返的 PeerState clone 不落盘 / 不写日志
- **DoS 限流的 PolicyState（per-pair / 全局计数器）也必须放入 PeerRegistry 或独立 RateLimiter**：增加 PeerRegistry 工程量；implementer 在 group-discovery feature ADR 决定单独 module 或并入 PeerRegistry
- **强制重连触发的 re_handshake 校验 banned**：health worker 增加对 PeerRegistry banned 集合的查询；若 banned 频繁变化需考虑读锁竞争——但 banned 集合写频率低（仅 ban/trust gossip），可接受

---

## 9. 实施提示（≤ 5 条）

1. **AAD 实现单点**：`crypto/aes_gcm.rs::AesGcmSealer::build_aad(kind, origin_device_id, seq) -> Vec<u8>` 单函数；encrypt / decrypt 内部调用；handler 调用方只传 (kind, origin, seq)
2. **zeroize 依赖加在 crypto 模块依赖范围而非全局**：`Cargo.toml` `zeroize = { version = "1.8", default-features = false, features = ["zeroize_derive"] }`；只 `peer/state.rs` 导入用
3. **DoS 限流模块**：`network/rate_limit.rs`（独立单文件）维护 `RwLock<HashMap<(IpAddr, String), VecDeque<Instant>>>` + 全局计数；handshake handler 第一行调；超阈值返 429
4. **sanitize 模块**：`peer/sanitize.rs` 三个函数 + 单元测试 ≥ 12 条覆盖（4 个 sanitize 函数 × 3 类输入 = path 穿越 / RTL / 长串）
5. **panic hook 注册位置**：在 `lifecycle.rs::Lifecycle::start` step 1 之前（即 main / lib.rs 注册 lifecycle 之前的最早入口处）；使用 `std::panic::set_hook(Box::new(|info| {...}))`；hook 内不依赖 Tauri runtime（runtime 可能已死）

---

## 10. 验证（How to Verify）

### 10.1 怎么证决策对

- **AAD 单元测试**：用同 key + 同 plaintext + 不同 aad 加密两次 → 输出 ciphertext 字节不相等；改 aad 一字节后 decrypt 返 Err
- **zeroize 验证**：人工检查 PeerState.aes_key 类型是 `Zeroizing<[u8;32]>`；CI lint check 该字段类型不被退化
- **/file seq dedupe 集成测试**：在 A 上 send_files 给 B → B 用户接受保存 → 攻击者抓包重放原报文 → B 不再弹第二次 file-pending（log 出现"replay seq dropped"）
- **状态码 409 → 403 转换**：单元测试调 handshake handler 给与本机 device_id 相同的 req → 返 403（不返 409）+ body = "forbidden" 通用串
- **handshake DoS 限流**：脚本以 1Hz 给本机 /handshake 发 5 个不同 device_id → 第 4 个起返 429
- **device_name / filename sanitize**：单测覆盖 RTL char / 8KB 超长 / 控制字符 / Win 保留名 各 1 条；正常输入保留不变

### 10.2 怎么证决策错（什么时候 supersede 本 ADR）

- **AAD 绑值后实战出现"feature 想改 kind 字段语义但 AAD 锁死"**：supersede 第 3.6 节（重新评估绑值组合）
- **zeroize 对实际场景无可观测收益**（用户从未遇到 dump 泄露）：可在 v3 移除依赖；但成本极低不会主动 supersede
- **DoS 限流误伤合法用户报告 ≥ 3 次**：阈值放宽或改为软限流；supersede 第 4.3 节
- **A1 主动 MITM 攻击实际发生**：强烈建议 supersede 第 3.7 节，引入 PSK / Noise Protocol
- **审计发现某 panic / unwrap 调用点 message 仍含变量插值**：说明 code-reviewer 流程不充分；supersede 第 6.1 节，转为强制 lint check（如 `clippy::panic` + 自定义 lint）

---

## 11. 决策卡片清单（v5-11 — 让用户回顾）

> 本 ADR 主要为"安全签字"性质，决议已在第 3-7 节给出。卡片用于让用户在阅读本 ADR 后**确认** 6 必修是否接受 + 3 不必修是否同意。

---

### 卡片 1 / 3 — AAD 绑值 + zeroize + DoS 限流（核心安全闭环）

**问题**：是否接受本 ADR 第 7.2 节 MUST-1 / MUST-2 / MUST-7 三条必修——AAD 绑 `b"sync-copy-v2" || kind || origin_device_id || seq` + zeroize 引入 + handshake DoS 限流？

**选项**：

- **A 全接受（推荐）**：项目层基础设施 PR 落实 MUST-1 / MUST-2，feature 层 PR 落实 MUST-7
- B 仅接受 MUST-1 / MUST-2，DoS 限流延后到 v2.1：减少 v2.0 复杂度但 v2.0 发布期间存在弹框淹没攻击面
- C 全延后到 v2.1：v2.0 仅做项目层骨架，安全加固独立批次——与 ADR-001 v2 重写"决策强制落盘"相悖

**取舍**：A 增 ~80 行 Rust + 1 个 zeroize 依赖；换来 _assumptions A23"抓包看不到明文"+"不被弹框淹没"两条不变式真正闭环

**不做后果**：v2.0 发布后弹框淹没 / dump 泄露 / 跨 kind 重放在 implementer 误用时直接打到用户

**must-fix**：MUST-1 / MUST-2 / MUST-7 是 implementer 落地前的硬阻塞条件；不接受 = 主窗口需走 ADR 修订流程否则不能进 IMPL_IN_PROGRESS

---

### 卡片 2 / 3 — 错误信息边界 + 协议层加固（4 中危发现）

**问题**：是否接受本 ADR 第 7.2 节 MUST-3 / MUST-4 / MUST-5 / MUST-6 / MUST-8 五条——状态码 409→403 通用 body / PeerRegistry.remove 原子顺序 / panic message 约定 / /file seq dedupe + size 双校验 / sanitize 模块？

**选项**：

- **A 全接受（推荐）**：implementer 在基础设施 PR + 三个 feature ADR（file-transfer-drag / group-discovery / diagnostic-logging）阶段落实
- B 接受协议层（MUST-3 / MUST-4 / MUST-5）+ 延后 sanitize 模块（MUST-8）到 feature 层 ADR 时再决定：风险是 sanitize 不在项目层 = 每个 feature 自己写 sanitize → v0 教训重现（"边写边定"）
- C 全部接受但严重发现 #1（/file seq dedupe）单独优先级 P0：与 A 等价，仅排序

**取舍**：A 增 ~150 行 Rust（含 rate_limit + sanitize 模块）+ 4 处 handler 改造；换来对外协议面"不可枚举 / 不可信息泄露 / 不可重放 / 不可 DoS"四闭环

**不做后果**：严重发现 #1 在 v2.0 发布即成为已知漏洞；中危发现 #4 #5 让 LAN 攻击者能枚举本机 device_id

**must-fix**：MUST-3 / MUST-4 / MUST-5 在项目层基础设施 PR；MUST-6 / MUST-8 在 feature ADR；缺一不可进 RELEASED

---

### 卡片 3 / 3 — 不必修议题确认（PSK / /ping origin / HMAC）

**问题**：是否接受本 ADR 第 7.1 节"低危发现"——v2.0 不引入 PSK / 不加 /ping origin 校验 / content_hash 暂保 SHA-256（不切 HMAC）？

**选项**：

- **A 全接受（推荐）**：v2.0 范围聚焦 v0 实战 bug + 核心闭环；3 项议题留 ADR-N supersede（如真实威胁出现）
- B PSK 在 v2.1 引入：增 UI 改造 + 用户输入流程；产品定位"不需要管理密码"逆转
- C HMAC 切到全组 epoch key 模型：v2.x 大改造（与 group-trust-gossip 联动），超 v2.0 范围

**取舍**：A 接受当前威胁模型边界（社会工程层防御 + LAN 端口扫描已暴露服务）；换来 v2.0 可发布

**不做后果**：A1 主动 MITM 攻击实际发生时本 ADR 需 supersede（如 6 个月内观察到 ≥ 1 次 MITM 投诉则升级 v2.x ADR-N 引入 PSK）

**must-fix**：本卡片不接受 = 主窗口需让 PM 在对应 spec 第 3 节 in scope 加新功能 + 新 ADR；改 v2.0 范围

---

> 3 张卡片接受后：本 ADR status PROPOSED → ACCEPTED；ADR-003 第 7 节追加引用行；P2-1.b 进入 feature ADR 分批阶段。本 ADR v1 直接以 ACCEPTED 状态落盘（用户已多次确认走"决策强制落盘"流程；本 ADR 是 ADR-003 第 7 节占位的接管 ADR，不需要重新走 PROPOSED → ACCEPTED 流程）。

---

## 12. 安全审阅自查（v2 7-section 全部段）

### 12.1 结论

**CHANGES_REQUESTED**（项目层方向 APPROVED；6 必修在 implementer 落地前补齐）。

ADR-003 第 3.4 节（加密层抽象）/ 第 3.6 节（错误日志总策略）/ 第 3.7 节（隐形掉线机制）整体方向 APPROVED——trait 化加密 / boundary enum 错误层 / 4 件套兜底 三个结构性决议无安全反对意见；用户决策卡片全选 B 不影响安全结论。8 必修是对决议方向的**实施层加固**，不否决方向。

### 12.2 威胁模型

3 类在场主体（同 LAN 恶意设备 / 网络监听者 / 已踢除但 IP 仍可达的旧 peer）+ 5 类不在场边界（本地物理访问 / 供应链 / 用户主动泄密 / 后量子 / 侧信道）。

### 12.3 关键发现数

- [严重] 1 条（/file 缺 seq dedupe）
- [中] 11 条（含 AAD / zeroize / 状态码 409→403 / 状态码 403 通用 body / 文件 size 双校验 / DoS 限流 / device_name / filename / panic message / Registry-pool 钩子顺序 / re-handshake banned 校验）
- [低] 3 条（PSK / /ping origin / HMAC 全组 key）

### 12.4 必修清单

8 条（本 ADR 第 7.2 节）：MUST-1 ~ MUST-8。其中 MUST-1 / MUST-2 / MUST-3 / MUST-4 / MUST-5 在项目层基础设施 PR 阶段落实；MUST-6 / MUST-7 / MUST-8 在对应 feature ADR 阶段落实。

### 12.5 过度工程自查

- 未引入 noise / snow / TLS / certmgr / PKI / Vault 等过度工程依赖
- 未要求"运行时 KMS 服务"或"密钥轮换 daemon"（密钥仍纯内存）
- zeroize 引入是单类型字段改造 + 1 个 RustCrypto 生态轻量依赖；非过度
- DoS 限流是单文件模块 + RwLock 数据结构；非过度
- 未提案"全组 epoch key + HMAC 跨设备删除"——明确**延后**（ADR-008 第 4.7 节决议）；避开 v2.0 范围爆炸

### 12.6 owner 边界自查

- security-reviewer 只在 `decisions/ADR-008-security-review-of-adr003.md` 写新文件
- ADR-003 第 7 节追加一行引用——属于 ADR-003 第 7 节"占位"段的接管语义，是 security-reviewer 通过 ADR-008 行使的合法权力（ADR-003 frontmatter `revision_history` 允许在 ACCEPTED_PENDING_SECURITY_SIGNOFF 后由 security-reviewer 收口第 7 节）
- 未改 ADR-003 第 1-6 节（架构师域）
- 未改 src-tauri/** 或 src/** 业务源码
- 未改任何 spec 第 1-7 节
- 未改 PLAN.md（v2-9 subagent 不写）
- 未调其它 agent
- 未用 Section sign 符号（U+00A7）

### 12.7 建议主窗口下一步

- ACCEPTED → 主窗口在 ADR-003 第 7 节追加一行 `> 已由 ADR-008 接管，本节不再扩展。`
- 主窗口推进 ADR-003 status `ACCEPTED_PENDING_SECURITY_SIGNOFF` → `ACCEPTED`（条件已满足）
- 主窗口在 PLAN.md 把 P2-1.a status 推到 ADR_ACCEPTED；P2-1.b 进入"feature 层 ADR 分批"——建议第一批选三块基础设施（PeerRegistry / Lifecycle / crypto traits）+ 把本 ADR MUST-1 ~ MUST-5 列入第一批基础设施 PR 的 must-fix
- 主窗口在 `docs/handoff-lessons-learned.md` 第 9 段记账"ADR-008 ACCEPTED + ADR-003 ACCEPTED + 8 必修入项目级跟踪"
- 不必跑 retrospective（决议方向 APPROVED + 必修清单清晰）

