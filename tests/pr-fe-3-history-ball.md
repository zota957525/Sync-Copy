# tests/pr-fe-3-history-ball.md — history-list + floating-ball v2.0 AC 手测

## 适用版本

- spec: specs/history-list.md 第 4 节（10 AC）/ specs/floating-ball.md 第 4 节（8 AC）
- adr: ADR-003 / ADR-009 / ADR-011
- PR: PR-7（lifecycle + handlers emit history-updated）/ PR-FE-3a（BALL_SIZE_PX=48 / FloatingHeader 拆分）
- 测试日期：____ 测试人：____ 结果：PASS / FAIL

## 环境前置

- [ ] 设备 A: macOS / 192.168.1.x（自填）/ 跑 `npm run tauri dev` 或安装 SyncCopy v0.1.0+
- [ ] 设备 B: Windows 10/11 或第二台 macOS / 192.168.1.y（自填）/ 同上
- [ ] 两机同一 WiFi / 有线 LAN（同一路由器）
- [ ] Mac 防火墙允许 `Sync Copy` 入站（或系统偏好关闭防火墙）
- [ ] Win 防火墙允许 5858 TCP 入站（若设备 B 为 Windows）
- [ ] 关闭 Clash / VPN 等 LAN 代理
- [ ] 两机互 ping 通：`ping 192.168.1.y`（A 上）/ `ping 192.168.1.x`（B 上）

---

## history-list 场景

### 场景 S10：本机复制 → A 浮窗历史实时刷新

对应 spec history-list.md 第 4 节 AC #1

步骤：

1. A 启动 SyncCopy，浮窗已显示（main view）
2. 在 A 上用系统剪切板复制纯文本 "hello AC1 test"
3. 在 1 秒内观察 A 的浮窗历史列表

预期：

- 历史列表顶部出现新条目，显示 "hello AC1 test"（line-clamp 2 截断或完整显示）
- meta 行显示 "本机 · 刚刚"
- 条目出现无需重新打开/隐藏浮窗（PR-7 emit history-updated 自动触发）

实测：（填）

---

### 场景 S11：A 复制 → B 浮窗历史实时刷新，含 device_name

对应 spec history-list.md 第 4 节 AC #2

前提：A/B 已握手并双向 Approved（参考 integration-pr6.md S1 步骤 1-4）

步骤：

1. 在 A 上复制纯文本 "cross device test 跨机测试"
2. 在 1 秒内观察 B 的浮窗历史列表

预期：

- B 历史列表顶部出现新条目，内容含 "cross device test 跨机测试"
- meta 行显示 "来自 [A 的 device_name] · 刚刚"
- device_name 与 A 的设置中配置的设备名一致

实测：（填）

---

### 场景 S12：单击历史条目 → 系统剪切板更新 + UI 反馈

对应 spec history-list.md 第 4 节 AC #3 / AC #4

前提：A 历史列表中至少有 1 条文本条目和 1 条图片条目（需先复制文本 + 截图）

步骤（文本）：

1. 在 A 历史列表中单击某条文本条目行主体（非 ✕ 按钮）
2. 打开文本编辑器，粘贴

预期（文本）：

- 粘贴内容与单击的历史条目文本完全一致
- 单击后条目右上角出现绿色 "已复制 ✓" chip，持续约 1.2s 后自动消失

步骤（图片，需先有图片条目）：

1. 截图（Cmd+Shift+4 等），使 A 历史列表出现图片条目
2. 单击图片条目
3. 打开 Preview/画图，粘贴

预期（图片）：

- 粘贴可得图片内容（PNG）

实测：（填）

---

### 场景 S13：删除 / 清空 / 50 条上限 / FIFO 截尾

对应 spec history-list.md 第 4 节 AC #7 / AC #8 / AC #9 / AC #10

步骤（单条删除）：

1. 历史列表有若干条目
2. 鼠标悬停某条 → 右上角出现 ✕ 按钮（颜色 #9ca3af）
3. 鼠标悬停 ✕ → 颜色变为 #ef4444（danger red）
4. 点击 ✕

预期（单条删除）：

- 该条目在约 50ms 内从列表消失，无动画（spec 第 6.4 节）
- history-updated 触发（列表刷新；其余条目无变化）

步骤（50 条上限）：

1. 向 A 快速复制 55 段不同文本（脚本循环或手动复制并等待同步）
2. 观察历史列表

预期（50 条上限）：

- 历史列表最多显示 50 条
- 第 51 条进入时最旧一条从底部消失（FIFO，MAX_HISTORY=50，spec 第 4 节 AC #8）

步骤（内容去重）：

1. 在 A 上连续复制完全相同的文本两次
2. 观察历史列表

预期（去重）：

- 历史只有 1 条该文本，位于顶部（spec 第 4 节 AC #9）

步骤（空态）：

1. 打开 settings，点"清空历史"
2. 关闭 settings，观察历史列表区域

预期（空态）：

- 历史区域显示居中文字 "还没有同步过" + "复制一段文本试试"（spec 第 4 节 AC #10）

实测：（填）

---

### 场景 S14（file 条目）：单击文件条目 → Finder/Explorer 显示文件

对应 spec history-list.md 第 4 节 AC #5 / AC #6

前提：需要先有文件传输功能（P2 file-transfer）；若 v2.0 未实现文件传输则标 "已知限制—file 条目仅 smoke"

步骤（有 saved_path）：

1. 接收到来自 B 的文件，历史列表出现 file 条目，状态 "已保存"
2. 单击该 file 条目行主体

预期（有 saved_path）：

