---
status: SPEC_REVIEWED
owner: product-strategist
related_adrs: [ADR-003, ADR-011]
related_specs: [00-product-overview, clipboard-text-sync, e2e-encryption, group-discovery, file-transfer-drag]
created: 2026-05-06
updated: 2026-05-08
revised: 2026-05-08 — ADR-003 第 3.2 节 答 第 7 节 [P1] 非 PNG 路由 / OS 光栅化 PNG 边界 (arboard get_image() 成功统一走 PNG 通路；用户拖文件走 /file 兜底；超 5 MB toast 提示)
priority: P1
revision_history:
  - version: v1
    date: 2026-05-06
    notes: 初版 SPEC_DRAFTED，仅 PNG 走剪切板图片同步通路；JPG/GIF/WebP 等其它格式标"out of scope"但未明确替代路径
  - version: v2
    date: 2026-05-08
    notes: 用户校对 _assumptions A14，明确"PNG 走本通路；JPG/GIF/WebP 等其它格式由 file-transfer-drag 文件传输通路兜底"；out of scope 段落显式声明替代通路；第 7 节加 P1 给架构师关于"OS 光栅化非 PNG 为 PNG"的边界 case
  - version: v3
    date: 2026-05-08
    notes: ADR-003 决议 — arboard get_image() 成功 → PNG 通路（统一编 PNG 不区分原始格式）；arboard 失败时静默不处理，由用户拖文件走 /file 通路；超 5 MB toast 提示用户
---

# clipboard-image-sync — 跨设备 PNG 图片剪切板自动同步

## 1. 问题（为什么做）

文本同步只是用户日常的一半。另一半是**截图**——Mac 上 `Cmd+Shift+Ctrl+4` 截图后默认进剪切板，期望像系统级 iCloud 通用剪切板那样能在 Win 上 `Ctrl+V` 粘出。无云、无中转、不出 LAN，是 Sync Copy 的差异化（00 总览 第 1 节 / 用户故事 #2 截图工作流）。技术挑战：跨平台图片格式归一化（arboard 给的是 RGBA 字节，必须编为 PNG 才便于跨机识别和落历史）、图片 5 MB 上限、与文本互斥的轮询顺序（避免截图时同步推送系统附带的 metadata 文本，详见 `clipboard-text-sync` 第 5.2 节 的"先 image 后 text"约定）、内容哈希夹带导致的隐式 metadata 泄露。

本 feature 与 `clipboard-text-sync` **共用**协议骨架与轮询线程：仅在 `ClipboardReq.kind = "image_png"` + `image_width / image_height` 字段上分叉。

## 2. 用户故事

- As a content creator on Mac, I want a screenshot I just took (`Cmd+Shift+Ctrl+4`) to be pasteable on my Windows within ~2 seconds in any app (Word / 微信 / Keynote), so that I skip save-then-airdrop-then-open.
- As a user, I want the app to silently skip syncing oversized screenshots (full-resolution Retina screen ≈ 10+ MB) instead of slowing down or crashing, so that the 5 MB cap is a soft, invisible safety net.
- As a user receiving a remote image, I want it placed in my system clipboard such that any image-aware app (Preview / Paint / Word) accepts it, not as a "data:image/png" string.

## 3. 范围

**in scope**：
- **仅 PNG 格式走剪切板图片同步通路**（_assumptions A14 用户校对决议 2026-05-08）：本 spec 仅响应 `arboard::Clipboard::get_image()` 成功返回的 RGBA 字节场景，编码为 PNG 后走加密广播。其它格式（JPG / GIF / WebP / HEIC / BMP / TIFF 等）不进本 spec 通路 — 详见下方 out of scope 与 第 7 节 [P1] 架构师未决问题
- 共用 `clipboard-text-sync` 启动的 std::thread + `arboard::Clipboard` + `mpsc::Sender<ClipboardCmd>` 命令通道（不另起线程，本 feature 仅扩展轮询循环的 image 分支）
- 1 秒轮询周期内**先看图片再看文本**：`clipboard.get_image()` → 取 RGBA 字节 → `image::codecs::png::PngEncoder`（quality=Fast、FilterType::NoFilter）编码为 PNG 字节流 → 算 SHA-256 → 与 `last_image_hash: Option<[u8;32]>` 比较 → 不同且 PNG 大小 ≤ 5 MB 时触发：
  - `state.history.push_image(width, height, data_url, content_hash_hex, Source::Local)` 进历史 + emit `history-updated`
  - `tauri::async_runtime::spawn(network::client::broadcast_image(state, png, w, h))` 异步加密广播
