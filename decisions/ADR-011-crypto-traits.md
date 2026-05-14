---
id: ADR-011
feature_id: crypto-traits
title: Crypto trait 拆分 / AesGcmSealer AAD 绑值实现 / HKDF salt v2 bump / zeroize 应用边界 / 单元测试清单
status: ACCEPTED
owner: tech-architect
date: 2026-05-09
accepted_at: 2026-05-09
security_signoff: ADR-011 第 7 节追加签字 APPROVED 0 必修补丁（2026-05-09，sec 一次过；项目最关键加密 ADR）
deciders: [tech-architect, main, user, security-reviewer]
user_decision_summary: 2/2 决策卡片均为技术实现细节（trait 拆分 / AAD 入参形态），按 lessons-learned 第 5 段第 10 条新策略主窗口直接采纳推荐 1B / 2B（不上报用户）；user 通过 2026-05-09 总反馈"决策疲劳要求降低技术细节卡片"代理授权。卡 1 选 B（保留 KeyExchange + Sealer 2 trait，Verifier 降级注释占位）；卡 2 选 B（build_aad 集中函数 + AadKind enum 9 值）
related_specs:
  - e2e-encryption
  - clipboard-text-sync
  - clipboard-image-sync
  - file-transfer-drag
related_adrs:
  - ADR-003
  - ADR-008
  - ADR-009
supersedes: []
superseded_by: []
revision_history:
  - version: v1
    date: 2026-05-09
    notes: 初版 — P2-1.b 第一批第三份（基础设施收尾）。把 ADR-003 第 3.4 节方向 + ADR-008 MUST-1（AAD 绑值）+ MUST-2（zeroize 边界）落到 trait 接口签名 + AAD 拼装契约 + HKDF v2 bump + zeroize 应用位置 + 单元测试清单层面。算法选型（X25519+HKDF+AES-GCM）不再重论证（ADR-003 已锁）；仅就 trait 拆分粒度 / AAD 入参形态两子点列选项
  - version: v1.1
    date: 2026-05-09
    notes: 主窗口按 lessons-learned 第 5 段第 10 条新策略，2 张技术细节卡片直接采纳推荐 1B / 2B（不上报用户）；status PROPOSED → ACCEPTED_PENDING_SECURITY_SIGNOFF；deciders 加 [main, user]；待 sec 在第 7 节追加签字段
  - version: v1.2
    date: 2026-05-09
    notes: sec 第 7 节签字 APPROVED 0 必修补丁（项目最关键加密 ADR 一次过）；status ACCEPTED_PENDING_SECURITY_SIGNOFF → ACCEPTED；deciders 加 [security-reviewer]；P2-1.b 第一批基础设施三件套（009/010/011）全部 ACCEPTED，可启动实现阶段
depends_on_artifacts:
  - path: decisions/ADR-003-project-architecture-skeleton.md
    version: ACCEPTED 2026-05-08（第 3.4 节 trait 化方向 + HKDF v2 bump 卡片 4 must-fix + 第 4.3 节 AAD/zeroize/PSK 留 ADR-008）
  - path: decisions/ADR-008-security-review-of-adr003.md
    version: ACCEPTED 2026-05-08（第 3.5 节 zeroize 引入 + 第 3.6 节 AAD 绑值规范 + 第 7.2 节 MUST-1 / MUST-2）
  - path: decisions/ADR-009-peer-registry.md
    version: ACCEPTED 2026-05-09（第 3.1 节 PeerState.aes_key 类型 + 字段填值时机表）
  - path: specs/e2e-encryption.md
    version: SPEC_REVIEWED 2026-05-08（第 4 节 AC 6 条单测 + 第 7 节 [P0/P1] 4 安全 / 2 架构议题）
  - path: specs/clipboard-text-sync.md
    version: SPEC_REVIEWED 2026-05-08（kind="text"）
  - path: specs/clipboard-image-sync.md
    version: SPEC_REVIEWED 2026-05-08（kind="image_png"）
  - path: specs/file-transfer-drag.md
    version: SPEC_REVIEWED 2026-05-08（kind="file"）
---

# ADR-011 — Crypto trait 拆分 / AAD 绑值实现 / HKDF v2 bump / zeroize 应用边界

> 范围：把 ADR-003 第 3.4 节"trait 化（KeyExchange + Sealer + Verifier 占位）+ 默认实现（X25519 + AES-GCM）"决议落到**可签编 trait / impl 形态 + AAD 拼装契约 + HKDF salt/info 字面量 + zeroize 应用位置表 + 单元测试清单**。本 ADR 不重新论证算法选型（X25519+HKDF+AES-GCM 在 ADR-003 已锁，ADR-008 第 3.1 节评级 APPROVED），仅就两个仍有候选的子点（trait 拆分粒度 / AAD 入参形态）列选项；其余子节是 ADR-003+008 已决方向的细化，无可选项直接进决定段。

---

## 1. 上下文（Context）

### 1.1 触发本 ADR 的输入

- **ADR-003 第 3.4 节** 已选 选项 B（trait 化）：列出 `KeyExchange` / `Sealer` / `Verifier` 三 trait 草案 + 默认实现 module 路径 + 密钥生命周期表，但**未细化**：(a) 三 trait 是否真的全保留（Verifier 在 v2 PSK 已被 ADR-008 否决场景下是否仍出 trait）；(b) Sealer::encrypt 的 aad 参数类型（`&[u8]` / `&dyn AssociatedData` / 结构体 builder）；(c) HKDF v2 bump 字面量虽 卡片 4 must-fix 已写，但调用点 + module 边界未落；(d) zeroize 在 trait 实现内部哪些中间值需要清零（与 x25519-dalek 自带 zeroize / ADR-009 PeerState.aes_key 的边界划分）。
- **ADR-008 第 7.2 节 MUST-1**（强制）：`AesGcmSealer::encrypt/decrypt` 实现层 AAD 绑值 = `b"sync-copy-v2" || kind.as_bytes() || origin_device_id.as_bytes() || seq.to_be_bytes()`；trait Sealer 签名保持 ADR-003 第 3.4 节预留入参；调用方传值。本 ADR 把"哪个调用方在哪一行传哪个 kind"落到调用点契约表。
- **ADR-008 第 7.2 节 MUST-2**（强制）：`zeroize = "1.8"` 已落 ADR-009 PeerState.aes_key（`Zeroizing<[u8; 32]>`）；本 ADR 确认 trait 实现内部不残留中间值（特别是 `derive_aes_key` 内的 HKDF expand 临时缓冲、`encrypt` 内的 AAD Vec、`decrypt` 内的 plaintext Vec 的 zeroize 边界）。
- **ADR-009 第 3.1 节** 已锁 `PeerState.aes_key: Zeroizing<[u8; 32]>` + 字段填值时机表（握手成功最后一步 `insert()` 写入；`PeerRegistry::remove` Drop 自动清零）。本 ADR 是 trait `KeyExchange::derive_aes_key` 输出落地到 PeerState 的"接口对接面"。
- **e2e-encryption.md 第 4 节 AC #6**：单元测试覆盖 ≥ 5 条；**ADR-008 实施提示** 又追加 1 条"改 aad 任一字节后 decrypt 失败"。本 ADR 把测试清单细化到 ≥ 6 条 + 每条断言点 + mock 边界。
- **e2e-encryption.md 第 7 节 [P1] [架构师]**：`derive_aes_key` 函数签名（消费 EphemeralSecret）是否改为更易测试的 trait 抽象 — 本 ADR 答：是，在 KeyExchange trait 关联类型 Secret 上把消费语义保留（diffie_hellman 必须消费 EphemeralSecret 是 x25519-dalek 库 API 强约束，不可绕开），但通过 trait 边界让 mock impl 可在测试用具体可控的 `Secret = TestSecret`（输入对称的可预测 32 字节）注入。
- **e2e-encryption.md 第 7 节 [P1] [架构师]**：HKDF salt/info 是否包含协议版本号显式字段 — 本 ADR 答：是，在 module 顶部用两个 `pub const` 落地，字面量 `b"sync-copy-v2-salt"` / `b"sync-copy-v2:aes-256-gcm"`（ADR-003 卡片 4 must-fix 已锁）；future ADR-N supersede 本 ADR 时 bump v3。

