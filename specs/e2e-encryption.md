---
status: SPEC_REVIEWED
owner: product-strategist
related_adrs: [ADR-003, ADR-008, ADR-011]
related_specs: [00-product-overview, group-discovery]
created: 2026-05-06
updated: 2026-05-08
revised: 2026-05-08 — ADR-003 第 3.4 节 锁定 KeyExchange / Sealer / Verifier trait 边界 + HKDF v2 bump (salt=sync-copy-v2-salt / info=sync-copy-v2:aes-256-gcm) + AAD/zeroize/PSK 留 ADR-008 安全审阅决议
priority: P0
---

# e2e-encryption — X25519 ECDH 临时密钥协商 + AES-256-GCM 报文加密

## 1. 问题（为什么做）

Sync Copy 的差异化定位之一是"**内容不出局域网 + 端到端加密**"——同 LAN 即使有人抓包，看到的只是密文。这条承诺写在 `使用说明.md` 顶部，是用户选择本工具而非"局域网 HTTP 文件分享脚本"的关键卖点。同时，加密 + "握手时审批"的组合等于把"身份认证"从"密码"改成"人在场决定"——这套范式 v0 已实战验证（00 总览 第 5.1 节）。本 feature 定义所有报文加密的密码学栈、密钥生命周期、与协议字段对接，是 P0 必须随握手第一刀就上线的。

## 2. 用户故事

- As a privacy-conscious user, I want all clipboard / file content between my devices to be encrypted in transit using ephemeral keys, so that even a malicious LAN neighbor or compromised router only sees ciphertext.
- As a user, I do not want to manage keys / passwords / certificates—the encryption should be invisible to me, with security backed by the human approval gate at handshake time.
- As an attacker on the same LAN, capturing a previous session's traffic should be useless (forward secrecy)—each session uses a new key derived from one-time ephemeral keypairs.

## 3. 范围

**in scope**：
- **密钥协商**：每次握手双方各自生成临时 X25519 密钥对（`EphemeralSecret` + `PublicKey`），交换公钥后做 Diffie-Hellman 算 32 字节共享秘密，HKDF-SHA256 派生为 AES-256 密钥
  - HKDF salt = `b"sync-copy-v1-salt"`
  - HKDF info = `b"sync-copy-v1:aes-256-gcm"`
- **每对 peer 一把密钥**：`peer_keys: HashMap<device_id, [u8; 32]>`，互不共用
- **密钥仅内存**：`Arc<RwLock<HashMap<...>>>`，进程退出即丢；下次启动重新协商（永不持久化）
- **报文加密**：所有非握手报文的 payload 字段（`/clipboard`、`/file`、未来 `/delete_history` 的内容字段）走 AES-256-GCM
  - 每条消息独立 12 字节随机 nonce（`OsRng`）
  - 密文（含 16 字节 GCM tag）+ nonce 各自 base64 编码后放进 JSON `nonce` / `ciphertext` 字段
  - **AAD 暂定不绑** (`aad: &[]`)，理由：协议字段改动成本 + Phase 2 待 ADR 论证；具体决定见 第 7 节 [P0] [安全]
- **握手报文不加密**：仅 X25519 公钥（32 字节，base64）+ device_id + device_name + listen_port，无机密信息
- **API**：`crypto.rs` 模块导出 `new_ephemeral()`, `pubkey_to_b64`, `pubkey_from_b64`, `derive_aes_key`, `encrypt`, `decrypt`
- **`device_id` re-handshake**：已知 peer 重新握手时直接覆盖密钥（不需要"重协商协议"，握手本身就是协商）

**out of scope**（v2 这个 feature 不做）：
- 长期密钥 / 持久化密钥环 / 密码保护的密钥库（违反"不需要密钥管理心智"的设计原则）
- 预共享密钥（PSK）防主动 MITM（由 第 7 节 安全风险条目讨论是否后续加）
- AAD 绑定上下文是否落地（如 device_id / seq 绑入 AAD 防 replay across peers）—— 待 ADR 决定，见 第 7 节 [P0] [安全]；本 spec v0.1 不在 in scope 锁死
- 后量子密钥协商（X25519 不抗量子）
- 证书 / PKI / TLS（HTTP body 自加密，不走 TLS）
- 完美前向保密的额外协议层（X25519 ephemeral 已自带前向保密；不再叠加 noise / TLS 1.3）