- `ClipboardCmd::SetImageSuppress { png, width, height }` 写入路径：解码 PNG → 构造 `ImageData { Cow::Owned(rgba), width, height }` → `clipboard.set_image()` → 更新 `last_image_hash = Some(hash(png))` + **强制 `last_text = None`**（图片进剪切板时系统可能清空文本，避免下一轮误识别）
- 接收路径（在 axum `/clipboard` handler 的 `kind == "image_png"` 分支）：
  - 校 `image_width > 0 && image_height > 0`，否则 400
  - 复用 `clipboard-text-sync` 已建立的：origin 在 peers 表校验 → seq dedupe → AES-GCM 解密
  - 算 `content_hash = sha256_hex(plaintext_png)`
  - 构造 `data_url = "data:image/png;base64," + base64(plaintext_png)` 用于历史 UI 展示
  - `state.history.push_image(w, h, data_url, content_hash, Source::Remote{device_name})`
  - 发 `ClipboardCmd::SetImageSuppress { png: plaintext, width, height }` 给剪切板线程
  - emit `history-updated` 事件
- 协议字段（与 `clipboard-text-sync` 共用 `ClipboardReq`）：
  - `kind: "image_png"`
  - `image_width: Option<u32>`、`image_height: Option<u32>`（text 时为 None）
  - `nonce` / `ciphertext` 携带 PNG 字节经 AES-256-GCM 加密后的产物
- 5 MB 大小上限 = `MAX_IMAGE_BYTES = 5_000_000`，超过即跳过（debug log 一行）

**out of scope**：
- **JPG / GIF / WebP / HEIC / BMP / TIFF 等其它图片格式不走本 spec 通路**（_assumptions A14 用户校对决议）。这类格式在剪切板里出现的典型场景是"用户从浏览器右键复制图像"或"截图工具配置为非 PNG 输出"。**替代通路**：由 `file-transfer-drag` 的文件传输通路兜底（用户拖拽该图片文件到浮窗即可发送）。注意：本 spec 不主动检测剪切板里的非 PNG 字节流并自动转走文件通路 — 即"用户 Cmd+C 一张 JPG"的体感可能是"什么都没发生"，由用户改用拖拽文件解决。**边界 case 见 第 7 节 [P1]**
- 富文本中嵌入的图片（仅响应"系统剪切板里整体是一张图片"的场景）
- 截图工具的"框选 → 直接发送"自定义工作流（用户用系统截图键即可）
- 图片预览缩略图（历史 UI 展示属 `history-list`，本 spec 仅 push 数据）
- 图片压缩 / 缩放（5 MB 内原样传；超过 5 MB 提示用户而非自动压缩）
- Linux Wayland 下的图片剪切板（00 总览 第 3 节 已锁不支持 Linux）

## 4. 验收标准（Definition of Done）