- macOS：Finder 打开，该文件被高亮选中
- Windows：资源管理器打开，文件被高亮选中

步骤（无 saved_path / 失败状态）：

1. 在历史列表中找到状态 = "保存失败" 或 saved_path 为空的 file 条目
2. 单击该条目

预期（无 saved_path）：

- 无系统文件管理器打开；条目行内显示红色 banner "路径不可用"（1.5s 自消失）

实测：（填 / 已知限制：v2.0 file transfer 未在 v2.0 范围内，file 条目手测推迟到 P2）

---

## floating-ball 场景

### 场景 S15：点 − 按钮 → 折叠为 48x48 球

对应 spec floating-ball.md 第 4 节 AC #1

步骤：

1. 浮窗处于 main view 状态
2. 点击顶部状态栏最左侧的 `−` 按钮（FloatingHeader oncollapse）

预期：

- 窗口**立即**（无动画延迟）变为 48x48 圆形小球
- 球显示 app icon SVG（双箭头 logo），居中，无文字/状态点
- 球位置在折叠前的窗口中心附近（窗口缩小到 48x48，中心不大幅偏移）
- 浮窗内容区（历史列表/footer/状态栏）不可见

实测：（填）

---

### 场景 S16：单击球 → 展开回记住的尺寸 + 视口校正

对应 spec floating-ball.md 第 4 节 AC #2 / AC #4

步骤（基本展开）：

1. 从 S15 折叠后的球状态开始
2. 单击球（鼠标按下后移动 <= 8px，抬起 < 1500ms）

预期（基本展开）：

- 窗口立即展开回 320x420（默认）或用户上次调整的尺寸
- main view 内容正常显示（历史列表 / 状态栏 / footer）

步骤（记住尺寸）：

1. 展开浮窗后，手动拖拽窗口边角将其调整为更大尺寸（如 320x600，若 Tauri 允许）
2. 点 − 折叠为球
3. 单击球展开

预期（记住尺寸）：

- 展开尺寸为步骤 1 中调整的 320x600（lastExpandedSize 正确记录）

步骤（视口校正）：

1. 将球拖到屏幕边角（甚至超出屏幕外 30% 以上）
2. 单击球展开

预期（视口校正）：

- 展开后窗口完全在当前显示器内可见（expandFromBall 视口校正生效）
- 不出现部分或完全超出屏幕外的情况

实测：（填）

---

### 场景 S17：球拖动 → 跟随鼠标移动，松开不展开

对应 spec floating-ball.md 第 4 节 AC #3

步骤：

1. 处于球形态
2. 鼠标按下球并**移动 > 8px**，持续拖动到屏幕另一位置
3. 松开鼠标

预期：

- 球跟随鼠标移动（startDragging 原生窗口拖动）
- 松开后球停在新位置，**不展开**（手势识别为拖动，非点击）
- cursor 在拖动中显示 grabbing

实测：（填）

---

### 场景 S18：重启后回展开态（球状态不持久化）

对应 spec floating-ball.md 第 4 节 AC #5

步骤：

1. 折叠为球状态
2. 完全退出（不是托盘隐藏，而是退出进程）
3. 重新启动 SyncCopy

预期：

- 启动后显示 main view（展开态，320x420 默认尺寸）
- 不恢复球形态（球状态不持久化，spec 第 4 节 AC #5 + 第 3 节 out of scope）
- 位置回到屏幕中央

实测：（填）

---

### 场景 S19：多显示器拖动 → 球跟随，展开在当前显示器内

对应 spec floating-ball.md 第 4 节 AC #6

前提：需要双显示器环境

步骤：

1. 球形态，停在主屏幕
2. 拖动球从主屏到副屏
3. 单击球展开

预期：

- 球跟随到副屏（OS 级窗口跨屏移动）
- 展开后窗口在副屏（球所在的显示器）内完全可见，视口校正参考副屏尺寸

实测：（填 / 如无双显示器环境标 "跳过"）

---

### 场景 S20：球与展开浮窗共用同一窗口实例，无新窗口

对应 spec floating-ball.md 第 4 节 AC #7

步骤：

1. 打开浮窗，折叠为球，再展开，重复 3 次
2. 每次折叠/展开期间观察 Dock（macOS）或任务栏（Windows）

预期：

- 应用 Dock 图标始终只有 1 个窗口实例（label=main）
- 无额外窗口被创建

实测：（填）

---

### 场景 S21：托盘 hide/show 时球形态保持

对应 spec floating-ball.md 第 4 节 AC #8

步骤：

1. 将浮窗折叠为球形态
2. 通过托盘左键单击隐藏（hide）
3. 再次托盘左键单击显示（show）

预期：

- show 后恢复球形态（不自动展开为 main view）
- 球位置与隐藏前一致（show 时调用 ensure_on_screen，若球在屏外则拉回）

实测：（填）

---

## 已知限制 / 待跟进

- file 条目手测（S14）：v2.0 不含 file-transfer，推迟到 P2 file-transfer spec 实现后验证
- 多显示器 S19：需物理双屏环境，无条件则跳过并标注
- floating-ball 视口校正双实现（前端 expandFromBall + 后端 ensure_on_screen）：PR-FE-3 review 第 9.2 节 [中等] 2 标记的架构债，v2.1 统一到 backend；当前前端实现功能正确，手测以功能是否正确为准
- historyStore.error 无 UI 消费（PR-FE-3 review 第 9.3 节 (d)）：删除/清空失败时用户无感知；v2.1 列入 PR-FE-4 修复，当前手测不做错误路径的 UI 反馈场景