## 4. 验收标准（Definition of Done）

- [ ] A、B 两台机器走完一次握手后，`peer_keys[A.device_id]` 在 B 上、`peer_keys[B.device_id]` 在 A 上各持有 32 字节密钥，且两边的密钥**字节相等**
- [ ] 在 A 上发送一条文本剪切板，B 抓包能看到 `/clipboard` 请求体里 `ciphertext` 字段是 base64 不可读字符串，明文文本不出现
- [ ] 在 B 上修改 `peer_keys[A.device_id]` 的任一字节（人为破坏），A 下一次发文本到 B 时 B 收到后 decrypt 失败，报错 `解密失败：密钥不一致或消息被篡改`，不写入剪切板
- [ ] 重启 A，A 与 B 重新握手 → 双方各自生成新的 X25519 `EphemeralSecret` 与新随机数，派生出的 AES key 与之前会话的 AES key 在字节层独立（即旧会话密钥泄露不影响新会话解密；前向保密的可验证体现 = 新旧 key 字节不相同 + 派生输入与之前会话不重叠）
- [ ] 抓包重放一条之前成功的 `/clipboard` 请求到目标机器，因 `seq` 去重逻辑被 200 OK 静默丢弃（`group-discovery` 里的 seq dedupe 保证；本 spec 不重复定义但引用）
- [ ] X25519 公钥 base64 长度始终为 44 字符（32 字节）；nonce base64 长度始终为 16 字符（12 字节）
- [ ] 单元测试覆盖 ≥ 5 条：(1) encrypt/decrypt round-trip 明文一致；(2) 错误密钥导致 decrypt 失败；(3) 错误 nonce 长度返回错误；(4) HKDF 派生确定性（同 shared_secret + 同 salt + 同 info → 同 32 字节 key）；(5) 跨 peer 密钥不互通（A↔B 派生的 key 与 A↔C 派生的 key 在字节层不相等）

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的
`legacy-prototype` 分支 `src-tauri/src/crypto.rs`（75 行，最干净的模块之一）：`new_ephemeral()` 返 `(EphemeralSecret, PublicKey)`，`derive_aes_key(secret, their_pub)` 走 `secret.diffie_hellman(their_pub)` + `Hkdf::<Sha256>::new(Some(SALT), shared.as_bytes()).expand(INFO, &mut key32)`，`encrypt(key, plaintext)` 用 `OsRng.fill_bytes(&mut nonce[12])` 构 nonce，`Aes256Gcm::new(key).encrypt(nonce, Payload{msg, aad:&[]})` 返 `(nonce_b64, ct_b64)`。`Cargo.toml`：`x25519-dalek = "2", aes-gcm = "0.10", hkdf = "0.12", sha2 = "0.10", base64 = "0.22", rand = "0.8"`。`protocol.rs` 的 `ClipboardReq.nonce / ciphertext` 都是 `String`（base64）。

### 5.2 v0 暴露的具体坑
- HKDF `salt` 与 `info` 是字符串字面量埋在 crypto.rs 里，没有 ADR 论证选值理由（只是版本号 `v1`）；将来 v2 协议不兼容时如何 bump 没规划
- AES-GCM 的 AAD 全空 → 没有把上下文（如 origin_device_id / seq / kind）绑定进 AAD，理论上一条加密报文可以被重放成"另一种 kind"或"另一对 origin/dest"。v0 靠 `seq` dedupe 缓解 replay，但**不是密码学层防御**
- 密钥派生存到 `peer_keys: Arc<RwLock<HashMap<String, [u8;32]>>>`，重新握手覆盖；没有 zeroize（密钥从 HashMap 移除时旧字节仍在内存某处）
- v0 没有任何单元测试覆盖加密路径——所有验证都靠"两台机器手测"
- 演进史：M3 用密码 → M4 早期想 PBKDF2 → M4 最终 X25519+HKDF+AES-GCM，**否决路径无 ADR 记录**，只在 commit message 留只言片语（00 总览 第 5.2.3 节 已点名这是 v2 必须改的）
- `derive_aes_key` 接收 `EphemeralSecret`（消费所有权）→ 调用方必须先持有再传入，与"先调 `new_ephemeral` 立刻派生" 紧耦合；测试时不便分步

