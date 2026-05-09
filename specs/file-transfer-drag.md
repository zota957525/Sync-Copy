---
status: SPEC_REVIEWED
owner: product-strategist
related_adrs: [ADR-003, ADR-011]
related_specs: [00-product-overview, e2e-encryption, group-approval, group-discovery, clipboard-image-sync]
created: 2026-05-06
updated: 2026-05-08
revised: 2026-05-08 — ADR-003 第 3.2 节 答 第 7 节 [P1] 非 PNG 路由（用户拖文件走 /file，剪切板模块不主动检测）；状态码 413/422/500/408 统一规约
priority: P1
revision_history:
  - version: v1
    date: 2026-05-06
    notes: 初版 SPEC_DRAFTED，单文件上限 50 MB
  - version: v2
    date: 2026-05-08
    notes: 用户校对 _assumptions A16，单文件上限 50 → 5 MB（LAN 同步剪切板配套场景下 5 MB 更合理，与图片上限对齐）；联动 A14 增加非 PNG 图片格式路由问题给架构师
  - version: v3
    date: 2026-05-08
    notes: ADR-003 锁定 — /file 状态码 413/422/500/408 进 NetworkError → IntoResponse 一处映射；非 PNG 由用户拖文件兜底（不在剪切板模块自动检测）
---

# file-transfer-drag — 拖文件到浮窗发送给所有受信任设备（≤5 MB / 接收审批 / Downloads 落盘）

## 1. 问题（为什么做）

用户场景：在 Mac 上 Finder 里看到一份 PDF，需要传到 Win 看一眼——不想发邮件、不想登微信、不想插 U 盘、不想开 AirDrop（Win 不支持）。Sync Copy 给出"拖到浮窗 → 自动发到所有设备 → 接收方点同意 → 落 Downloads"的极简路径（00 总览 第 1 节 / 用户故事 #3 小文件递送）。

工程门槛：浏览器拖放 API 给出的是文件 metadata，真实路径需 Tauri webview drop event 提供；文件名要做 sanitize（防 `../../etc/passwd` 路径注入）；接收必须**用户审批**（不能自动写盘——同 LAN 攻击者发恶意文件的防御）；5 MB 是设计选择 + DoS 防御；命名冲突时附加 `_1` `_2`；接收成功要在系统文件管理器能 reveal。

## 2. 用户故事

- As a Mac user, I want to drag a sub-5MB PDF onto the floating window and trigger a popup on all my other devices asking "save this file?"; whoever clicks save gets it in Downloads, so that I skip email-to-self / chat-to-self.
- As a recipient, I want to see who sent the file, what it's called, and how big it is **before** the bytes hit my disk, so that I can refuse a malicious or unwanted file.
- As a sender, I want to see "delivered N/M devices" feedback after the drop, so that I know which targets actually saved it.
- As a user, I do not want a hostile filename like `../etc/passwd` or `con.txt` to break out of Downloads or crash my OS, so that filename sanitization is non-negotiable.

## 3. 范围

**in scope**：
- **前端拖拽入口**：浮窗整窗体接收 webview drop event（Tauri 2 的 `tauri://drag-drop` event）→ 收到 `paths: Vec<String>` → 调 `send_files` 命令
  - drop 期间显示 `dragOver` 视觉提示（中央卡片 `松开即发送给所有设备 · 单文件 ≤ 5 MB`）
- **后端 `send_files(app, paths) -> Result<String, String>` 命令**：
  - 校验有 ≥ 1 peer，否则返回 `还没有连接的设备`
  - 对每个 path：检查是文件、读 metadata、size ≤ `MAX_SEND_SIZE = 5 * 1024 * 1024` 否则跳过并加报告行 `<file>: 超过 5MB 上限`
  - `tokio::fs::read(&path)` 读全字节 → `sha256_hex(&bytes)` 算 content_hash → `network::client::broadcast_file(state, filename, bytes)` 返回 `(ok, total)` 计数
  - 发送端**也**写 history（`push_file` Source::Local，file_status = "sent"，saved_path = 源路径）
  - 返回多行报告字符串如 `report.pdf: 已送达 2/3 台\n other.png: 超过 5MB 上限`