- [ ] A、B 已 `小组 · 2 台`。在 A 上用系统截图（`Cmd+Shift+Ctrl+4` Mac / `Win+Shift+S` Win）截一片屏 → 2 秒内 B 浮窗历史顶部出现该图缩略 + B 系统剪切板内含图片；B 上 Preview/Paint `Ctrl+V` 能粘出该图
- [ ] 在 A 上截一张超过 5 MB 的高分屏全屏图 → 不广播、不进历史、不报错（debug log 一行 `image too large, skipping`）
- [ ] 在 A 上连续两次截相同区域（PNG 字节哈希不变）→ 仅第一次广播
- [ ] 在 A 上**先**截图（图片进剪切板）然后**未触发**对应文本广播（即图片伴随的 metadata 字符串不会被当作 text 推出去）
- [ ] B 收到 A 的图片写入剪切板后，B 的轮询不会反推同张图给 A（`last_image_hash` 抑制环路）
- [ ] B 收到密文但 PNG 解码失败（人为破坏字节）→ 不写入剪切板、log warn、不进历史
- [ ] A 把刚截的图复制后立即关闭应用，B 在 A 退出前已收到该条 → B 历史保留
- [ ] 两端架构不同（Mac Apple Silicon + Win x64）时，图片 RGBA→PNG→RGBA 全流程像素级一致（无颜色通道错位）

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的
`legacy-prototype` 分支 `src-tauri/src/clipboard.rs` 的 run loop 中 image 分支：`clipboard.get_image()` → `width/height as u32` → `encode_rgba_to_png(width, height, &img.bytes)` 用 `image::codecs::png::PngEncoder::new_with_quality(out, CompressionType::Fast, FilterType::NoFilter)` + `write_image(rgba, w, h, ExtendedColorType::Rgba8)`。`MAX_IMAGE_BYTES = 5_000_000`。`hash_bytes` 算 SHA-256 → `last_image_hash: Option<[u8;32]>` 单值环路防御。`hex_string` 64 字符 hex 作 history `content_hash`。`data_url = "data:image/png;base64," + B64.encode(&png_bytes)` 用于历史展示。`SetImageSuppress` 写入路径除更新 `last_image_hash` 外**强制 `last_text = None`**（注释里写"图片进剪切板时，文本可能被系统替换为空。重置 last_text 避免误识别"）。`network/client.rs` 的 `broadcast_image` → `broadcast_payload(state, png, "image_png", Some(width), Some(height))` 复用文本广播路径仅改 kind 和图像尺寸字段。`network/server.rs` 的 `handle_clipboard` 的 `match req.kind.as_str()` 分支处理 `"image_png"`：校 `width/height > 0`、`push_image`、发 `SetImageSuppress`。`network/protocol.rs` 的 `ClipboardReq` 已声明 `image_width / image_height: Option<u32>` 字段（v0 已就位）。

### 5.2 v0 暴露的具体坑
- **`last_text = None` 在 SetImageSuppress 必须手动重置**：图片进剪切板时系统在某些 OS（macOS 部分版本）会顺带清空文本，下一秒轮询若不重置 `last_text` 会把"空字符串"当成本地新复制 → 触发空字符串过滤但状态错位。这条规则只在源码注释里有，**未文档化为不变式**（00 总览 第 5.2.1 节 已点名）
- **"先 image 后 text" 顺序**：截图时系统会同时在剪切板写图片 + 一段 metadata 文本（如某些 Mac 截图工具写文件路径）；如果先看 text 会先广播一段无意义文本 → v0 用 `handled_image: bool` 局部变量短路
- **每秒轮询有最差 1 秒延迟**：与文本同样的 trade-off（00 总览 第 5.4 节 待架构师评估事件驱动）
- **SHA-256 跨机器 content_hash 暴露 metadata**：同张图在两台机器上算出来的 hash 相同，攻击者可对照其它密文匹配"是否同一张图"。这与 `clipboard-text-sync` 第 5.2 节 的同一坑同源
- **PNG 编码用 `CompressionType::Fast`**：换更慢的压缩算法可让 5 MB 限额下塞更多像素，但 Fast 已经"实测够用"，没有 ADR 论证选 Fast 不选 Default 的理由
- **5 MB 上限是图片字节，但 axum body 上限是 8 MB**（因 base64 膨胀 + JSON 包裹）—— 一旦改图片上限到 7 MB 就会撞 body 上限，没文档化这个连锁约束
- **图片进历史用 data_url 内嵌 base64**：50 张大图历史 ≈ 250 MB 内存占用。v0 历史上限 50 条；图片密度高时内存暴涨没专门优化
- **arboard image 在 Linux Wayland 不稳**：编译时无 cfg 隔离

### 5.3 v2 应继承
- 共用文本同步的 std::thread + arboard + mpsc 命令通道
- "先 image 后 text" 轮询顺序
- `last_image_hash: Option<[u8;32]>` 单值环路防御
- `SetImageSuppress` 必须 reset `last_text = None`
- 5 MB 图片上限
- PNG 作为唯一跨机格式（编码用 image::codecs::png）
- ClipboardReq 协议字段（kind / image_width / image_height）
- content_hash = SHA-256(PNG bytes) 用于历史去重 + 跨机同步删除（属 `history-sync-delete`）

