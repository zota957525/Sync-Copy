# tests/integration-pr6.md — Backend MVP 双机集成手测

## 适用版本

- spec: specs/clipboard-text-sync.md / specs/peer-heartbeat.md / specs/group-discovery.md / specs/e2e-encryption.md
- adr: ADR-008 / ADR-009 / ADR-010 / ADR-011
- 测试日期：____ 测试人：____ 结果：PASS / FAIL

## 环境前置

- [ ] 设备 A: macOS / 192.168.1.x（自填）/ 跑 `npm run tauri dev` 或安装 SyncCopy v0.1.0+
- [ ] 设备 B: Windows 10/11 / 192.168.1.y（自填）/ 同上
- [ ] 两机在同一 WiFi / 有线局域网（同一路由器下）
- [ ] Mac 防火墙：系统偏好 → 安全性 → 防火墙 → 允许 `Sync Copy` 入站（或关闭防火墙）
- [ ] Windows 防火墙：允许 5858 端口入站（`netsh advfirewall firewall add rule name="SyncCopy" dir=in action=allow protocol=TCP localport=5858`）
- [ ] 关闭 Clash / VPN 等 LAN 代理（防止 LAN 请求被劫持；no_proxy() 应已内置，但代理可能修改路由）
- [ ] 确认两机能互 ping：A 上 `ping 192.168.1.y`，B 上 `ping 192.168.1.x` 均通

---

## 场景 S1：双机握手 + 文本同步（ASCII + Unicode + Emoji）

对应 spec: clipboard-text-sync.md 第 4 节 AC #1 #2

步骤：
1. A 启动 SyncCopy，浮窗显示本机 IP:PORT（如 `192.168.1.50:5858`）
2. B 启动 SyncCopy，点击「加入」，输入 A 的 IP:PORT，点「加入」
3. A 出现审批弹窗，点「同意」
4. 验证：A、B 浮窗均显示 `小组 · 2 台`
5. 在 A 上复制纯 ASCII 文本：`Hello World 123`
6. 验证：B 上 `Ctrl+V` 粘贴，得到 `Hello World 123`
7. 在 A 上复制中文文本：`你好世界，这是剪切板同步测试`
8. 验证：B 上粘贴得到相同中文文本（UTF-8 透明）
9. 在 A 上复制含 Emoji 的文本：`Hello 🎉🚀 你好`
10. 验证：B 上粘贴内容与 A 完全一致（包含 Emoji）
11. 在 B 上复制文本 `From B to A` → A 上粘贴验证（双向同步）

预期：
- 步骤 4：双方状态点变绿，`小组 · 2 台`
- 步骤 6、8、10：B 粘贴内容与 A 复制内容字节级一致
- 步骤 11：A 收到 B 的文本，双向均正常

实测：（填）

---

## 场景 S2：三机 gossip 自动扩展

对应 spec: group-discovery.md 第 4 节 AC #2（PR-7 / commit 86ac6ac 已实现）

步骤：
1. A、B 已在同一小组（`小组 · 2 台`）
2. C 启动 SyncCopy，点「加入」，输入 **B 的** IP:PORT（也可输入 A 的，效果相同）
3. B 侧审批通过
4. 等待 ≤ 5 秒
5. 验证：A、B、C 三台均显示 `小组 · 3 台`

gossip 路径说明（PR-7 实现，PR-7a commit 86ac6ac）：
- C dial B → B 的 HandshakeResp.peers 含 A 的 stub → C 自动 gossip_dial_stub 向 A 发起握手 → C.peers 加入 A
- C dial B 完成后 B 调 broadcast_announce(C) → A 收到 announce → A spawn dial_handshake(C.addr) → A.peers 加入 C
- 结果：三机均互相 Approved，`小组 · 3 台`

预期：
- B 审批通过后 ≤ 5 秒内，A、B、C 三台均显示 `小组 · 3 台`
- C 的 peers 包含 A（gossip_dial_stub 路径）
- A 的 peers 包含 C（broadcast_announce 路径）
- 三机均互相 Approved（六个方向均连通）

实测：（填）

---

## 场景 S3：错误密码 / 密钥修改 → 解密失败

对应 spec: clipboard-text-sync.md 第 4 节 AC #6 / e2e-encryption.md 第 4 节 AC #3

步骤：
1. A、B 已握手（`小组 · 2 台`）
2. 在 B 侧手动修改内存中 A 的 aes_key（需通过调试工具或重写测试；此步骤在普通安装版本中需模拟）
   替代方案：B 重启进程（密钥丢失），A 不重启，然后 A 在旧 peer 列表仍有 B 的情况下发文本
3. A 复制文本 `Test tamper`
4. 验证：B 侧收到后解密失败，**不**写入 B 的剪切板；B 上 `Ctrl+V` 粘贴的是旧内容而不是 `Test tamper`
5. 日志中（B 侧）出现解密失败相关 warn/error 日志

预期：
- B 剪切板不被写入（AC #6：解密失败不写入剪切板、不进历史）
- B 侧 tracing 日志包含 `DecryptFailed` 或 `422` 相关信息

实测：（填）

---