- **协议** `FileReq { origin_device_id, origin_device_name, seq, filename, size, nonce, ciphertext }`：
  - 整个文件字节经 AES-256-GCM 加密为单条 ciphertext（base64）
  - `size` 是明文字节数（接收端校验 `plaintext.len() == size`，不一致返 400）
  - `filename` 是发送端展示名（接收端再做 sanitize）
- **`/file` 接收 handler**（`network/server.rs::handle_file`）：
  - 校 origin_device_id 在 `peer_keys`（已加密信任）→ 否则 403
  - 校 `req.size <= MAX_FILE_SIZE = 5 * 1024 * 1024`，否则 413（PAYLOAD_TOO_LARGE）
  - AES-GCM decrypt → 校 `plaintext.len() == size`，不符 400
  - `sanitize_filename(&req.filename)`：取 basename → 过滤 `/ \\ \0` → 限制 ≤ 200 字符 → 空串回退 `"file"`
  - 生成 `request_id` UUID → 插 `pending_file_saves: HashMap<String, PendingFileSave>` → emit `file-pending` 事件 → 浮窗弹**接收审批弹框**（与 `group-approval` 弹框是**同一覆盖层组件**但不同语义；样式见 第 6 节）
  - 接收审批超时：暂定 30 秒（v0 沿用同 `APPROVAL_TIMEOUT` 与握手审批共用）→ 408；待 ADR 决定是否拆为独立常量 `FILE_APPROVAL_TIMEOUT = 60s`（见 第 7 节 [P1] [安全]）
  - 用户拒绝 → 403（不写盘、history 不记 file 条目）
  - 用户同意：找 Downloads 目录（`directories::UserDirs::download_dir`，找不到回退 `std::env::temp_dir`）→ `unique_path` 防覆盖（追加 `_1` `_2` ... 直到 1000，再 fallback uuid 后缀）→ `tokio::fs::write` 落盘 → 算 content_hash → `history.push_file(filename, size, Some(saved_path), "received", ..., Source::Remote{device_name})` → emit `history-updated` + `file-saved { path, filename }`
  - 写盘失败 → history 仍记一条 `file_status = "failed"`，error 字段填 IO 错误，返 500
- **历史中文件条目**单击行为：发送端是 reveal 源路径；接收端是 reveal 已保存文件（不复制到剪切板，文件不入剪切板）
- **`reveal_file(path)` 命令**：mac 调 `open -R <path>`，win 调 `explorer /select,<path>`
- 客户端 reqwest builder 给文件传输用更长 timeout：`timeout(60s) + connect_timeout(5s)`（区别于剪切板的 5s/3s）

**out of scope**：
- 文件夹（仅单文件；递归打包属可选项 v3）
- 大于 5 MB 的文件（00 总览 第 3 节 已锁；分片续传与流式加密留 v3）
- 进度条（5 MB 在常见 LAN 1-3 秒完成；spinner 即可，无字节级进度）
- 文件类型限制（不限扩展名；恶意防御只靠用户审批 + filename sanitize）
- 自动 unzip / 预览（保存即结束，预览交给用户系统）
- 跨发送的"批量送达回执"（v0 只在 send_files 返回值里给一次性 report）

## 4. 验收标准（Definition of Done）

