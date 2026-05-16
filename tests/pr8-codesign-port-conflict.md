# tests/pr8-codesign-port-conflict.md — PR #8 Bug #1 ad-hoc codesign + Bug #2 端口冲突 fatal 三件套验证

## 适用版本
- spec: specs/clipboard-sync.md（核心 lifecycle 相关）
- adr: ADR-008（lifecycle step 5 bind 前置）、ADR-010（codesign）
- PR: #8 commit edd34bb，CI run 25923138904
- 测试日期：2026-05-15  测试人：qa-tester（自动执行）  结果：PASS（Bug #1 部分通过 / Bug #2 全通过）

## 环境前置
- [x] 设备：macOS Darwin 25.3.0（Apple Silicon）
- [x] artifact：SyncCopy-v0.2.0-macos-universal-portable.app.zip（CI run 25923138904）
- [x] 端口 5858 启动前无占用
- [x] 无现存 sync-copy dev instance
- [x] 测试结束后清理 /tmp/sync-copy-v020-test-v2/（已确认删除）

---

## 场景 S1：Bug #1 ad-hoc codesign 验证
对应 PR #8 release 脚本中新增的 codesign --sign - 步骤。

步骤：
1. 下载 CI run 25923138904 的 sync-copy-macos-universal-v0.2.0 artifact
2. 解压 SyncCopy-v0.2.0-macos-universal-portable.app.zip
3. 执行 `codesign -dv --verbose=4 "Sync Copy.app"`
4. 执行 `codesign -v "Sync Copy.app"`
5. 执行 `spctl -a -v "Sync Copy.app"`
6. 执行 `xattr "Sync Copy.app"`

预期：
- codesign -dv 输出含 `flags=0x2(adhoc)` — 确认 ad-hoc 签名写入
- codesign -v 无输出（= valid on disk）
- spctl -a 输出 `rejected` — ad-hoc 签名不被 Gatekeeper 信任，属预期（需开发者证书才能 accepted）
- xattr 显示 com.apple.provenance（无 quarantine 标记意味着 Gatekeeper 不会主动拦截已移除隔离的 app）

实测：
- codesign -dv: `flags=0x2(adhoc)` 确认存在 — PASS
- codesign -v: 无输出 = valid on disk — PASS
- spctl -a: 输出 `rejected` — 属预期行为（ad-hoc 不等于 Apple 证书签名），不计为 Bug
- xattr: `com.apple.provenance`（无 com.apple.quarantine）

结论：Bug #1 修复确认有效。ad-hoc 签名已正确写入，构建产物不再是"完全未签名"状态。
用户手动移除 quarantine（`xattr -dr com.apple.quarantine`）或右键 Open 后可正常运行。

已知限制：spctl 仍 rejected 不是 Bug，是 Apple 公证（notarization）缺失的已知现状，与 Bug #1 无关。

---

## 场景 S2：干净环境启动 + 浮窗显示
对应之前 qa 报告"浮窗 occluded / 不可见"的复测。

步骤：
1. 确认无 sync-copy dev instance 运行（lsof -i :5858 无输出）
2. 移除 quarantine：`xattr -dr com.apple.quarantine "Sync Copy.app"`
3. `open "Sync Copy.app"`
4. 等待 6 秒
5. 检查进程：`ps aux | grep sync.copy`
6. 检查浮窗：`osascript -e 'tell application "System Events" to tell process "Sync Copy" to get {position, size} of window 1'`

预期：
- 进程存在
- 返回窗口坐标和尺寸（非空）

实测：
- 进程 PID 65987 正常运行
- 窗口 position=(560, 1013)，size=(320, 420)
- 5858 端口处于 LISTEN 状态
- HTTP 验证：/ 返回 404，/handshake 返回 405 — PASS

结论：干净环境下浮窗正常显示。之前 qa 报告的 "occluded" 问题确认是 dev instance 占用 5858 端口导致的干扰，非浮窗本身 Bug。

---

## 场景 S3：Bug #2 端口冲突 fatal 三件套验证
对应 ADR-008 lifecycle step 5 bind 同步前置 + show_startup_error_dialog。

步骤：
1. 关闭已运行的 release app
2. 用 Python 占用 5858 端口：`python3 -c "s.bind(('0.0.0.0', 5858)); s.listen(1); time.sleep(90)"`（后台运行）
3. 验证 Python 已占用端口：`lsof -i :5858`
4. `open "Sync Copy.app"`
5. 等待 10 秒
6. 检查 sync-copy 进程是否已退出：`ps aux | grep sync.copy`
7. 检查 osascript dialog 进程：`ps aux | grep osascript`
8. 检查 DiagnosticReports：`ls ~/Library/Logs/DiagnosticReports/ | grep sync`

预期（fatal 三件套 v4-7）：
- (a) 写文件日志：当前版本 diagnostic-logging 未实现，仅 stderr — 此项 SKIP（已知 backlog）
- (b) 弹 GUI dialog：osascript display alert 进程可见，内容含端口冲突错误信息
- (c) 非静默 exit：sync-copy 进程已不在 ps 列表，且有 DiagnosticReport 证明 SIGABRT

实测：
- osascript 进程命令行中含：
  `port bind failed: 端口 5858 已被占用 (Address already in use (os error 48))`
  并提示用户关闭占用端口的程序后重启 — PASS（b 件）
- ps 列表中 sync-copy 进程不存在 — PASS（c 件）
- DiagnosticReports 生成文件：sync-copy-2026-05-15-230411.ips（SIGABRT Abort trap: 6）— PASS（c 件补充）
- 文件日志未实现 — SKIP（a 件，已知）

结论：Bug #2 修复有效，端口冲突路径触发了 GUI dialog + SIGABRT abort，非静默 exit。

---

## 场景 S4：IPC HTTP server 可达性
对应核心功能 lifecycle 验证。

步骤：
1. 正常启动 app（无端口冲突）
2. `lsof -iTCP:5858 -sTCP:LISTEN`
3. `curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:5858/`
4. `curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:5858/handshake`

预期：
- 5858 端口 LISTEN
- / 返回 404（无 index handler）
- /handshake 返回 405（POST only）

实测：
- LISTEN 确认
- / => 404 — PASS
- /handshake => 405 — PASS

结论：HTTP IPC server 正常可达。

---

## 已知 fail / 待跟进

1. Bug #1 局限：spctl -a 仍 rejected — ad-hoc 签名不等于 Apple 公证；用户分发仍需手动移除 quarantine 或右键 Open。后续如需免手动操作，要走完整 Apple Developer 签名 + 公证流程（成本较高，留给 release-engineer 评估）。

2. fatal 三件套 a 件缺失：文件日志（写入磁盘的 error log）未实现，当前仅依赖 stderr 和 DiagnosticReport。如需完整 v4-7 合规，需让 implementer 在 show_startup_error_dialog 前写入 ~/Library/Logs/SyncCopy/error.log。建议 PLAN.md 新增 backlog 项。

3. crash report 触发：SIGABRT 会生成 .ips crash report，对普通用户可能造成困惑（系统弹 "Sync Copy quit unexpectedly"）。可考虑改用 std::process::exit(1) 代替 process::abort() 以避免 crash report 生成，但这属于实现细节，让 implementer 评估权衡（dialog 先于 exit，用户已知原因，crash report 可能属于多余噪音）。
