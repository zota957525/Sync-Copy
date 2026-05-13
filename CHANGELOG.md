# Changelog

All notable changes to Sync Copy will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

---

## [Unreleased]

---

## [0.2.0] — 2026-05-10

This release represents a **complete rewrite** from the v0 prototype (`legacy-prototype` branch, commit `f4be188`). All code is new; the v0 prototype is preserved for historical reference only. The primary motivation is documented in `decisions/ADR-001`.

### Added

- **E2E 加密 (e2e-encryption)**：X25519 ECDH 临时密钥对 + HKDF-SHA256 + AES-256-GCM 全链路端到端加密。每对 peer 独立会话密钥，进程退出即清除（前向保密）。AAD 绑值防跨 kind/peer/seq 重放。(`decisions/ADR-008`, `decisions/ADR-011`, `specs/e2e-encryption.md`)

- **剪切板文本同步 (clipboard-text-sync)**：arboard 1s 轮询 + SHA-256 环路防护 + 1 MB 上限 + 5s 去重窗口。(`specs/clipboard-text-sync.md`)

- **N≥3 Gossip Mesh (group-discovery)**：HandshakeResp 携带 PeerStub 列表，新成员自动 gossip dial 扩展完整 mesh；`/peers/announce` 让已有成员反向连接新成员；`GOSSIP_MAX_CONCURRENT=3` 防 cascade 风暴。(`specs/group-discovery.md`, `decisions/ADR-003` 第 3.2 节)

- **分布式审批 (group-approval)**：任一在线设备点同意即全组生效，其他弹框自动 dismiss；30s 超时返 408；trust gossip 传播审批结果。(`specs/group-approval.md`)

- **心跳 5s + 隐形掉线修复 (peer-heartbeat)**：5s 主动 ping，连续 5 次失败触发强制重建底层 TCP 连接（force_rebuild 6 步）；`last_successful_sync_at` 仅在广播 200 OK 时写入（v0 实战 bug 根治）。(`specs/peer-heartbeat.md`, `decisions/ADR-010` 第 3.6 节, `decisions/ADR-009`)

- **Lifecycle 4 阶段 + 4 退出路径收敛 (lifecycle)**：Booting → Running → Shutting → Dead 状态机；托盘/设置/Cmd+Q/Win×四路退出统一经 `quit_app` → `Lifecycle::shutdown`；关闭 7 步总 deadline ≤2800ms。(`decisions/ADR-010`)

- **PeerRegistry 集中 peer 状态 (peer-registry)**：统一替换 v0 散点 4-HashMap；approved/banned 互斥短路集合；Zeroizing<[u8;32]> aes_key；remove 内嵌 client_pool.remove（原子顺序）。(`decisions/ADR-009`)

- **历史列表 (history-list)**：内存中最近 50 条同步内容（VecDeque/FIFO）；单击复制回剪切板；✕ 删除；清空全部。(`specs/history-list.md`)

- **设置面板 (settings-panel)**：设备名编辑（持久化到 ProjectDirs JSON）；清除历史；退出应用。(`specs/settings-panel.md`)

- **悬浮球 (floating-ball)**：浮窗收缩为 48×48 圆形；8px 移动阈值消歧点击/拖动；记忆展开前尺寸。(`specs/floating-ball.md`)

- **系统托盘 (tray-integration)**：macOS 菜单栏 / Windows 通知区 4 项菜单（显示/隐藏/设置/退出）。(`specs/tray-integration.md`)

- **11 个 Tauri 命令**：`quit_app` / `get_status` / `get_peers` / `join_group` / `get_config` / `set_config` / `approve_peer` / `reject_peer` / `get_history` / `delete_history_item` / `clear_history` / `recopy_history_item`

- **3 个 Tauri 事件**：`status-updated` / `history-updated` / `peer-pending`

- **153 条单元测试全过**：142 lib 单测 + 11 集成测试（含 gossip mesh 三机场景）

### Changed

- 项目从 v0 草率堆砌重写为严格 SDLC 流程：每个 feature 有 spec、每个技术决策有 ADR、每个实现有 code review + QA 测试通过方能合入。(`decisions/ADR-001`)

- 后端模块切分：原 `network/server.rs`（784 行单文件）拆分为 `handlers/`（7 个子文件）+ 独立 `client.rs` / `client_pool.rs` / `health.rs` / `rate_limit.rs`；原 `state.rs` 上帝结构拆分为 `peer/mod.rs`（PeerRegistry）+ `app/state.rs`（仅持有子域 Arc）。(`decisions/ADR-003` 第 3.1 节)

