# Sync Copy v0.2.0 — v2 重写首个里程碑版本

> 这是 Sync Copy 的 **v2 完整重写版本**，从 v0 prototype（`legacy-prototype` 分支，commit `f4be188`）
> 完全重做。所有代码是新的；v0 prototype 仅作历史参考保留。
> 本版本代表 v2 重写目标的**完整交付**：后端业务逻辑 + 前端 UI + CI 跨平台构建全部就位。

---

## 亮点功能

### 1. E2E 端到端加密 — X25519 + AES-GCM 全链路

每对设备独立建立 X25519 ECDH 临时密钥对，通过 HKDF-SHA256 派生出唯一 AES-256-GCM 会话密钥。
所有报文携带 AAD（magic || kind || origin_device_id || seq），防跨类型/跨 peer/跨序号三维重放攻击。
进程退出时会话密钥自动清零（`Zeroizing<[u8;32]>`），实现前向保密。

技术决策来源：`decisions/ADR-008`、`decisions/ADR-011`

### 2. N≥3 设备 Gossip Mesh 自动扩展

新设备加入时，握手响应自动携带 PeerStub 列表（仅 device_id + addr 最小化字段）；
新成员自动 gossip dial 扩展完整 mesh；`/peers/announce` 让已有成员反向连接新成员；
`GOSSIP_MAX_CONCURRENT=3` 防 cascade 风暴。支持任意规模的 LAN 设备组。

### 3. 隐形掉线根治 — v0 实战 Bug 修复

v0 实战问题：长时间运行后，peer 表面显示在线（绿点），但实际 TCP 连接已死，剪切板同步失败，唯一兜底是重启程序。

v2 修复方案（双层机制）：
- **主动探测**：5 秒心跳 ping，连续 5 次失败触发 force_rebuild（6 步强制重建底层 TCP 连接）
- **准确同步状态**：`last_successful_sync_at` 仅在广播收到 200 OK 时写入，UI 显示真实的"上次成功同步时间"

### 4. 系统托盘 + 浮窗 UI + 折叠悬浮球

- **系统托盘**：macOS 菜单栏 / Windows 通知区，4 项菜单（显示/隐藏/设置/退出）
- **浮窗主界面**：状态点（绿/橙/红）+ peer 列表 + 历史列表 + 设置入口
- **折叠悬浮球**：48×48 圆形悬浮球，8px 移动阈值消歧点击与拖动，记忆展开前尺寸
- **审批弹框**：新设备入组弹框，30s 倒计时三色阈值（绿/橙/红）

### 5. 历史列表 — 最近 50 条同步内容

内存中保存最近 50 条同步记录（VecDeque FIFO）；支持单击复制回剪切板；支持逐条删除和清空全部。

### 6. 跨平台构建 — macOS Apple Silicon/Intel + Windows x64

GitHub Actions 矩阵构建：
- **macOS**：`universal-apple-darwin`（同时支持 Apple Silicon M 系列 + Intel），产物 `.dmg` + `.app.zip`
- **Windows**：x64，产物 `.msi` + NSIS `-setup.exe` + portable `.exe`

产物命名约定：`SyncCopy-v<version>-<platform>-<variant>.<ext>`（例：`SyncCopy-v0.2.0-macos-universal.dmg`）

### 7. 严格 SDLC — Spec + ADR + 153 单元测试

本版本按完整 SDLC 流程交付：
- 20 份 feature spec（specs/ 目录）
- 7 份技术决策 ADR（decisions/ 目录，ADR-001 ~ ADR-011）
- **153 单元 + 集成测试全过**（142 lib 单测 + 11 集成测试，含 N=3 gossip mesh 三机场景）
- 每个 feature 经过 code-reviewer review + qa-tester 测试通过方能合入

---

## 安全说明

v0.2.0 完整闭环了 8 条必修安全项（`decisions/ADR-008` MUST-1 ~ MUST-8）：