### 1.2 v0 函数式 6 函数 + 0 单测的反面教材（仅引文件路径）

`legacy-prototype:src-tauri/src/crypto.rs` 暴露 6 个自由函数 `new_ephemeral / pubkey_to_b64 / pubkey_from_b64 / derive_aes_key / encrypt / decrypt`；其中 `encrypt` / `decrypt` 内部 `aad: &[]` 写死、`derive_aes_key` 直接消费 `EphemeralSecret` 所有权（与 x25519-dalek API 一致但**未在 trait 层抽象**）→ 调用方与 crypto 模块紧耦合。`Cargo.toml` 未引 zeroize；`peer_keys: Arc<RwLock<HashMap<String, [u8;32]>>>` 移除 / 覆盖时旧字节遗留内存。**全文件 0 单测**——所有验证靠"两台机器手测"。e2e-encryption.md 第 5.2 节、ADR-003 第 3.4 节选项 A、ADR-008 第 3.6 节都点名这是 v2 必须改的项。本 ADR 是该教训的具体修复路径。

### 1.3 现在不决的后果

- 后续任一 feature ADR（clipboard-text-sync / clipboard-image-sync / file-transfer-drag handler 落地）都要重新论证 AAD 拼装格式、kind 字面量取值、Vec 拼接顺序，不一致风险高（一端 big-endian 一端 little-endian → decrypt 全失败）。
- ADR-008 MUST-1 没有"落到 trait 签名 + 调用契约"的 ADR 兜底 → implementer 自由发挥（如把 AAD 拼装写在 handler 里而非 Sealer 内）→ code-reviewer 没参照系审查。
- HKDF v2 bump 字面量在 ADR-003 卡片 4 must-fix 写了但未到 module 边界，implementer 可能分散到 crypto/x25519.rs 与 crypto/aes_gcm.rs 两处定义而错位，造成派生密钥两端不一致的隐形 bug。
- 单测清单未细化 → e2e-encryption.md AC #6 "≥ 5 条单测" 在 implementer 阶段被"补 1 条 round-trip 就交差"敷衍掉，AAD 改字节、跨 origin、跨 seq、zeroize 落地等关键防线无验证。

---

## 2. 选项考虑（Options Considered）

> ADR-003 第 3.4 节已锁定算法选型方向（X25519+HKDF+AES-GCM）+ trait 化方向；ADR-008 已锁 AAD 绑值具体字节组成 + zeroize 引入。本 ADR 仅就两个仍有候选的子点列选项：(a) **trait 拆分粒度**（保留 3 trait 还是合并）；(b) **AAD 入参形态**（裸 `&[u8]` 还是结构体 builder）。其余子节直接进第 3 节决定段。

### 2.1 Trait 拆分粒度

#### 选项 A：保留 3 trait（KeyExchange / Sealer / Verifier）— ADR-003 第 3.4 节草案

- 怎么做：与 ADR-003 第 3.4 节伪代码一致。`KeyExchange`（new_ephemeral / pubkey_to_b64 / pubkey_from_b64 / derive_aes_key）、`Sealer`（encrypt / decrypt）、`Verifier`（verify_origin 占位）三 trait 各占一文件 module。
- 优点：职责切分明确（密钥协商 / 报文加密 / 身份验证三件事各管一摊）；future PSK 引入只改 Verifier impl 不动 KeyExchange / Sealer；e2e-encryption spec 第 7.4 节"trait 边界让 future PSK 不破坏调用点"承诺已写。
- 缺点：`Verifier::verify_origin` 在 ADR-008 否决 PSK 后**v2 实质无 impl**——只剩 trait 与一个空 stub 实现 `NoopVerifier`。新增 trait 但无业务意义 = YAGNI 反模式（CLAUDE.md v5-1）。
- 实现复杂度：低
- 跨平台风险：无

#### 选项 B：保留 2 trait（KeyExchange / Sealer），Verifier 不出 trait 仅留 module 注释占位

- 怎么做：`crypto/mod.rs` 仅 `pub trait KeyExchange` + `pub trait Sealer`；`crypto/verifier.rs` 不存在（不创建空 module）；ADR-003 第 3.4 节 Verifier trait 占位**降级为 module 顶部一段 `// FUTURE: PSK / HMAC challenge will introduce a Verifier trait here. ADR-008 第 3.7 节 PSK 否决 v2 不引入。`** 注释。Future ADR-N 引入 PSK 时再正式定义 trait，supersede 本 ADR 第 3.1 节即可。
- 优点：避免 v2 出现 0-impl trait（YAGNI 闭环）；trait 边界仍守住 future PSK 演进路径（注释 + ADR-008 第 3.7 节决议双锚点）；新增依赖 / 测试矩阵更小（少 1 个 mock NoopVerifier）；与 ADR-009 第 3.1 节 TrustState::Pending 同手法（保留枚举值兼容未来 PSK 但不引入 trait 抽象）。
- 缺点：future PSK 引入时需新增 1 个 trait + 改 4 个 handler 调用点（handshake / clipboard / file / approval）。但同样 4 个调用点本身就要因 PSK 字段加而改，trait 新增成本可忽略。
- 实现复杂度：低
- 跨平台风险：无

#### 选项 C：合并为 1 trait `Cryptosuite`（KeyExchange + Sealer 方法都塞进同一 trait）

- 怎么做：一个 `pub trait Cryptosuite` 含 6 个方法（new_ephemeral / pubkey_to_b64 / pubkey_from_b64 / derive_aes_key / encrypt / decrypt）；默认实现 `pub struct Suite;` 一个 struct 带所有方法。
- 优点：调用方 import 1 个 trait 即可
- 缺点：违反 SRP（Single Responsibility）—"密钥协商" 与 "报文加密" 是两件事；future 切到 Noise Protocol 时（ADR-003 第 3.4 节选项 C 已否决但留 supersede 路径）会被迫改全 trait；mock 时无法只 mock encrypt 不 mock new_ephemeral；test #4 (HKDF 派生确定性) 与 test #2 (encrypt round-trip) 共用 mock 接口，注入边界混乱
- 实现复杂度：低
- 跨平台风险：无
- 否决理由：trait 边界塌缩到 god trait；与 ADR-003 第 3.4 节选项 B 草案精神冲突

### 2.2 AAD 入参形态（Sealer::encrypt 的 aad 参数类型）