- [ ] A、B、C 三机已小组。在 A 上把 Finder 里一份 2 MB PDF 拖进浮窗 → B 与 C 浮窗**同时**弹"收到来自 A 的文件 report.pdf · 2.0 MB · 将保存到 Downloads"卡片
- [ ] B 点保存 → B 的 Downloads 出现 `report.pdf` 文件 + B 历史顶部出现"已保存"条目；点条目 Finder 高亮该文件
- [ ] C 点拒绝 → C 的 Downloads 不出现该文件 + C 历史不写 file 条目；A 的 send_files 报告显示 `report.pdf: 已送达 1/2 台`（B ok, C reject 计为失败）
- [ ] 在 A 上拖一个 7 MB 文件 → A 自身报告 `<file>: 超过 5MB 上限`；网络层不发请求；B/C 不弹框
- [ ] 攻击者构造发送 `filename = "../../tmp/evil"` → 接收端 sanitize 后实际写入 Downloads 下的 `evil`（不带路径），不会逃出 Downloads 目录
- [ ] 已存在 `report.pdf` 时再次接收同名文件 → 落盘为 `report_1.pdf`，不覆盖原文件
- [ ] B 30 秒内不点 → B 弹框消失 + A 报告 `report.pdf: 已送达 0/1 台`（408 计入失败）
- [ ] 接收方人为拔网线在解密成功后 + 写盘前 → history 写一条 `file_status = "failed"` + error 字段填磁盘错误，UI 仍能展示

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的
`legacy-prototype` 分支 `src-tauri/src/commands.rs::send_files`（约 380-440 行）：检查 `state.peers.count() == 0` → 错误；遍历 paths → metadata → size 校 `MAX_SEND_SIZE = 5*1024*1024` → `tokio::fs::read` 全字节读入内存 → `sha256_hex` → `broadcast_file` 返回 `(ok, total)` → 发送端 `history.push_file(..., "sent", ..., Source::Local)`。`network/client.rs::broadcast_file` 用独立 reqwest::Client（`timeout(60s) + connect_timeout(5s)`，区别于剪切板用的 build_client 5s/3s），对每 peer：取 peer_key、AES-GCM encrypt、构 `FileReq` POST `/file`，并发 join_all 收集成功数。`network/server.rs::handle_file`（约 630-740 行）：peer_keys.get → 403 if missing → size > MAX_FILE_SIZE → 413 → decrypt → 校 plaintext.len == size → sanitize_filename → 生成 request_id + insert `pending_file_saves` + emit `file-pending` → tokio::time::timeout(APPROVAL_TIMEOUT=30s) await rx → 用户决定后 unique_path（加 `_N` 防覆盖）→ `tokio::fs::write` → push_file Source::Remote → emit `file-saved`。失败也 push_file `failed` + error。`commands.rs::respond_file_save(state, request_id, accept)` Tauri 命令对接前端按钮 → `pending_file_saves.lock().remove(&rid).tx.send(accept)`。`reveal_file` 调系统命令。前端 +page.svelte 的 `pendingFiles` 数组承载 `file-pending` 事件，弹框模板独立于审批弹框（不同图标 📎 + 不同文案）。`unique_path` 函数：循环 1..1000 试 `<stem>_<i><ext>`，超出后 fallback uuid。`sanitize_filename`：取 basename + 过滤 `/ \\ \0` + 限长 200 + 空串回退。

### 5.2 v0 暴露的具体坑
- **整文件读入内存再加密**：5 MB × N peer 同时发 → 高峰 5N MB RAM。00 总览 第 5.4 节 已点名属"应挑战"
- **AES-GCM 单条 nonce 加密整个 5 MB**：非流式；GCM nonce-misuse 对单 key 重复 nonce 灾难，但每条消息独立随机 nonce + per-peer key，理论安全。仍是隐式约定无 ADR
- **filename sanitize 仅过滤 `/ \\ \0`**：未过滤 Win 保留名（`con` `nul` `aux`）—— 接收端在 Win 上写 `con.txt` 会报错走 failed 分支；不会落盘但用户体感是"莫名失败"
- **filename 限长 200 字符**：是字节数还是字符数（v0 是 `chars().take(200).collect()` —— 字符数）；多字节文件名（中文）会比 200 字节短，不撞文件系统上限
- **`unique_path` 1000 次循环**：理论 DoS（Downloads 已存在 999 个 `evil_N.pdf`），fallback 加 uuid 后缀（永不冲突）
- **审批超时 30s 与握手审批共用 APPROVAL_TIMEOUT**：耦合常量；调一处影响另一处
- **接收方 `Downloads` 找不到时回退 `std::env::temp_dir`**：用户感知不到落到了 /tmp，可能找不到文件
- **5 MB 字节限本地校验，但握手层无 body limit 校验**：axum DefaultBodyLimit 是 8 MB（5 MB 文件 + base64 33% 膨胀 + JSON）—— 攻击者发 `size = 100` 但实际 body 7 MB 仍然能通过 axum，只在 decrypt 后被 `plaintext.len() != size` 兜住。是冗余防御，但没 ADR 写明
- **发送端"已送达 N/M 台"是字符串拼接**返回给前端 alert 显示，不结构化；前端无法知道每台具体成功失败