### 5.3 v2 应继承
- X25519 临时密钥对（`EphemeralSecret` 类型，每次握手新生成）
- HKDF-SHA256 派生 AES-256 密钥
- AES-256-GCM + 12 字节随机 nonce + 内置 16 字节 tag
- 密钥仅内存（`Arc<RwLock<HashMap<...>>>`），永不持久化
- 每对 peer 一把独立密钥
- HKDF salt/info 字符串语义（v2 可能 bump 为 `v2`，但风格不变）
- crypto.rs 模块独立、保持 ≤ 100 行

### 5.4 v2 应挑战
- **AAD 绑定**：把 `origin_device_id || seq || kind` 拼成 AAD 传入 AES-GCM，防止跨 peer / 跨 kind 重放（密码学层而非应用层防御）。是否做需架构师 + 安全在 ADR 论证
- **Zeroize 密钥**：用 `zeroize` crate 在密钥移除 / 进程退出时清零内存；v0 没做
- **PSK 防主动 MITM**：同 LAN 攻击者可主动 MITM 握手，因为公钥未加密（仅靠"审批弹框看到的设备名"识别正确性，这是社会工程层防御）。v2 是否提供可选 PSK（如 6 位口令）作为额外认证？
- **HKDF salt/info 版本化**：写到一个常量 + 协议版本号字段，未来不兼容时强制双方同 v 才协商
- **协议 version negotiation**：握手 req 里加 `protocol_version` 字段，v1/v2 不兼容时直接拒绝
- **加密的不变式**：例如"接收端必须先验证 origin_device_id 在 peers 表 → 再取 peer_keys[origin] → 再 decrypt"——这条不变式必须在 ADR 里明文写，所有非握手 handler 不能跳过

## 6. UX 段（占位）

> 本 feature 是纯密码学后端模块，不直接产生用户可见 UI。但 第 6 节 仍保留以让 ux-designer 决定：
> - 是否在浮窗某处（如设置面板）展示一个"已加密"的小图标 / 提示，以让用户感知"这条传输是加密的"
> - 解密失败时是否给用户错误消息（v0 只 log 不显示——某些场景应让用户知道，如显式 ban 一个 peer 后老消息进来失败）
>
> 未确定前 第 6 节 N/A。

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 4 条] [P1 2 条] [P2 0 条]

- [P0] [安全] AES-GCM AAD 当前为空（第 3 节 暂定不绑）：是否把 `origin_device_id || seq || kind` 绑入 AAD？trade-off：协议改字段成本 vs 重放攻击防护提升。决议直接修改 第 3 节 in scope
- [P0] [安全] 握手公钥不加密 → 主动 MITM 攻击者可截获并替换公钥让两端各自与攻击者协商。在 LAN 信任假设下尚可，但是否考虑 PSK / 短口令认证作为 v2 增强？
- [P0] [安全] 密钥 zeroize：是否引入 `zeroize` crate？密钥从 HashMap 移除时是否主动清零？进程退出 panic 时密钥可能落在 core dump 里
- [P0] [安全] re-handshake 覆盖旧密钥：旧密钥的内存字节是否清零？还是直接 drop（HashMap value 替换不保证 zeroize）？
- [P1] [架构师] HKDF salt/info 是否包含协议版本号显式字段？未来协议升级如何不踩兼容性坑
- [P1] [架构师] `derive_aes_key` 函数签名（消费 EphemeralSecret）是否改为更易测试的 trait 抽象？

## 8. Review 段（占位）

> code-reviewer / tech-architect / security-reviewer 后续填写。本 feature 必须经 security-reviewer 显式 ACK 才能 ADR_ACCEPTED（CLAUDE.md 第 9 节）。