### 5.4 v2 应挑战
- **`last_text = None` 不变式必须明文落 ADR**（00 总览 第 5.2.1 节 教训直接对应）
- **历史中图片用 data_url 还是落盘临时文件 + 路径引用**：50 张高分屏图 ~250 MB 常驻内存是否值得为 UI 渲染省一次 IO？属架构师在 ADR 论证
- **"图片同步" + "文件传输"语义重合**：两者都是字节流 + 加密广播。v2 是否合并底层路径（同一个 `/payload` endpoint，由 kind 字段分流）？现状是 `/clipboard` 与 `/file` 两个端点
- **content_hash = SHA-256(plaintext)** 跨 peer metadata 泄露同 `clipboard-text-sync` 的隐患 —— 安全是否要求 HMAC(per-pair-key, plaintext) 替换？
- **PNG 压缩等级**：Fast vs Default 的 trade-off 需 ADR 写明
- **Retina 屏 RGBA 像素 → PNG 字节 5 MB 转化率**：4K 全屏截图常突破 5 MB；用户体感是"突然某张图没同步"。v2 是否提示用户（toast：`图片超过 5 MB，未同步`）？

## 6. UX 段（占位）

> 待 ux-designer 在后续阶段填写。建议覆盖：
> - 图片在 `history-list` 的缩略形态（固定高度 / 等比缩放 / 模糊裁剪）
> - 超大图被跳过时的用户感知（toast / 静默 / 图标提示）
> - 接收图片的 flash 反馈（v0 在 `history-list` 顶部短暂高亮，本 spec 沿用但视觉细节由 UX 拍板）

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 3 条] [P1 4 条] [P2 1 条]

- [P0] [架构师] `last_text = None` 在 SetImageSuppress 时重置的不变式如何在代码层强制（如抽象 `LastClipboardKind` 状态机）？v0 是隐式约定，v2 必须显式（与 `clipboard-text-sync` 第 7 节 [P0] 同源问题）
- [P0] [安全] content_hash = SHA-256(PNG bytes) 暴露"两条消息是否同图"的 metadata，是否改为 HMAC(per-pair-key, plaintext)？同 `clipboard-text-sync` 第 7 节 的同一问题
- [P0] [架构师] arboard 在 Linux Wayland 下编译时是否 `cfg(not(target_os = "linux"))` 隔离避免误启用？
- [P1] [架构师] **OS 光栅化非 PNG 为 PNG 的边界判定**（_assumptions A14 联动）：用户在剪切板里 Cmd+C 一张 JPG 时，OS 行为不一致 —— ① macOS 部分版本会自动光栅化为 PNG 格式（NSPasteboard 的 `public.png` UTI 优先），arboard `get_image()` 返回成功 RGBA → 此情形按照本 spec 通路走（已是 PNG 字节）；② 部分情形 arboard 直接返回原 JPG 字节或返回错误。架构师需明确：(a) v2 是否**统一在 arboard `get_image()` 成功时按 PNG 通路处理**（即不区分原始格式，OS 给我们 RGBA 我们就编 PNG 发），无视用户原本剪切板里是 JPG/GIF？(b) 若用户原意是 JPG（更小体积）但被光栅化为 PNG（更大体积、可能撞 5 MB 上限）—— 是否要给用户感知（toast：`已转为 PNG 同步`）？(c) 当 arboard `get_image()` 失败但剪切板里确实有非 PNG 图像时，本 spec 静默；这类场景只能靠文件传输通路兜底 —— 是否在文档/UX 层让用户知道这条退路？
- [P1] [架构师] 历史中图片是 data_url 内嵌还是落盘 + 路径？50 条上限 + 5 MB/张时内存峰值是否可接受
- [P1] [架构师] `/clipboard` 与 `/file` 是否合并为统一 payload 端点？两者都是 AES-GCM 加密字节流 + 元信息
- [P1] [UX] 图片超过 5 MB 时是否给用户主动提示而不是 v0 的静默 debug log？
- [P2] [架构师] PNG 编码 `CompressionType::Fast` 的选定需 ADR 写明（CPU vs 大小 vs 兼容性）

## 8. Review 段（占位）

> code-reviewer / tech-architect / security-reviewer 后续填写。本 feature 的网络协议层（/clipboard image 分支）必须经 security-reviewer ACK（CLAUDE.md 第 9 节）。