### 5.2.1 v2 用户校对修正（2026-05-08）

PM 在写 `_assumptions.md` 时反向假设"v0 单文件上限 50 MB"（基于 LAN 较快、5 MB 似乎太保守的直觉）；用户校对（A16）明确否决：在 LAN 剪切板同步配套场景下，文件传输的核心用例是"发个 PDF / 截图 / 短视频片段"，5 MB 更贴合实际诉求。继续保持 v0 的 5 MB 硬上限有以下产品理由：① 与剪切板图片同步（`clipboard-image-sync`）的 5 MB 上限对齐，跨通路体感一致；② 5 MB 的"读全字节进内存再加密"方案可实现成本最低（v0 已验证），无需流式加密的复杂度；③ DoS 防御 + 用户对单次同步耗时的预期吻合（≈1-3 秒）；④ 大文件场景超出本工具定位（用户原意是"剪切板配套"，不是"通用文件传输"）。**结论**：v2 上限 = 5 MB（与 v0 一致），但理由从隐式经验值升级为产品决议层显式定锚。

### 5.3 v2 应继承
- 5 MB 单文件硬上限
- AES-256-GCM 整文件单条加密 + 独立 nonce
- 接收端必须用户审批（30s 超时 → 408）
- 落盘 Downloads 目录 + `unique_path` 命名冲突回避
- `sanitize_filename`（basename + 过滤特殊字符 + 限长）
- 协议 DTO `FileReq { origin_device_id, origin_device_name, seq, filename, size, nonce, ciphertext }`
- `pending_file_saves` 后端 + `file-pending` / `file-saved` 事件 + `respond_file_save` 命令
- 发送端独立 reqwest::Client 60s 长 timeout
- 历史 file_status 三值：`sent / received / failed`
- `reveal_file` 平台命令

### 5.4 v2 应挑战
- **流式加密 + 流式落盘**：避免 5 MB × N peer 并发的内存峰值（00 总览 第 5.4 节 列出）
- **filename sanitize 升级**：覆盖 Win 保留名（CON/NUL/AUX/COM*/LPT* 等）+ Unicode 反向覆盖字符 + 控制字符
- **审批超时常量解耦**：握手审批与文件审批分两个常量（用户场景不同；文件常常在用户专注其它应用时进来，30s 偏短）
- **send_files 返回结构化报告**：`{ filename, size, deliveries: [{peer_id, status: "ok"|"rejected"|"timeout"|"network_error", error?: string}] }` 让前端 UI 能逐 peer 展示
- **接收审批弹框组件复用**：与 `group-approval` 共用 BaseApprovalCard 抽象（30s 倒计时 + 同意/拒绝 + 多并发提示），样式属性化区分 type=join|file
- **接收方落盘目录可配**：v0 硬编码 Downloads；用户可能想固定到自定义目录（属 `settings-panel` 扩展）
- **/file 端点是否合并到统一 /payload**：与 `clipboard-image-sync` 第 5.4 节 同议题——所有加密字节流走一个端点是否更简洁