> 背景：ADR-008 第 3.6 节 / 第 7.2 节 MUST-1 锁定 AAD 绑值组成 = `b"sync-copy-v2" || kind || origin_device_id || seq.to_be_bytes()`；但**入参形态**（trait 签名上 aad 的 Rust 类型）未锁。两个候选：

#### 选项 A：`&[u8]` 裸字节（ADR-003 第 3.4 节草案 + ADR-008 第 3.6 节实施提示原文）

- 怎么做：`fn encrypt(&self, key: &[u8;32], plaintext: &[u8], aad: &[u8]) -> Result<(Nonce, Ciphertext)>;` 调用方在 handler 内自行拼装 4 段 → `Vec<u8>` → 传 `&aad`。
- 优点：trait 签名极简；与 aes-gcm crate `Payload { msg, aad }` 字段一致（零包装）；mock 实现写起来最少代码；测试时构 `&[]` 或自定义字节均无负担
- 缺点：拼装顺序 / 字节序 / `kind` 字面量取值靠**调用方**遵守 ADR-008 规范；handler 散在 3 个文件（`network/handlers/clipboard.rs` text/image / `network/handlers/file.rs`）→ 每处都要写一次拼装代码；implementer 误写"先 seq 后 origin"或"u64 用 to_le_bytes"都不会被编译器抓住，单测也只能在端到端失败时反推
- 实现复杂度：低（trait 简单），但**调用方**复杂度从低升到中（拼装散点 + 容易错）
- 跨平台风险：无

#### 选项 B：在 crypto module 内部提供 `pub fn build_aad(kind: AadKind, origin_device_id: &str, seq: u64) -> Vec<u8>`，trait 签名仍收 `aad: &[u8]`

- 怎么做：trait `Sealer::encrypt` 签名同选项 A（保持灵活；future PSK 仍可传不同 aad）；但在 `crypto/mod.rs` 顶部暴露一个 `build_aad` 自由函数（**不是** trait 方法），调用方 handler 必须用 `let aad = build_aad(AadKind::Text, origin_device_id, seq); sealer.encrypt(&key, plaintext, &aad)?` 这条路径。`AadKind` 是封闭 enum：`Text / ImagePng / File / Trust / Ban / Leave / DeleteHistory / ClearHistory / Approval`（与 ADR-009 第 3.1 节 last_seen_seq_by_kind 的 9 个 kind 字面量一一对应）。`build_aad` 内部按 ADR-008 规范拼接（`b"sync-copy-v2" || kind.as_bytes() || origin_device_id.as_bytes() || seq.to_be_bytes()`）。
- 优点：拼装顺序 / 字节序 / kind 字面量集中在一个函数 + 一个 enum，**编译器**保证 kind 取值正确（拼写错 `text` vs `Text` 不通过）；handler 调用面收敛到一行 + 一个 Sealer.encrypt 调用；单元测试只测 build_aad 即覆盖所有 kind 拼装的正确性（一处覆盖 9 个 kind）；future bump v3 改 build_aad 内部即可不动调用点；与 ADR-009 第 3.1 节 last_seen_seq_by_kind 的 enum 化语义一致（kind 集中归一）
- 缺点：crypto module 多 1 个函数 + 1 个 enum（< 30 行 Rust）；trait 签名仍收 `&[u8]` 留弹性（future PSK / Noise 不同 aad 形态可绕开 build_aad 直接传字节）
- 实现复杂度：低
- 跨平台风险：无

#### 选项 C：trait 签名直接收结构化 `aad: &AssociatedData`（trait + impl）

- 怎么做：定义 `pub trait AssociatedData { fn to_bytes(&self) -> Vec<u8>; }` + impl for `struct StandardAad { kind: AadKind, origin_device_id: String, seq: u64 }`；trait Sealer::encrypt 签名 `fn encrypt(&self, key: &[u8;32], plaintext: &[u8], aad: &dyn AssociatedData)`。
- 优点：完全把 AAD 从字节抽象成结构 → 编译期最强保证
- 缺点：**dyn trait object** 引入运行时 vtable + 调用面冗长；future PSK 加 challenge_nonce 字段需扩 trait（破坏向后兼容）；与 aes-gcm crate 的 `aad: &[u8]` 入参之间还要 `.to_bytes()` 一次额外分配；mock 测试要为每个 kind 写一个 impl AssociatedData；**过度抽象**违反 CLAUDE.md v5-1 / v5-4
- 实现复杂度：中
- 跨平台风险：无
- 否决理由：dyn trait 对 1 个具体 impl 是过度工程；选项 B 用 `pub fn build_aad` 已达"集中 + 编译期保证"目标 90%

---

## 3. 决定（Decision）

### 3.1 Trait 定义 — 选 选项 B（保留 2 trait，Verifier 降级为注释占位）

```rust
// crypto/mod.rs

pub mod x25519;
pub mod aes_gcm;

pub use x25519::X25519KeyExchange;
pub use aes_gcm::AesGcmSealer;

// FUTURE: 若 v3 引入 PSK / HMAC challenge，会在此处新增
// `pub trait Verifier { fn verify_origin(&self, claim: &[u8]) -> Result<(), VerifyError>; }`
// 触发条件：ADR-008 第 3.7 节 PSK 否决决议被 supersede。
// 引入路径：新 ADR-N supersede 本 ADR 第 3.1 节，handler 增 1 行 verifier.verify_origin。

pub trait KeyExchange {
    type Secret;        // 默认 impl: x25519_dalek::EphemeralSecret
    type PublicKey;     // 默认 impl: x25519_dalek::PublicKey

    fn new_ephemeral() -> (Self::Secret, Self::PublicKey);
    fn pubkey_to_b64(pk: &Self::PublicKey) -> String;
    fn pubkey_from_b64(s: &str) -> Result<Self::PublicKey, KeyExchangeError>;
    /// 消费 secret（diffie_hellman API 强约束）；HKDF salt/info 由 impl 决定，调用方不传
    fn derive_aes_key(secret: Self::Secret, their: &Self::PublicKey)
        -> Result<[u8; 32], KeyExchangeError>;
}

pub trait Sealer {
    /// aad 由 caller 用 build_aad() 拼装（见 第 3.3 节 / ADR-008 MUST-1）
    /// 返回 (nonce_b64, ciphertext_b64)：nonce 12B + ct（含 16B GCM tag）各自 base64
    fn encrypt(&self, key: &[u8; 32], plaintext: &[u8], aad: &[u8])
        -> Result<(String, String), SealError>;
    fn decrypt(&self, key: &[u8; 32], nonce_b64: &str, ct_b64: &str, aad: &[u8])
        -> Result<Vec<u8>, SealError>;
}

#[derive(Debug, thiserror::Error)]
pub enum KeyExchangeError {
    #[error("pubkey base64 decode failed")] Base64,
    #[error("pubkey must be 32 bytes")] Length,
    #[error("hkdf expand failed")] Hkdf,
}

#[derive(Debug, thiserror::Error)]
pub enum SealError {
    #[error("nonce/ciphertext base64 decode failed")] Base64,
    #[error("nonce must be 12 bytes")] NonceLength,
    #[error("aead encrypt failed")] EncryptFailed,
    /// 触发 NetworkError → 422（ADR-003 第 3.6 节状态码表）；body 不区分原因
    #[error("aead decrypt failed (key mismatch / aad mismatch / tampered)")] DecryptFailed,
}
```