## 场景 S4：自连拒绝（device_id 相同）

对应 spec: group-discovery.md 第 4 节 AC #7 / ADR-008 MUST-3

步骤：
1. A 启动 SyncCopy，复制本机显示的 IP:PORT（如 `192.168.1.50:5858`）
2. A 上点「加入」，输入 A 自己的 IP:PORT（自连）
3. 点「加入」

预期：
- 出现错误提示，内容类似「对方拒绝了你的加入请求」或「连接失败」
- A 浮窗仍显示 `小组 · 0 台`（或之前数量不变），不会把自己加入 peers 列表

实测：（填）

---

## 场景 S5：隐形掉线检测（防火墙 block 心跳端口）

对应 spec: peer-heartbeat.md 第 4 节 AC #1（≤ 25s 剔除）/ 第 1.1 节 隐形掉线

步骤：
1. A、B 已连接（`小组 · 2 台`）
2. 在 B 上用系统防火墙规则临时 block 5858 端口入站：
   - Windows：`netsh advfirewall firewall add rule name="BlockSyncCopy" dir=in action=block protocol=TCP localport=5858`
   - Mac：在「安全性 → 防火墙」中 block，或用 `pf` 规则
3. 等待 ≤ 25 秒（2 次心跳失败 × 10s + 5s 处理余量）
4. 验证：A 浮窗变为 `小组 · 1 台`（B 被剔除）
5. 移除防火墙规则，B 重启 SyncCopy，重新加入
6. 验证：A 恢复 `小组 · 2 台`

预期：
- 步骤 4：A 在 25s 内检测到 B 不可达，从 peers 表移除
- 步骤 6：重新握手后恢复正常

实测：（填）

---

## 场景 S6：Leave 离线广播 → 对端及时移除

对应 spec: peer-heartbeat.md（leave 与 heartbeat 双层防御）/ ADR-010 第 3.3 节 step 3

步骤：
1. A、B 已连接（`小组 · 2 台`）
2. 在 B 上正常退出 SyncCopy（点设置 → 退出，或 Cmd+Q / Alt+F4）
3. 验证：A 浮窗在 ≤ 2 秒内变为 `小组 · 1 台`

预期：
- B 退出时发送 leave 广播（ADR-010 shutdown step 3）
- A 收到 leave 后立即移除 B（不等心跳超时）
- A 浮窗 ≤ 2s 响应（leave 广播 + 处理时间）

实测：（填）

---

## 场景 S7：超大文本跳过广播（≥ 1 MB）

对应 spec: clipboard-text-sync.md 第 4 节 AC #5

步骤：
1. A、B 已连接
2. 在 A 上创建并复制一段超过 1 MB 的字符串（如用 Python：`python3 -c "import pyperclip; pyperclip.copy('a' * 1100000)"`，或手写文本文件 > 1 MB 后全选复制）
3. 等待 2 秒

预期：
- B 不收到任何内容（A 不广播超长文本）
- A 侧 tracing 日志出现 `skip oversized` 或 `MAX_TEXT_BYTES` 相关 debug/info 日志
- 不报错、不崩溃（静默跳过）

实测：（填）

---

## 场景 S8：重启程序 → 重新加入组

对应 spec: clipboard-text-sync.md 第 4 节 AC #8（重启后重新握手）

步骤：
1. A、B 已连接（`小组 · 2 台`）
2. A 上退出 SyncCopy，B 侧在 ≤ 25 秒内检测到 A 掉线（`小组 · 1 台`）
3. 重新启动 A 的 SyncCopy
4. A 在浮窗点「加入」，输入 B 的 IP:PORT，B 侧同意（或 approved_device_ids 白名单自动通过）
5. 验证：A、B 恢复 `小组 · 2 台`，正常文本同步

预期：
- 重启后 A 生成新 UUID device_id（或复用旧的，取决于持久化策略）
- 重新握手后双方派生新的 AES 密钥（前向保密）
- 文本复制同步正常

实测：（填）

---

## 场景 S9：关闭程序 → shutdown ≤ 2800ms

对应 spec: ADR-010 第 3.3 节（总 deadline ≤ 2800ms）

步骤：
1. A、B 已连接
2. 在 A 上点「退出」
3. 用秒表计时从点击到进程消失的时间

预期：
- 进程在 ≤ 2.8 秒内消失
- 期间 B 收到 A 的 leave 信号（≤ 1.5s leave 广播）

实测：（填）耗时：____ms

---

## 已知 fail / 待跟进

- gossip（S2）：PR-7（commit 1d9e41a）+ PR-7a（commit 86ac6ac）已完整实现，
  集成测试 test_three_instance_gossip_mesh 9 pass 验证（P5-2）。S2 预期 PASS。
- 前端 UI（`小组 · N 台` 状态显示）依赖前端 Tauri command 接入，backend 测试用 tracing 日志验证状态变化
- clipboard_apply_tx 接收侧：文本同步 S1 步骤 6 能通过的前提是 arboard 线程正常消费明文，
  若 arboard 在 CI headless 环境 init 失败，S1 B 侧仅能验证 handler 返 200 而不验证剪切板写入