## 6. UX 设计

> 本节由 ux-designer 在 2026-05-06 填写。颜色/字号/字体/间距等视觉语言定义见 floating-window.md 第 6.5 节，本节不重复定义。

### 6.1 信息架构

文件传输涉及两个独立的 UX 场景，信息架构分别描述：

**发送端（dragOver 到 drop 到结果）**：
1. 拖入提示（正在拖 → 覆盖层提示"松开即发送"）
2. 发送进度（spinning 等待状态）
3. 发送结果报告（已送达 N/M 台，或超出大小限制）

**接收端（弹框审批）**：
1. 来源设备名（谁发来的）
2. 文件名（接收什么）
3. 文件大小（多大）
4. 保存位置提示（将保存到 Downloads）
5. 30 秒倒计时
6. 同意（保存）/ 拒绝 双按钮

### 6.2 关键流程图（文字版）

发送主路径：

1. 用户把文件从 Finder / Explorer 拖向浮窗 → 进入浮窗区域时显示 dragOver 覆盖层
2. 用户松开（drop）→ 覆盖层变为"发送中…" spinner
3. 后端 `send_files` 完成 → 浮窗顶部显示 banner 报告（"已送达 2/3 台"，3s 自消失）
4. 发送方历史顶部出现 file 条目（状态：已发送）

接收主路径：

1. 接收端收到 `file-pending` 事件 → 显示文件接收覆盖层（与 group-approval 同一视觉框架）
2. 用户点"保存" → 弹框消失 → 文件落 Downloads → 历史顶部出现 file 条目（已保存）
3. 用户点"拒绝" → 弹框消失 → 无文件写盘 → 历史不记录

异常路径：

- 文件 > 5MB：dragOver 覆盖层显示"单文件 ≤ 5 MB"提示；drop 后直接 banner 提示"<文件名>：超过 5MB 上限"，不发送
- 拖入多文件（> 1 个）：逐个尝试发送，超限的单独报告，不影响其他文件
- 接收方 30s 不响应：弹框自动关闭（超时），发送方报告此 peer 为"超时"失败
- Downloads 目录不可访问：接收方历史写一条 file_status = "failed" + error 文字

### 6.3 ASCII wireframe（必填）

发送端 —— dragOver 覆盖层（文件拖入浮窗范围时）：

```
┌────────────────────────────────┐
│  ● 小组 · 2 台  [加入]  −  ⚙  │← 顶部状态栏仍可见
├────────────────────────────────┤
│ ┌──────────────────────────── ┐│← 蓝色虚线边框覆盖历史区域
│ │                              ││← 背景 rgba(59,130,246,0.08)
│ │    📂                        ││← 图标居中
│ │    松开即发送给所有设备       ││← 14px #f3f4f6
│ │    单文件 ≤ 5 MB             ││← 12px #9ca3af
│ │                              ││
│ └──────────────────────────── ┘│
├────────────────────────────────┤
│  192.168.1.50:5858    工作 Mac │
└────────────────────────────────┘
```

发送中（drop 后，等待结果）：

```
├────────────────────────────────┤
│                                │
│         发送中…                │← spinner + 文字居中，13px
│                                │
├────────────────────────────────┤
```

发送结果 banner（顶部，3s 自消失）：

```
┌────────────────────────────────┐
│  ✓ report.pdf: 已送达 2/2 台   │← 绿色背景，白字，13px，顶部贴边
├────────────────────────────────┤
│  [正常历史列表内容]              │
```

超限文件 banner：

```
┌────────────────────────────────┐
│  ✗ big.zip: 超过 5MB 上限      │← 红色背景，白字，13px
├────────────────────────────────┤
```

接收端 —— 文件接收弹框（复用 group-approval 覆盖层结构，不同图标和文字）：