- 前端从 `+page.svelte` 单文件 1483 行重写为 8 个独立 Svelte 5 组件（FloatingWindow / FloatingHeader / StatusDot / HistoryList / ApprovalDialog / SettingsPanel / ClearConfirm / FloatingBall）。(`decisions/ADR-003` 第 3.1 节)

- 加密层从函数式 6 函数改为 trait 化（`KeyExchange` + `Sealer`），单元测试可达；HKDF salt/info 版本从 v1 bump 至 v2（`b"sync-copy-v2-salt"` / `b"sync-copy-v2:aes-256-gcm"`），v0 与 v2 build 协议层不互通（设计选择）。(`decisions/ADR-011`)

- Gossip mesh 从握手响应同步整张 peers 表改为最小化 PeerStub（仅 device_id + addr），加入 `/peers/announce` 端点实现双向扩展。(`specs/group-discovery.md` 第 8 节)

### Fixed

- **隐形掉线根治**：v0 实战 bug（长时间运行后 peer 表面在线但同步失败，重启程序才恢复）。v2 通过 `last_successful_sync_at`（仅广播 200 OK 写入）+ force_rebuild 强制重建底层 TCP 连接的双层机制根治。(`specs/peer-heartbeat.md` 第 1.1 节, `decisions/ADR-009` 第 3.5 节)

- `/file` 端点缺少 seq dedupe，可被重放触发重复文件接收弹框（v0 漏洞）。(`decisions/ADR-008` 第 4.2 节, MUST-6)

### Security

- **MUST-1 AAD 绑值全闭环**：所有加密报文 AAD = `magic || kind || origin_device_id || seq(BE8)`，防跨 kind/peer/seq 三维重放。(`decisions/ADR-008` MUST-1, `decisions/ADR-011` 第 3.3 节)

- **MUST-2 密钥内存清零**：`PeerState.aes_key: Zeroizing<[u8;32]>`，drop 时自动清零，防 dump 文件泄露。(`decisions/ADR-008` MUST-2, `decisions/ADR-009` 第 3.1 节)

- **MUST-3 403 通用 body**：ban / 未知 / 拒绝三种内部路径均返相同 403 body，防攻击者区分 device_id 状态。(`decisions/ADR-008` MUST-3)

- **MUST-4 PeerRegistry.remove 原子顺序**：`inner.remove → client_pool.remove` 在同一函数内严格此顺序；`client_pool.remove` 标 `pub(crate)` 禁外部直接调用。(`decisions/ADR-008` MUST-4, `decisions/ADR-009` 第 3.5 节)

- **MUST-5 panic 不含变量插值**：全局约定 `panic!` / `unwrap` / `expect` message 为静态字面量，防运行时敏感数据进 crash 报告。(`decisions/ADR-008` MUST-5)

- **MUST-6 /file seq dedupe**：补上 v0 遗漏的重放保护。(`decisions/ADR-008` MUST-6)

- **MUST-7 handshake DoS 限流**：`RateLimiter` 独立模块（`network/rate_limit.rs`），per-pair 60s ≤3 次、全局 60s ≤10 个不同 device_id。(`decisions/ADR-008` MUST-7, `decisions/ADR-009` 第 3.6 节)

- **MUST-8 device_name sanitize 三函数**：Bidi 控制字符黑名单 + 控制字符过滤 + 64 codepoints 上限，防 RTL override 注入。(`decisions/ADR-008` MUST-8, `specs/settings-panel.md` 第 4 节)

### Documentation

- 建立完整 specs/ + decisions/ 文档体系：20 份 feature spec + 7 份 ADR（ADR-001~ADR-011，含 ADR-002 HANDOFF v5 规范迁移）
- `specs/_assumptions.md`：44 条事实假设清单，用户校对确认
- `docs/handoff-lessons-learned.md`：v0 踩坑分类记录 + 项目经验教训

---

## [0.1.0] — 2026-05-05 (v0 prototype, legacy-prototype branch)

Initial working prototype. Features: clipboard text/image/file sync, X25519+AES-GCM E2E encryption, distributed approval, floating window + ball, system tray. No spec/ADR documentation; 0% test coverage. Preserved at `legacy-prototype` branch for historical reference.