| 必修项 | 内容 |
|--------|------|
| MUST-1 | AAD 绑值全闭环：防跨 kind/peer/seq 三维重放 |
| MUST-2 | 密钥内存清零：`Zeroizing<[u8;32]>` drop 时自动清零 |
| MUST-3 | 403 通用 body：ban/未知/拒绝路径返同一 body，防 device_id 可枚举 |
| MUST-4 | PeerRegistry.remove 原子顺序：inner.remove → client_pool.remove 严格顺序 |
| MUST-5 | panic message 静态字面量：不含变量插值，防运行时敏感数据进 crash 报告 |
| MUST-6 | /file seq dedupe：补 v0 遗漏的重放保护 |
| MUST-7 | handshake DoS 限流：per-pair 60s ≤3 次，全局 60s ≤10 个不同 device_id |
| MUST-8 | device_name sanitize：Bidi 控制字符黑名单 + 控制字符过滤 + 64 codepoints 上限 |

注意：v2 build 与 v0 prototype **不互通**（HKDF salt 从 v1 bump 到 v2，设计选择，见 `decisions/ADR-011`）。

---

## 下载

> CI 跑完后由用户/维护者填写下载链接。

| 平台 | 文件 | 说明 |
|------|------|------|
| macOS (Apple Silicon + Intel) | `SyncCopy-v0.2.0-macos-universal.dmg` | 安装包（推荐） |
| macOS portable | `SyncCopy-v0.2.0-macos-universal-portable.app.zip` | 解压即运行，无需安装 |
| Windows x64 (NSIS) | `SyncCopy-v0.2.0-win-x64-setup.exe` | 安装包（推荐） |
| Windows x64 (MSI) | `SyncCopy-v0.2.0-win-x64.msi` | MSI 安装包 |
| Windows x64 portable | `SyncCopy-v0.2.0-win-x64-portable.exe` | 免安装双击即运行 |

**注意**：本版本未经代码签名（v2 不申请开发者证书）。
- macOS：首次启动需右键 → 打开，或在系统设置"安全性与隐私"中手动允许
- Windows：SmartScreen 警告选"仍然运行"

---

## 已知限制

1. **仅 PNG 图片走剪切板通路**：截图同步仅支持 PNG 格式；其他图片格式（JPEG/TIFF 等）暂不支持（设计决策，避免编解码依赖膨胀）
2. **1MB 文本上限**：剪切板文本内容超过 1MB 将被跳过不同步
3. **group-approval 首次响应者优先（handshake-dismissed）推迟到下一版本**：当前实现中，任一在线设备同意入组即全组生效（分布式审批），但同时多个弹框同时 dismiss 的确认流程完整工作流推迟到独立 group-approval feature（v0.3.0 计划）

---

## v0 Prototype 教训（v2 重写的起点）

v0 prototype 踩过三类坑，v2 直接将教训编进了 spec/ADR：

1. **隐形掉线**：v0 长时间运行后出现"表面在线但实际无法同步"现象。v2 通过心跳 force_rebuild + `last_successful_sync_at` 真实写入根治（`decisions/ADR-009` 第 3.5 节，`specs/peer-heartbeat.md` 第 1.1 节）。

2. **单文件膨胀**：v0 `+page.svelte` 1483 行、`network/server.rs` 784 行，隐式不变式无文档。v2 拆分为 8 个 Svelte 5 组件 + 7 个 handler 子文件，每个文件 ≤ 250 行约束（`decisions/ADR-003` 第 3.1 节）。

3. **零测试覆盖**：v0 所有决策散落在 commit message 和会话记忆里，0 单元测试，0 spec，改一处会意外 break 其他。v2 从第一行代码起就有 spec + ADR + test 三层保障，153 条测试全过。

---

## 变更链接

完整变更记录见 [CHANGELOG.md](./CHANGELOG.md) `[0.2.0]` 段。

技术决策记录见 `decisions/` 目录（ADR-001 ~ ADR-011）。

---

*由 release-engineer agent 生成 — 2026-05-13*