```
┌────────────────────────────────┐
│  ● 小组 · 2 台  [加入]  −  ⚙  │← 顶部状态栏仍可见
├────────────────────────────────┤
│  ██████████████████████████████│← 半透明蒙层
│  █                            █│
│  █  ╔══════════════════════╗  █│
│  █  ║  📎  收到文件         ║  █│← 📎 图标 + "收到文件"标题
│  █  ╠══════════════════════╣  █│
│  █  ║  report.pdf           ║  █│← 文件名，13px #f3f4f6 加粗
│  █  ║  2.0 MB               ║  █│← 文件大小，12px #9ca3af
│  █  ║  来自 工作 Mac         ║  █│← 来源设备名，12px #9ca3af
│  █  ║  将保存到 Downloads    ║  █│← 说明，11px #9ca3af
│  █  ║  ⏱ 还剩 23 秒        ║  █│← 倒计时，颜色规则同 group-approval
│  █  ║  [拒绝]    [保存]     ║  █│← ghost + primary blue
│  █  ╚══════════════════════╝  █│
│  ██████████████████████████████│
├────────────────────────────────┤
│  192.168.1.50:5858    工作 Mac │
└────────────────────────────────┘
```

历史中 file 条目（接收方，已保存）：

```
┌──────────────────────────── ✕ ┐
│ 📎 report.pdf                  │← 13px 文件名
│    2.0 MB · 已保存              │← 12px 副标题，"已保存"绿色
│ 来自 工作 Mac · 刚刚            │← 12px meta 行
└────────────────────────────── ┘
```

历史中 file 条目（发送方，已发送）：

```
┌──────────────────────────── ✕ ┐
│ 📎 report.pdf                  │
│    2.0 MB · 已发送              │← "已发送"灰色
│ 本机 · 刚刚                     │
└────────────────────────────── ┘
```

历史中 file 条目（接收方，保存失败）：

```
┌──────────────────────────── ✕ ┐
│ 📎 photo.png                   │
│    4.8 MB · 保存失败：磁盘已满  │← "保存失败："红色，后接错误信息
│ 来自 工作 Win · 5 分钟前        │← 12px meta 行
└────────────────────────────── ┘
```

### 6.4 交互细节

发送端交互：

- dragOver 进入：覆盖层立即显示（蓝色虚线边框 + 蓝色半透明背景）；`cursor: copy`
- dragOver 离开（鼠标拖出浮窗范围）：覆盖层消失，回到历史列表
- drop（松开）：覆盖层变为"发送中…"spinner，无可点击元素
- 发送完成：spinner 消失，顶部 banner 出现 3s 后消失

接收端交互：

- 蒙层出现：同 group-approval 覆盖层规则（蒙层不可穿透点击）
- "保存"按钮：primary blue，点击后 disabled + "保存中…"
- "拒绝"按钮：ghost，点击后 disabled + "已拒绝"
- 倒计时视觉规则：与 group-approval 完全一致（见 group-approval.md 第 6.4 节）
- 蒙层背景：不可点击关闭（必须明确选择）

文件接收弹框与握手审批弹框的区分（关键决策）：

- 图标区分：握手审批用 📥，文件接收用 📎
- 标题区分：握手审批"有设备申请加入"，文件接收"收到文件"
- 内容行区分：握手审批显示设备名 + IP；文件接收显示文件名 + 大小 + 来源 + 保存位置
- 确认按钮文字区分：握手审批"同意"，文件接收"保存"
- 视觉框架（蒙层 + 卡片 + 圆角 + 倒计时 + 双按钮）完全复用（BaseApprovalCard 抽象）

发送结果报告展示（关键决策）：

- 展示位置：浮窗顶部 banner（叠在状态栏下方），而非 toast 弹框
- 格式："✓ report.pdf: 已送达 2/2 台"（成功绿）/ "✗ big.zip: 超过 5MB 上限"（danger red）
- 多文件时：每个文件一行 banner，或合并为"已送达 2/2 台（共 3 个文件）"——首先取决于文件数，单文件显示文件名，多文件显示汇总
- 持续时间：3s 后自消失，用户可忽略