**为什么不选 A**（保留 Verifier trait）：v2 PSK 否决（ADR-008 第 3.7 节）后 Verifier 实质无 impl；空 trait + NoopVerifier 是 YAGNI；future PSK 引入时再新增 trait 同样 4 个 handler 调用点要改，新增 trait 成本可忽略。

**为什么不选 C**（god trait）：违反 SRP；future 切 Noise Protocol 改全 trait；mock 边界混乱。

### 3.2 默认实现（X25519KeyExchange / AesGcmSealer）

```rust
// crypto/x25519.rs
use x25519_dalek::{EphemeralSecret, PublicKey};
use hkdf::Hkdf;
use sha2::Sha256;

pub const HKDF_SALT: &[u8] = b"sync-copy-v2-salt";
pub const HKDF_INFO: &[u8] = b"sync-copy-v2:aes-256-gcm";

pub struct X25519KeyExchange;

impl super::KeyExchange for X25519KeyExchange {
    type Secret = EphemeralSecret;
    type PublicKey = PublicKey;
    // 4 方法 impl 略（implementer 落地）
}

// crypto/aes_gcm.rs
pub struct AesGcmSealer;

impl super::Sealer for AesGcmSealer {
    // encrypt / decrypt impl 略（implementer 落地）
    // encrypt 内部：Aes256Gcm::new(key) + 12B 随机 nonce (OsRng) + Payload { msg, aad }
    // decrypt 内部：Aes256Gcm::new(key) + Payload { msg: ct, aad }；aead 失败统一映射 SealError::DecryptFailed
}
```

**字段说明**：

- `X25519KeyExchange` / `AesGcmSealer` 都是 unit struct（无字段）—— 无状态、不 Mutex / RwLock；线程安全靠"无内部可变状态"。
- HKDF 常量 `HKDF_SALT` / `HKDF_INFO` 在 `crypto/x25519.rs` 顶部 `pub const`（仅一处定义，避免错位）；ADR-003 卡片 4 must-fix v2 bump 落地点。
- ADR-009 PeerState.aes_key（`Zeroizing<[u8; 32]>`）的填值路径 = handshake handler 调 `X25519KeyExchange::derive_aes_key(my_secret, their_pub)?` 拿到 `[u8; 32]` → 用 `Zeroizing::new(...)` 包装 → 写入 PeerState。本 ADR 不让 trait 返 `Zeroizing<[u8; 32]>`（保 trait 与 zeroize 解耦，便于 future 切实现 / 跑测试不依赖 zeroize）。

### 3.3 AAD 绑值实现（落实 ADR-008 第 7.2 节 MUST-1）— 选 选项 B

```rust
// crypto/mod.rs

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AadKind {
    Text,             // → b"text"            （clipboard-text-sync）
    ImagePng,         // → b"image_png"       （clipboard-image-sync）
    File,             // → b"file"            （file-transfer-drag）
    Trust,            // → b"trust"           （group-trust-gossip）
    Ban,              // → b"ban"             （group-trust-gossip）
    Leave,            // → b"leave"           （group-leave-notify）
    DeleteHistory,    // → b"delete_history"  （history-sync-delete）
    ClearHistory,     // → b"clear_history"   （history-sync-delete）
    Approval,         // → b"approval"        （group-approval forward 回流报文）
}

impl AadKind {
    pub fn as_bytes(&self) -> &'static [u8] { /* 9 路 match 略 */ }
}

pub const AAD_MAGIC: &[u8] = b"sync-copy-v2";

/// ADR-008 第 7.2 节 MUST-1 落地：AAD = magic || kind || origin_device_id || seq (BE 8 bytes)
/// 所有 broadcast handler 在 encrypt / decrypt 前**唯一入口**调本函数；禁止散点拼装
pub fn build_aad(kind: AadKind, origin_device_id: &str, seq: u64) -> Vec<u8> {
    let id_bytes = origin_device_id.as_bytes();
    let mut buf = Vec::with_capacity(AAD_MAGIC.len() + kind.as_bytes().len() + id_bytes.len() + 8);
    buf.extend_from_slice(AAD_MAGIC);
    buf.extend_from_slice(kind.as_bytes());
    buf.extend_from_slice(id_bytes);
    buf.extend_from_slice(&seq.to_be_bytes());
    buf
}
```

**字节顺序锁死**：`magic(12B) || kind(变长 ASCII) || origin_device_id(变长 UTF-8) || seq(8B big-endian)`。

**big-endian 选取理由**：u64 网络字节序约定；与 protocol 字段 `seq` 是 JSON 数字（语言无关）配合时双方都以 BE 解释最直观；single-source-of-truth = build_aad 实现，跨平台一致。

**kind 字面量表**（9 个，与 ADR-009 第 3.1 节 last_seen_seq_by_kind 对应）：

| AadKind 枚举 | as_bytes() 字面量 | 来源 spec |
|---|---|---|
| Text | `b"text"` | clipboard-text-sync |
| ImagePng | `b"image_png"` | clipboard-image-sync |
| File | `b"file"` | file-transfer-drag |
| Trust | `b"trust"` | group-trust-gossip |
| Ban | `b"ban"` | group-trust-gossip |
| Leave | `b"leave"` | group-leave-notify |
| DeleteHistory | `b"delete_history"` | history-sync-delete |
| ClearHistory | `b"clear_history"` | history-sync-delete |
| Approval | `b"approval"` | group-approval（forward 回流） |

**调用契约表**（handler 调用面 — implementer 必读）：

| Handler | encrypt 路径 | decrypt 路径 |
|---|---|---|
| network/handlers/clipboard.rs (text) | `let aad = build_aad(AadKind::Text, my_id, seq); sealer.encrypt(&key, plaintext, &aad)?` | 同样 build_aad 用 req.origin_device_id |
| network/handlers/clipboard.rs (image) | `AadKind::ImagePng` | 同上 |
| network/handlers/file.rs | `AadKind::File` | 同上 |
| network/handlers/gossip.rs (trust/ban) | 不加密（gossip 报文非机密；ADR-008 第 3.6 节范围内仅 clipboard/file 加密）；本表无入口 | — |

**ADR-008 第 3.6 节范围确认**：AAD 绑值仅作用于"加密报文"（clipboard / file / 未来 history-sync-delete 的 payload 字段）；trust / ban / leave / approval 等握手 / gossip 报文 v2 不加密（payload 本就是公开 device_id），AAD 绑值无意义；本 ADR 在 AadKind 枚举保留 trust / ban / leave / delete_history / clear_history / approval 是为了"未来若需加密 gossip 报文 enum 已就位"，与 ADR-009 last_seen_seq_by_kind 的 9 个 kind 一致（dedupe 字段任何 kind 都需要）。

### 3.4 HKDF salt v2 bump（落实 ADR-003 卡片 4 must-fix）

**字面量唯一定义点**：`crypto/x25519.rs` 顶部 `pub const HKDF_SALT: &[u8] = b"sync-copy-v2-salt"; pub const HKDF_INFO: &[u8] = b"sync-copy-v2:aes-256-gcm";`。

