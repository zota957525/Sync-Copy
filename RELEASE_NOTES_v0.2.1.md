# Sync Copy v0.2.1 — Patch Release

**发布日期**：2026-05-16

本版本是 v0.2.0 的 patch 修复版，针对用户实测发现的 4 个阻断性 bug 全部修复。
如果你正在使用 v0.2.0，**强烈推荐立即升级**——v0.2.0 的"加入小组"功能因 IPC 参数名 bug 而完全失效。

---

## 主要修复

### 1. 加入小组按钮无反应（最高优先级）

v0.2.0 的"加入小组"按钮是前端占位符，点击后没有任何反应。

修复内容：
- 新建 `JoinDialog` 组件，提供完整的输入框 + 确认流程
- `FloatingWindow` 增加 'join' view state，正确路由到 JoinDialog
- 前端 `join_group` / `approve_peer` / `reject_peer` 三处 IPC 调用参数名由 camelCase 改为 snake_case，与 Rust 后端一致（Tauri 2 不做自动驼峰转换）

影响范围：所有平台。v0.2.0 用户无法使用加入功能，升级后即可正常使用。

### 2. macOS Gatekeeper 拒绝打开（macOS 用户必读）

v0.2.0 的 macOS 产物未经代码签名，首次打开会被 Gatekeeper 拒绝（提示"无法打开，因为 Apple 无法检查是否包含恶意软件"）。

修复内容：
- CI 构建流程增加 ad-hoc codesign 步骤（`codesign --deep --force --sign -`），消除 Gatekeeper 拒绝
- `使用说明.md` FAQ Q6 补充了三种放行方式（右键打开 / xattr 命令 / 系统设置手动允许）

注意：Apple Developer 正式公证仍未完成（需 Apple Developer Program 账号）。ad-hoc 签名解决了 Gatekeeper 的"未签名"拒绝，但用户仍需在首次运行时在系统设置中手动允许一次。

### 3. 端口冲突静默失败（fatal 三件套完整化）

v0.2.0 在 HTTP server 绑定端口失败时，进程会静默退出，用户界面无任何提示，也没有日志可查。违反 v4-7 fatal error 三件套规范。

修复内容：
- lifecycle step 5 的 bind 操作改为同步前置检测
- 端口冲突时弹出 `show_startup_error_dialog` 用户可见对话框
- 以 `process::exit(1)` 退出（非静默），并写入文件日志
- 新增 fatal error 文件日志：`~/Library/Application Support/com.synccopy.SyncCopy/logs/error.log`（完成 v4-7 三件套 a 件）

### 4. macOS 系统"意外退出"crash report 噪音

v0.2.0 在 fatal path 使用 `process::abort()`，macOS 系统会将其记录为崩溃并弹出"意外退出"系统报告弹框，对用户造成误导。

修复内容：
- lifecycle fatal path 改为 `process::exit(1)`，macOS 系统不再弹出崩溃报告
- panic hook 内部仍保留 `abort()`，以确保 panic 时有完整 backtrace 可查

---

## 升级说明

从 v0.2.0 升级：

1. 下载对应平台的 v0.2.1 安装包（见下方 Artifacts）
2. 直接安装覆盖，无需卸载旧版本
3. macOS 用户：首次运行仍需在"系统设置 > 隐私与安全性"中手动允许一次

**v0.2.0 → v0.2.1 兼容性**：协议层无任何变化，v0.2.1 节点可与未升级的 v0.2.0 节点组成同一小组并正常同步。

---

## 已知限制

- Apple Developer 正式公证未完成（_assumptions A29）。ad-hoc 签名已消除 Gatekeeper 拒绝，但仍需用户首次手动放行。
- Windows 端本次 IPC camelCase 修复同样生效，但 Windows artifact 在 v0.2.1 CI build 完成后方可确认。

---

## Artifacts

| 平台 | 文件 |
|------|------|
| macOS (universal) | `SyncCopy-v0.2.1-macos-universal.dmg` |
| Windows x64 | `SyncCopy-v0.2.1-windows-x64-setup.exe` |

---

## 关联 spec / ADR

- ADR-010 第 3.6 节：lifecycle fatal path 规范
- v4-7 fatal error 三件套（写文件日志 + GUI dialog + 非静默 exit）
- _assumptions A29：Apple 代码签名 / 公证假设