### 6.5 状态与颜色字典

详见 floating-window.md 第 6.5 节。文件传输特有颜色：

| 元素 | 颜色 | 说明 |
|---|---|---|
| dragOver 边框 | `#3b82f6` 蓝色虚线 | 2px dashed |
| dragOver 背景 | `rgba(59,130,246,0.08)` | 极浅蓝 |
| 发送成功 banner 背景 | `rgba(34,197,94,0.15)` | 浅绿 |
| 发送成功 banner 文字 | `#22c55e` | 成功绿 |
| 超限 banner 背景 | `rgba(239,68,68,0.15)` | 浅红 |
| 超限 banner 文字 | `#ef4444` | danger red |
| 文件接收蒙层 | `rgba(0,0,0,0.50)` | 同 group-approval |
| 文件接收卡片 | `rgba(28,28,32,0.96)` | 同 group-approval |
| 文件名 | `#f3f4f6` + font-weight 600 | 13px 加粗 |
| 文件大小/来源 | `#9ca3af` | 12px 次要色 |
| 保存位置提示 | `#9ca3af` | 11px hint |
| 状态徽章"已保存" | `#22c55e` | 成功绿 |
| 状态徽章"已发送" | `#9ca3af` | 中性灰 |
| 状态徽章"保存失败" | `#ef4444` | danger red |

### 6.6 边界与例外

- 0 个 peer 时拖文件：覆盖层显示"还没有连接的设备"（不显示"松开即发送"），drop 后 banner 提示同样信息
- 文件 = 0 bytes：属发送端 metadata 校验，返回"文件为空"提示
- 文件 = 1 MB（正常范围）：正常流程
- 文件 = 5 MB（恰好在上限）：允许（协议要求 `size ≤ MAX_SEND_SIZE = 5 * 1024 * 1024`，等于 5MB 通过）
- 文件 > 5 MB：banner 提示，不发送
- 多文件拖入（3 个，其中 1 个超限）：超限的单独报告跳过，其余正常发送；banner 展示每个文件的结果
- 接收方 Downloads 不可访问（如沙箱限制）：历史写 failed 条目 + error 文字；不阻塞发送方
- 文件名含 Win 保留字（CON / NUL / AUX）：sanitize 后安全，但用户看到文件名可能变化（如 `con.txt` → `con_sanitized.txt`，具体规则属架构师 + 安全 ADR 决议）
- 并发接收多个文件审批（A 和 B 同时发文件）：与 group-approval 相同的队列策略，one-at-a-time 处理
- 实测可能暴露的问题：文件接收弹框与握手审批弹框在同一队列中可能同时出现（有人来申请加入 + 同时有文件进来），需要确认队列优先级；建议架构师在 ADR 中明确

### 6.7 给前端工程师的实现提示（可选）

- dragOver 覆盖层建议监听整个 `document` 的 `dragenter` / `dragleave` / `drop` 事件（而不是浮窗某个子元素），避免子元素边界触发 leave 的闪烁
- Tauri 的 `tauri://drag-drop` 事件和 HTML5 的 `drop` 事件可能同时触发，建议只用 Tauri 事件处理文件路径，HTML5 事件只用于 dragOver 视觉状态

### 6.8 给主窗口和 PM 的反馈（UX 开放问题回应）

**问题 1**：拖入时的视觉反馈（蓝色虚线框 / 蒙层 / 改变光标）。

结论：蓝色虚线边框（2px dashed `#3b82f6`）+ 极浅蓝背景（`rgba(59,130,246,0.08)`）+ `cursor: copy`。不做全屏蒙层（蒙层是接收审批弹框的专属语义，发送端的 dragOver 用虚线框区分）。虚线框在视觉上直觉地表示"这是一个可放置的目标区域"，符合系统级拖放惯例。

**问题 2**：接收审批弹框是否复用 group-approval 的 overlay 视觉。