**不互通声明**：v0 prototype 用 `b"sync-copy-v1-salt"` / `b"sync-copy-v1:aes-256-gcm"`；v2 build 与 v0 build 派生密钥不同 → 即使握手包字段兼容也无法解密对方报文。**这是设计选择，不是 bug**——v2 协议字段（如 image_width / image_height 在 image_png 路径）与 v0 不兼容，密钥层强制不互通是冗余防线。release notes v2.0.0 必须显式说明"v2 与 v0 / v1 build 不互通；混跑会卡在握手或解密失败"。

**Future bump 路径**：v3 引入时新 ADR-N supersede 本节，改 `HKDF_SALT` 为 `b"sync-copy-v3-salt"`；同步改 AAD_MAGIC 为 `b"sync-copy-v3"`（第 3.3 节）—— 两常量 bump 一致是不变式，code-reviewer 在 PR 阶段强制 grep 检查"两个 v 数字相等"。

### 3.5 zeroize 应用边界（落实 ADR-008 MUST-2 + 第 3.5 节）

**已锁定的 zeroize 应用点**（ADR-009 + x25519-dalek 自带）：

| 数据 | 类型 | zeroize 来源 | Drop 时机 |
|---|---|---|---|
| `EphemeralSecret`（X25519 临时秘钥） | x25519_dalek::EphemeralSecret | x25519-dalek 2.x 自带 ZeroizeOnDrop | derive_aes_key 消费即 drop |
| `SharedSecret`（DH 输出） | x25519_dalek::SharedSecret | x25519-dalek 2.x 自带 ZeroizeOnDrop | derive_aes_key 函数返回前 drop |
| `PeerState.aes_key`（每 peer AES key） | `Zeroizing<[u8; 32]>` | ADR-009 第 3.1 节 / zeroize crate | PeerRegistry::remove / re-handshake 覆盖 |

**本 ADR 新决议（trait 实现内部边界）**：