结论：复用视觉框架（蒙层 + 卡片 + 圆角 + 倒计时 + 双按钮），但通过图标（📎 vs 📥）、标题文字和卡片内容行区分两种场景。这是 spec 第 3 节提到的 BaseApprovalCard 抽象的实现方向，属架构师和前端实现者的决定；UX 层面两者视觉框架一致是正确的。

**问题 3**：文件大小超限时的反馈。

结论：两层反馈。发送端：dragOver 覆盖层提示"单文件 ≤ 5 MB"（提前告知），drop 后 banner 精确报告"<文件名>：超过 5MB 上限"（3s 自消失，danger red）。不弹 modal（保持轻量）。

**问题 4**：多文件并发时的视觉处理（拒接 vs 排队）。

结论：发送端不拒绝多文件拖入，逐个尝试处理，每个文件在 banner 中单独报告结果。接收端的并发审批弹框采用 one-at-a-time 队列（与 group-approval 一致）。不允许接收端同时显示多张文件接收卡片（视觉混乱，且用户无法判断哪个文件对应哪张卡片）。

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 2 条] [P1 5 条] [P2 2 条]

- [P0] [安全] filename sanitize 是否覆盖 Win 保留名（CON/NUL/...）+ Unicode 反向覆盖字符（U+202E）+ 控制字符？v0 仅过滤 `/ \\ \0`
- [P0] [安全] 攻击者发声明 size=100、实际 body 7 MB 的请求：v0 靠 decrypt 后 length 校验兜底，是否在协议层（HTTP body limit + 早期 size 校验）多加一道防御
- [P1] [架构师] **非 PNG 图片格式（JPG / GIF / WebP / HEIC / BMP / TIFF 等）的剪切板内容是否走文件传输通路？** —— 与 `clipboard-image-sync` 联动决议（_assumptions A14 用户反提）。当前 `clipboard-image-sync` 仅处理 PNG；用户在 OS 上 Cmd+C 一张 JPG 图片时（例如从浏览器右键复制图像），arboard 在不同平台的行为不一致：① macOS 部分情形会光栅化为 PNG（属 `clipboard-image-sync` 通路），② 部分情形保留为原格式但 arboard `get_image()` 不返回（属本 spec 通路 — 把它当"剪切板里的图像引用"按文件处理），③ Windows 行为又另一套。需要架构师明确：JPG/GIF/WebP 等非 PNG 内容是否在 `arboard::get_image()` 失败时由本 spec 的文件传输通路兜底（通过某种"剪切板里的非 PNG 字节流"接口路径）；若是则需新增轮询分支与协议字段。议题决议位置：本 spec 第 3 节 + `clipboard-image-sync` 第 3 节 同步更新
- [P1] [安全] 接收审批 30 秒超时是否过短？文件接收常发生在用户专注其它应用时；是否分两个常量（握手 30s / 文件 60s）。本 spec 第 3 节 暂定 30s 待 ADR 决议
- [P1] [架构师] /file 与 /clipboard 是否合并为统一 /payload 端点（对应 `clipboard-image-sync` 第 7 节 同议题）
- [P1] [UX] 文件接收弹框是浮窗内覆盖层（v0）还是系统级原生通知？文件常在用户不看浮窗时进来，浮窗内覆盖层易被忽略 → 30s 超时
- [P1] [架构师] send_files 返回结构化报告 vs v0 字符串拼接：UI 是否要支持"重发到失败的 peer"？
- [P2] [架构师] 流式加密 / 流式落盘以避免 5 MB × N peer 内存峰值——是否值得在 P1 阶段就上，还是后续优化
- [P2] [UX] 落盘目录是否可配（settings 加一项），还是永远 Downloads？

## 8. Review 段（占位）

> code-reviewer / tech-architect / security-reviewer 后续填写。本 feature 涉及网络协议、加密路径、文件系统写盘 + filename sanitize，必须经 security-reviewer ACK（CLAUDE.md 第 9 节）。