| 数据 | 在哪个 impl 内 | 是否需 zeroize | 理由 |
|---|---|---|---|
| HKDF expand 输出缓冲 `[u8; 32]`（局部变量） | `X25519KeyExchange::derive_aes_key` | **不强制** | 该缓冲返回给 caller 作为 aes_key；caller 负责 Zeroizing 包装；trait 内部不双重清零（避免提前清零导致返回后字节空）。**实现层约束**：函数返回该缓冲后**禁止**在 trait impl 内额外引用（`let key = [0u8; 32]; ...; Ok(key)` 后无任何 `key` 副本逃逸） |
| AAD `Vec<u8>`（build_aad 输出） | 调用方栈帧 | **不需要** | AAD 是公开值（含 origin_device_id / seq / kind 都是协议明文字段），无机密性 |
| plaintext `Vec<u8>`（decrypt 输出） | `AesGcmSealer::decrypt` 返回值 | **不强制 zeroize**，但**调用方**（network/handlers/clipboard.rs 的 image/text 写剪切板路径）应在写完 arboard 后立即 drop | 明文敏感性等同于 OS 剪切板内容（已暴露给系统级 API）；本 ADR 不要求 trait 层包 Zeroizing<Vec<u8>>（违反 trait 简洁性）；caller 路径短即 drop 已是标准 Rust ownership |
| nonce `[u8; 12]`（OsRng 生成） | `AesGcmSealer::encrypt` 局部 | **不需要** | nonce 公开值（base64 走 JSON 字段送给对端）；GCM 安全性不依赖 nonce 保密 |
| aes-gcm crate 内部 cipher state | aes-gcm crate 内部 | **库责任** | aes-gcm 0.10 内部 [Aes256Gcm](https://docs.rs/aes-gcm/0.10) 已 derive ZeroizeOnDrop（v0.10 起）；本 ADR 不重复 |

**零额外 zeroize 引入**：除 ADR-008 / ADR-009 已锁的 `Zeroizing<[u8; 32]>` 一处类型外，本 ADR 不在 crypto module 内**新增**任何 zeroize 调用；trait 实现保持纯函数式 API，复杂度不增。

**反模式黑名单**：

- ❌ trait impl 内 `let mut key = [0u8; 32]; ...; key.zeroize(); Ok(key)` —— zeroize 后返回空字节
- ❌ build_aad 输出包 Zeroizing —— AAD 公开值，包 Zeroizing 无意义且 trait 签名 `&[u8]` 不接受
- ❌ caller 把 PeerState.aes_key clone 出来后赋给非 Zeroizing 变量（如 `let k: [u8;32] = state.aes_key.clone().into_inner()` 失去 zeroize 链）

### 3.6 单元测试清单（≥ 6 条；落实 e2e-encryption.md AC #6 + ADR-008 MUST-1 单测）

implementer 在 `crypto/x25519.rs` + `crypto/aes_gcm.rs` + `crypto/mod.rs` 三 module 内写 inline `#[cfg(test)]` 测试；最小集 6 条，建议 9 条（覆盖每条 AAD 维度）：

| # | 测试名 | 在哪个 module | 断言点 |
|---|---|---|---|
| 1 | `roundtrip_text_decrypts_to_plaintext` | aes_gcm.rs | 同 key / 同 aad encrypt 后 decrypt 返回原 plaintext 字节相等 |
| 2 | `wrong_key_decrypt_fails` | aes_gcm.rs | 第二把 key（HKDF 同 salt 但不同 shared）解密返 SealError::DecryptFailed（落实 e2e-encryption AC #3） |
| 3 | `aad_byte_flip_decrypt_fails` | aes_gcm.rs | encrypt 后把传入 decrypt 的 aad 翻一字节（含 magic / kind / origin / seq 各一个 case）→ 全部 SealError::DecryptFailed（落实 ADR-008 MUST-1 单测要求） |
| 4 | `cross_origin_aad_rejected` | aes_gcm.rs | encrypt 用 `build_aad(Text, "device-A", 1)`；decrypt 用 `build_aad(Text, "device-B", 1)` → DecryptFailed |
| 5 | `cross_seq_aad_rejected` | aes_gcm.rs | encrypt 用 `seq=1`；decrypt 用 `seq=2` → DecryptFailed |
| 6 | `nonce_uniqueness_under_repeated_encrypt` | aes_gcm.rs | 用同 key + 同 plaintext + 同 aad 调 encrypt 100 次；100 个 nonce_b64 全不同（OsRng 12B 碰撞概率 ≈ 0） |
| 7 (建议) | `hkdf_deterministic_same_inputs_same_key` | x25519.rs | mock secret 注入相同 shared_secret → derive_aes_key 输出字节相等（落实 e2e-encryption AC #4） |
| 8 (建议) | `cross_peer_keys_differ` | x25519.rs | 跑 3 次 new_ephemeral + derive，两两 key 字节不等（落实 e2e-encryption AC #5） |
| 9 (建议) | `pubkey_b64_roundtrip` | x25519.rs | pubkey_to_b64 → pubkey_from_b64 → 相等；长度 == 44 字符（落实 e2e-encryption AC `公钥 base64 长度 44`） |

**zeroize 落地不在单测层强制**（ADR-008 第 3.5 节已说明跨平台不可靠）；改由"类型签名 `Zeroizing<[u8;32]>`"在编译期保证 + ADR-009 单测 #14 best-effort 覆盖。

**build_aad 单测**（建议在 mod.rs）：1 条 `aad_layout_byte_exact` 用固定输入断言输出字节序列等于手算的 magic+kind+id+seq.to_be_bytes() 拼接结果；防止 future 重构改顺序失误。

---

## 4. 后果（Consequences）

### 4.1 正面

- **ADR-003 第 3.4 节方向 + ADR-008 MUST-1 / MUST-2 闭环到 trait 签名 + 调用契约**：implementer 拿到 ADR-011 后无解释空间——两 trait 签名写死、`build_aad` 拼装顺序写死、HKDF 常量唯一定义点写死、调用契约表覆盖 3 handler 路径
- **AAD 拼装收敛到 1 个函数 + 1 个 enum**：handler 调用面收敛到一行；future bump v3 / 引入 PSK 改 build_aad 内部即可；`AadKind` enum 编译期保证 kind 字面量取值正确
- **HKDF v2 bump 字面量唯一定义点**：crypto/x25519.rs 顶部 2 个 pub const；implementer 不会在 2 个文件错位；release notes v2.0.0 强制声明"与 v0/v1 不互通"是设计选择
- **zeroize 边界明确**：trait 实现内部不引入额外 zeroize（保 trait 简洁）；保护链由 ADR-009 PeerState.aes_key + x25519-dalek 自带 + aes-gcm 自带三方共同负责，本 ADR 仅划清边界
- **测试矩阵覆盖到 ≥ 6 条 + AAD 各维度防线**：e2e-encryption AC #6（≥ 5 条）+ ADR-008 MUST-1 单测（aad 翻字节）一并闭环；建议 9 条覆盖跨 origin / 跨 seq / nonce 唯一性 / HKDF 确定性 / 跨 peer 密钥独立 / pubkey b64 roundtrip
- **trait 数从 ADR-003 草案 3 个降到 2 个**：避免 v2 出现 0-impl Verifier trait（YAGNI 闭环）；future PSK 引入路径仍清晰（注释 + ADR-008 第 3.7 节决议双锚点）

### 4.2 负面 / 妥协

- **trait 仍有少量样板代码**（KeyExchange 关联类型 + 4 方法、Sealer 2 方法）；与 v0 函数式 6 函数相比代码量 +30 行 — 换得单元测试可达 + 未来切实现不破坏调用点
- **build_aad 输出 `Vec<u8>` 是堆分配**；每次 encrypt / decrypt 各一次（N=8 设备 / 心跳 10s 不可观测，但密集 broadcast 场景可观测）；future 优化路径 = 改 stack-allocated `[u8; ?]` 或 `SmallVec`，仅在 profiler 证明热点时做
- **AadKind 枚举有 9 个值但 v2 实质只用 3 个**（Text / ImagePng / File）；其余 6 个保留兼容 ADR-009 last_seen_seq_by_kind + future 加密 gossip 路径；冗余字段
- **trait 实现是 unit struct（X25519KeyExchange / AesGcmSealer 无字段）**；如 future 需注入"密钥派生策略" / "nonce 来源" / "PRNG" 配置，要改 struct + 改 new()；目前用 unit struct 是 YAGNI 闭环
- **HKDF v2 bump 让 v0 prototype 用户升级时报"解密失败"**：v0 build 与 v2 build 不互通需用户全员升级；release notes 需显式说明（运维成本，但 v2 是协议级断点版本，可接受）

### 4.3 需要警惕的副作用

- **build_aad 顺序错配**：implementer 若手抖把 origin / seq 顺序颠倒（如 `magic || kind || seq_be8 || id`），同版本两端互通但**与本 ADR 规范偏移**——code-reviewer 必须在 PR 阶段比对字面量顺序与本 ADR 第 3.3 节字节顺序锁死段；单测 #3 / #4 / #5 中 1 条会抓出（aad 翻字节场景），但前提是单测真按本 ADR 写
- **HKDF 常量未来 bump 时 v 数字不一致**：implementer 若只 bump HKDF_SALT（v3）忘记 bump AAD_MAGIC（仍 v2），加密路径仍能跑通但 magic 不变 → "v3 build 与 v2 build 在 magic 层互通而 HKDF 层不通" → 难调试。**对策**：本 ADR 第 3.4 节最后一段写"两常量 bump 一致是不变式"+ release notes 模板提示
- **caller 漏调 build_aad 直传 `&[]`**：trait 签名 `aad: &[u8]` 不阻挡空 aad（因 aes-gcm crate 接受空 aad）；若 implementer 在某 handler 路径手抖直传 `&[]` 跳过 build_aad，加密路径仍能跑通但 AAD 防线全空 → 与 v0 退化等价。**对策**：code-reviewer 在 PR 阶段 grep `sealer.encrypt(` 调用点，确认每处都有 `build_aad(` 在前一行
- **AesGcmSealer / X25519KeyExchange 是 unit struct + 全 stateless**：在 AppState 不需 Arc 包装；future 若需注入配置（如可注入 PRNG）时需 supersede 本节
- **Zeroizing<[u8;32]> Clone 拷贝字节**（ADR-009 第 4.3 节副作用 #4 已警告）：本 ADR `derive_aes_key` 返 `[u8; 32]` 裸数组，调用方包 Zeroizing → 链路开始处的"裸数组短暂存在栈帧"是不可避免的窗口（HKDF expand 直接写入裸数组 buffer）；最小化窗口靠 caller "拿到即包" 模式（< 5 行 Rust）

---

## 5. 实施提示（≤ 5 条，给 backend-implementer）

1. **3 文件落地**：`crypto/mod.rs`（trait 定义 + build_aad + AadKind enum + AAD_MAGIC + 错误 enum）/ `crypto/x25519.rs`（X25519KeyExchange impl + HKDF_SALT/INFO 常量 + 单测）/ `crypto/aes_gcm.rs`（AesGcmSealer impl + 单测）。三文件合计 ≤ 350 行（ADR-003 第 3.1 节硬约束 单文件 < 400 行）。
2. **HKDF / AAD_MAGIC 字面量唯一定义点**：HKDF_SALT/INFO 仅在 `crypto/x25519.rs` 顶部；AAD_MAGIC 仅在 `crypto/mod.rs` 顶部；其它 module 不重定义。code-reviewer 在 PR 阶段 grep `sync-copy-v2` 字面量个数 = 3（salt + info + magic 各一处）。
3. **Sealer / KeyExchange 是 unit struct**：在 `app/state.rs::AppState` 不需 Arc 包装；handler 内 `crypto::aes_gcm::AesGcmSealer.encrypt(...)` 直接调即可（trait method on unit value）。
4. **build_aad 调用纪律**：所有 broadcast handler（clipboard text / clipboard image / file）在 encrypt / decrypt 调用**前一行**调 `build_aad(AadKind::*, origin_device_id, seq)` 拿 aad 变量，禁止散点拼装、禁止跳过 build_aad 直传 `&[]`。
5. **不要做的反模式**：
   - ❌ 在 trait impl 内 zeroize 返回值（提前清零返回空字节）
   - ❌ 把 PeerState.aes_key clone 出来赋给非 Zeroizing 变量（失去 zeroize 链）
   - ❌ HKDF 常量与 AAD_MAGIC v 数字不一致（v3 bump 时同步改）
   - ❌ 在 handler 内 `sealer.encrypt(&key, plaintext, &[])` 跳过 build_aad（AAD 防线退化到 v0）
   - ❌ build_aad 修改顺序 / 字节序 / kind 字面量取值（本 ADR 第 3.3 节字节顺序锁死；future 要改走 supersede）

---

## 6. 验证（How to Verify）

### 6.1 怎么证决策对（单元 + 集成测试）

**单元测试**（最小 6 条 + 建议 3 条详见第 3.6 节表）：

- 6 条最小集（roundtrip / wrong_key / aad_flip / cross_origin / cross_seq / nonce_uniq）覆盖加密核心防线
- 3 条建议（hkdf_deterministic / cross_peer / pubkey_b64）覆盖密钥协商防线 + e2e-encryption AC #4 / #5 / 公钥长度
- build_aad 1 条（aad_layout_byte_exact）覆盖第 3.3 节字节顺序锁死

**集成测试**（与 P2-2 阶段后 implementer + qa-tester 在 e2e 测试栈跑）：

- 跨平台 macOS ↔ Windows 互通：v2 build A 发文本 → v2 build B 收，明文相等
- v0 prototype 与 v2 build **不互通**：v0 发握手 → v2 解 HKDF salt v1 不匹配 → 派生 key 不同 → broadcast 时 422（密钥不一致）—— release notes 必须说明
- AAD 防线 e2e：用代理工具改 JSON `origin` 字段重发 → v2 接收端 422（aad mismatch）；改 `seq` 字段同样 422（aad mismatch）；这一防线 v0 无法覆盖（v0 AAD 空）

### 6.2 怎么证决策错（supersede 触发）

- **build_aad 拼装在 prod 用户报"两端 v2 互不通"**（运行时所有 broadcast 解密失败）→ 顺序错配 / 字节序错配；supersede 第 3.3 节字节顺序锁死段或修 implementer bug
- **trait 边界被发现限制 future PSK 演进**（PSK 引入需改 KeyExchange + Sealer 双 trait）→ supersede 第 3.1 节，引入 Verifier trait
- **HKDF v2 bump 让 v0 用户全员升级时反馈"卡死握手"**（用户没看 release notes）→ 加 fallback 探测路径（握手时同时发 v1+v2 两个公钥 / 协议 version 字段）；supersede 第 3.4 节
- **build_aad Vec<u8> 堆分配热点**（profiler 证明 broadcast 密集场景占 broadcast 总耗时 > 5%）→ 改 SmallVec / stack buffer；supersede 第 3.3 节实现细节但保字节顺序
- **AadKind 9 个值发现遗漏**（如 future 加 audio / video kind）→ enum 加值 + 单测覆盖；不一定要 supersede 本 ADR，仅扩 enum
- **3 单测最小集发现 bug 漏抓**（如 implementer 误把 `seq.to_le_bytes()` 写进 build_aad 但单测只测同一端）→ 加跨端集成测试；supersede 第 3.6 节单测清单加最小集到 7 条

---

## 7. 安全审阅（by security-reviewer · 2026-05-09）

**结论**：APPROVED

### 7.1 审阅范围

聚焦 5 点：(1) MUST-1 AAD 字节序与字面量字面级一致性（vs ADR-008 第 7.2 节）；(2) HKDF v2 bump 三常量唯一定义点 + future bump 不变式；(3) MUST-2 zeroize 边界（trait 实现内部不引入额外 zeroize）；(4) AES-GCM nonce 唯一性 + caller 不可注入；(5) caller 漏调 build_aad 直传 `&[]` 退化路径。**不重审** ADR-008 已审过的算法选型 / nonce 处理基础原则 / AAD 绑值规范本身（仅查"本 ADR 是否字节级一致地落地了 ADR-008 规范"）。

### 7.2 审阅意见

1. **AAD 字节序与字面量** ✅：第 3.3 节 `build_aad` 拼装 = `AAD_MAGIC(b"sync-copy-v2") || kind.as_bytes() || origin_device_id.as_bytes() || seq.to_be_bytes()` 与 ADR-008 第 7.2 节 MUST-1 字面级一致；`AadKind` 9 值字面量 (`text` / `image_png` / `file` / `trust` / `ban` / `leave` / `delete_history` / `clear_history` / `approval`) 全小写 + snake_case 无大小写 / 顺序歧义；与 ADR-009 第 3.1 节 `last_seen_seq_by_kind` 9 kind 一一对应；BE8 选择正确（u64 网络字节序 + 跨平台 single-source-of-truth = build_aad 自身）；接收端经同 build_aad 重建 aad（调用契约表第 2 列），双端字节相等。
2. **HKDF v2 bump 字面量唯一性** ✅：`HKDF_SALT` / `HKDF_INFO` 在 `crypto/x25519.rs` 顶部 `pub const`（第 3.2 节）；`AAD_MAGIC` 在 `crypto/mod.rs` 顶部 `pub const`（第 3.3 节）；3 处单点定义 + grep 验证（实施提示 #2 `sync-copy-v2` 字面量 = 3）；future bump v3 不变式（三常量 v 数字必须一致）在第 3.4 节末段 + 4.3 节副作用 #2 + release notes 模板占位三处锚定。
3. **MUST-2 zeroize 边界** ✅：trait 实现内部不新增 zeroize 调用 = 正确边界。EphemeralSecret + SharedSecret 由 x25519-dalek 2.x 自带 ZeroizeOnDrop（ADR-008 第 3.5 节已审）；aes-gcm 0.10 的 `Aes256Gcm::new(key)` 内部状态由 RustCrypto 生态自带 ZeroizeOnDrop（v0.10 起）；HKDF expand 输出 [u8;32] 由 caller 即包 `Zeroizing::new(...)`（ADR-009 第 3.1 节锁定）。第 3.5 节反模式黑名单 3 条覆盖到位。
4. **nonce 唯一性 + AEAD 安全** ✅：`Sealer::encrypt` 签名仅收 plaintext + aad，nonce 是**返回值**而非入参 → caller 在 API 层无法注入 / 复用 nonce；第 3.2 节 encrypt 内部"12B 随机 nonce 来自 OsRng" + 单测 #6 `nonce_uniqueness_under_repeated_encrypt`（100 次同输入断言 100 个 nonce 全不同）覆盖到位。AES-GCM nonce reuse 在密码学层由 trait 签名禁止。
5. **caller 漏调 build_aad** ⚠（已识别 + 缓解就位，非阻塞）：trait 签名 `aad: &[u8]` 不阻挡空字节串；第 4.3 节副作用 #3 + 第 5 节实施提示 #4 + 反模式 #4 三处点名；缓解 = code-reviewer PR grep `sealer.encrypt(` 前一行必有 `build_aad(`。**sec 决议不强制 trait 签名层面 newtype 包装**（如 `pub struct Aad(Vec<u8>);` 私字段构造）—— 工程优化属 implementer 自由选项；clippy custom lint 在 v2 阶段过度工程；PR grep + 单测 #3/#4/#5（aad 翻字节 / 跨 origin / 跨 seq）形成的"漏调即测试失败"防线已构成合理深度。

### 7.3 必修补丁

无（结论 APPROVED）。

**建议级（非阻塞）补丁**（implementer / code-reviewer 自行判断是否采纳，**不阻塞 ACCEPTED**）：

- 第 3.5 节"反模式黑名单"可加 1 条："❌ AesGcmSealer::encrypt 签名增加 nonce 入参"——锁死 nonce 由 impl 生成，防 future 重构破坏密码学不变式。
- 第 5 节实施提示 #5 反模式可补一句："code-reviewer PR 阶段 grep `sealer\.encrypt\(.*&\[\]` 必须 0 命中"，把第 4.3 节缓解措施从注释升级为可执行检查项。

### 7.4 结论

APPROVED — ADR-011 字节级一致地落地了 ADR-008 MUST-1 / MUST-2，三个加密层不变式（AAD 绑值字节序 / HKDF v2 bump 字面量唯一 / nonce 由 impl 内部生成）在 trait 签名 + 调用契约 + 单测三层闭环。无严重密码学问题，可进入 IMPL_IN_PROGRESS。

---

## 8. 决策卡片清单（v5-11）

> 仅 3.1 / 3.3 是有可选项的关键拍板点。3.2 / 3.4 / 3.5 / 3.6 是 ADR-003 + ADR-008 + ADR-009 已决方向的细化（默认实现 / HKDF v2 bump 字面量 / zeroize 边界划分 / 单测清单），无可选项不出卡片。

> **注**：本 ADR 2 张卡片均为技术实现细节（trait 拆分粒度 / AAD 入参形态），按 lessons-learned 第 5 段第 10 条新策略主窗口直接采纳推荐项；不上报用户。

### 卡片 1 / 2 — Trait 拆分粒度（第 3.1 节）

**问题**：ADR-003 第 3.4 节 trait 化方向已锁，但拆几个 trait？v2 PSK 已被 ADR-008 否决，Verifier trait 是否还出？

**选项**：

- **A**: 保留 3 trait（KeyExchange / Sealer / Verifier）— ADR-003 草案
- **B**: 保留 2 trait（KeyExchange / Sealer），Verifier 降级为 module 顶部注释占位（推荐）
- **C**: 合并 1 trait `Cryptosuite`（god trait）

**推荐**：B

**取舍**：
- A：v2 Verifier 实质 0-impl，YAGNI 反模式；future PSK 引入时新增 trait 与改 4 handler 调用点反正都要做，trait 新增成本可忽略
- B：避免 v2 出 0-impl trait；注释 + ADR-008 第 3.7 节决议双锚点守住 future 演进；与 ADR-009 TrustState::Pending 保留枚举值同手法
- C：违反 SRP；future 切 Noise Protocol 改全 trait；mock 边界混乱 — **否决**

**must-fix**：选 B 后，crypto/mod.rs 顶部加 `// FUTURE: PSK / HMAC challenge ...` 注释段（指向 ADR-008 第 3.7 节）；future ADR-N supersede 时再正式定义 Verifier trait

### 卡片 2 / 2 — AAD 入参形态（第 3.3 节）

**问题**：ADR-008 第 3.6 节锁定 AAD 绑值组成，但 trait Sealer::encrypt 的 aad 参数类型用什么？拼装代码放哪？

**选项**：

- **A**: `aad: &[u8]` 裸字节；调用方在 handler 内自行拼装 4 段（ADR-003 第 3.4 节草案 + ADR-008 第 3.6 节实施提示原文）
- **B**: trait 签名仍 `&[u8]`，但 crypto module 暴露 `pub fn build_aad(kind: AadKind, origin_device_id, seq) -> Vec<u8>`；handler 必须经 build_aad（推荐）
- **C**: trait 签名收 `aad: &dyn AssociatedData` trait object

**推荐**：B

**取舍**：
- A：3 handler 散点拼装；implementer 误写顺序 / 字节序 / kind 字面量编译器抓不出；与 v0 函数式风格倒退
- B：拼装收敛到 1 函数 + 1 enum（AadKind）；编译器保证 kind 取值正确；handler 调用面收敛到一行；future bump v3 改 build_aad 内部不动调用点；与 ADR-009 last_seen_seq_by_kind 9 kind 一致
- C：dyn trait + vtable 对 1 个具体 impl 是过度抽象；违反 v5-1 / v5-4 — **否决**

**must-fix**：选 B 后，AadKind enum 9 值 + as_bytes() 表与 ADR-009 第 3.1 节 last_seen_seq_by_kind 字面量一一对应；code-reviewer PR grep `sealer.encrypt(` 每处前一行必有 `build_aad(`；HKDF_SALT/INFO 与 AAD_MAGIC 三常量字面量个数 = 3（避免散点定义错位）

---

## 9. 自查

**过度工程**：本 ADR 行数 ≤ 500 行（约 480 行）；不重复 ADR-003 第 3.4 节算法选型 / ADR-008 第 3.6 节 AAD 字节组成 / ADR-009 第 3.1 节 PeerState.aes_key 类型论证；trait 数从 ADR-003 草案 3 个降到 2 个（Verifier 降级注释占位，YAGNI 闭环）；未引新 crypto crate（仅复用 ADR-003 已锁的 x25519-dalek + aes-gcm + hkdf + sha2 + zeroize）；决策卡片仅 2 张（覆盖 3.1 / 3.3 真正可选点；3.2 / 3.4 / 3.5 / 3.6 不出卡 — ADR-003/008/009 已锁方向）；卡片标注"技术实现细节直接采纳推荐"主窗口可静默落地。

**owner 边界**：只写 trait / struct 签名 + build_aad 函数签名 + AadKind enum + 调用契约表 + 单测 list；未写 .rs 函数体实现代码；未改 spec 第 1-7 节业务范围（仅建议 spec frontmatter related_adrs 加 ADR-011，4 份 spec：e2e-encryption / clipboard-text-sync / clipboard-image-sync / file-transfer-drag）；未改 PLAN.md（建议见汇报）；未调用任何 agent。

**v5 规则镜像**（CLAUDE.md 第 14 节）：
- v5-1 不引入 0-impl trait（Verifier 降级注释占位）
- v5-3 严格 SDLC（依赖 ADR-003 + ADR-008 + ADR-009，不跳步）
- v5-4 不引新依赖（zeroize / x25519-dalek / aes-gcm / hkdf / sha2 / base64 / rand 全是 ADR-003 已锁；本 ADR 不加）
- v5-7 SDK idempotent（HKDF_SALT/INFO 与 AAD_MAGIC 三常量字面量唯一定义点 + bump v 数字一致约束）
- v5-9 本 ADR 即 crypto traits registry
- v5-10 三向决议（HKDF v2 bump 在 ADR-003 卡片 4 / ADR-011 第 3.4 节 / release notes v2.0.0 三处一致；AAD 绑值在 ADR-008 第 3.6 节 / ADR-011 第 3.3 节 / build_aad 单测 / e2e 集成测试四处一致）
- v5-11 决策卡片 2 张含 问题/选项/推荐/取舍/must-fix；技术细节卡片标注"直接采纳推荐"
- v5-12 章节符号禁令遵守（无 § 符号）

**状态机制**：PROPOSED → 主窗口直接采纳推荐 1B / 2B（lessons-learned 第 5 段第 10 条新策略：技术细节卡片不上报用户）→ 调 security-reviewer 审第 7 节占位段 → CHANGES_REQUESTED 走文本级补丁主窗口直接落 → ACCEPTED → P2-1.b 第二批 / P2-1.c 启动。
