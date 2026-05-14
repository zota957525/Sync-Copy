---
title: 事实假设清单（PM 视角，已用户校对）
owner: product-strategist
status: APPROVED_WITH_REVISIONS
created_at: 2026-05-08
reviewed_at: 2026-05-08
reviewed_by: user
depends_on_artifacts:
  - path: specs/00-product-overview.md
    version: 2026-05-06
  - path: HANDOFF.md
    version: v5
revisions_summary:
  - id: A2
    field: 切换频率
    old: 1-10 次/天
    new: 10-100 次/天
    triggers_spec_update: false
  - id: A14
    field: 非 PNG 图片格式
    old: 仅 PNG，其它不支持
    new: PNG 走剪切板图片通路；JPG/GIF/WebP 等走文件传输通路（待 PM 在两份 spec 联动决议）
    triggers_spec_update:
      - specs/clipboard-image-sync.md
      - specs/file-transfer-drag.md
  - id: A16
    field: 单文件上限
    old: 50 MB
    new: 5 MB
    triggers_spec_update:
      - specs/file-transfer-drag.md
  - id: A_BUG_HIDDEN_DEAD
    field: 新增 v0 实战 bug（非 _assumptions 项）
    old: —
    new: 隐形掉线（长时间运行后部分设备同步失败但表面无异常，重启程序恢复）
    triggers_spec_update:
      - specs/peer-heartbeat.md
    notes: 已记录到 docs/handoff-lessons-learned.md 第 4.1 段
---

# Sync Copy 事实假设清单

> v2-6 强制产物：第二阶段确认完角色清单后、product-strategist 写第一份 backlog 之前必须产出。
>
> 本项目 P1-1~P1-5 已完成 18 份 spec 后才补建本清单（v5 迁移补丁 — ADR-002）。已有 18 份 spec 中隐含的事实假设全部反向提取在此，请逐条校对，避免后续返工。

## 校对说明

| 列 | 含义 |
|---|---|
| `#` | 序号，固定不变（即使删条目也不重排） |
| `假设内容` | PM 已基于该假设写过 spec / ADR |
| `出处 / 推断依据` | 哪份文档 / 哪句话 / 哪个 spec 第 X 节 体现 |
| `置信度` | 高 / 中 / 低 — PM 自评 |
| `用户校对` | ☐ 未校 / ✅ 确认 / ❌ 修正（写新值） / ⚠ 部分对（写补充） |

校对时请直接编辑此表的最后一列。任意 ❌ 或 ⚠ 触发对应 spec 的 SUPERSEDED 流程。

---

## 一、用户与场景

| # | 假设内容 | 出处 / 推断依据 | 置信度 | 用户校对 |
|---|---|---|---|---|
| 1 | 主用户 = 单人，使用自己拥有的多台设备 | `specs/00-product-overview.md` 第 1 节"产品定位"；用户原话"单人多机使用" | 高 | ✅ |
| 2 | 用户每天**主动**切换设备 10-100 次（**用户校对修正**：原 PM 假设 1-10 次过低，实际更频繁 — 1 天 10-100 次） | `specs/00-product-overview.md` 第 2 节"使用场景" — **2026-05-08 用户校对修正** | 中 | ❌ → 修正：10-100 次 |
| 3 | 用户技术水平：能看懂 IP / 端口 / 防火墙提示，**不会**写代码 | `specs/00-product-overview.md` 隐含；v0 floating-window UI 设计假定用户能识别 IP | 中 | ✅ |
| 4 | 用户对同步延迟容忍度：**1 秒级**（CLAUDE.md "1 秒级同步延迟可接受"） | `specs/clipboard-text-sync.md` 第 4 节 AC | 高 | ✅ |
| 5 | 一组规模：2-5 台设备（同时在线） | `specs/group-discovery.md` / `specs/peer-heartbeat.md` 第 3 节 | 中 | ✅ |

## 二、网络与部署形态

| # | 假设内容 | 出处 / 推断依据 | 置信度 | 用户校对 |
|---|---|---|---|---|
| 6 | 所有设备处于同一局域网（同 broadcast domain，可路由） | `specs/00-product-overview.md` 第 1 节 | 高 | ✅ |
| 7 | LAN 网段 = RFC1918：`192.168.0.0/16` 优先 + `10.0.0.0/8` + `172.16.0.0/12`（172.x 因 WSL/Docker 频繁误命中故降级） | `specs/local-ip-display.md` 第 4 节；v0 修过这个坑 | 高 | ✅ |
| 8 | LAN 直连可达，**不**走 NAT 穿透 / 中继服务器 / 公网 | `specs/00-product-overview.md`；用户原话"无中心服务器" | 高 | ✅ |
| 9 | 监听端口默认 5858（用户可改） | `specs/settings-panel.md` 第 4 节 | 高 | ✅ |
| 10 | 系统代理（Clash / ClashX / Surge 等）可能拦截 LAN 请求；客户端必须 `.no_proxy()` 走直连 | v0 实战修过；`specs/group-discovery.md` 第 5 节"v0 历史" | 高 | ✅ |
| 11 | 多网卡场景常见（VPN / 虚拟网卡 / WSL），需 IP 优先级筛选；虚拟网卡名（vEthernet / utun / vmnet 等）需排除 | v0 实战；`specs/local-ip-display.md` 第 5 节 | 高 | ✅ |

## 三、同步对象与体积

| # | 假设内容 | 出处 / 推断依据 | 置信度 | 用户校对 |
|---|---|---|---|---|
| 12 | 当前阶段同步范围 = 文本 + 图片 + 文件三类 | `specs/00-product-overview.md` 第 2 节"核心功能" | 高 | ✅ |
| 13 | 文本上限 1 MB，超过跳过广播 | `specs/clipboard-text-sync.md` 第 4 节 | 中 | ✅ |
| 14 | 图片仅 PNG 格式（v0 已选定，理由：跨平台一致 + 无损） | `specs/clipboard-image-sync.md` 第 3 节 | 高 | ⚠ 用户反提：剪切板图片走 PNG 通路；JPG / GIF / WebP 等其它格式怎么办？建议走文件传输通路。**待 PM 在 `clipboard-image-sync.md` 第 7 节 + `file-transfer-drag.md` 第 7 节联动决议** |
| 15 | 图片上限 5 MB（PNG 解压后） | `specs/clipboard-image-sync.md` 第 4 节 | 中 | ✅ |
| 16 | 文件上限 5 MB（单文件）—— **用户校对修正**：原 PM 假设 50 MB 过大，LAN 同步剪切板配套场景下 5 MB 更合理 | `specs/file-transfer-drag.md` 第 3 节 — **2026-05-08 用户校对修正** | 中 | ❌ → 修正：5 MB |
| 17 | 不同步富文本格式（RTF / HTML） — 仅纯文本 | `specs/clipboard-text-sync.md` 第 3 节"范围外" | 高 | ✅ |
| 18 | 不同步剪切板内的图像引用（如 macOS Finder 选中文件 → Cmd+C 后粘贴到聊天工具会变图） — v2 待澄清 | `specs/file-transfer-drag.md` 第 7 节 第 P1 项 | 中 | ✅ |

## 四、信任与安全

| # | 假设内容 | 出处 / 推断依据 | 置信度 | 用户校对 |
|---|---|---|---|---|
| 19 | 信任建立靠 = **弹框审批**（每台新设备入组都触发组内任一在线设备弹框） | `specs/group-approval.md` 第 1 节 | 高 | ✅ |
| 20 | 加密 = E2E，**密钥对每会话动态协商**（非长期共享密码派生） | `specs/e2e-encryption.md` 第 3 节 | 高 | ✅ |
| 21 | 加密原语 = X25519 ECDH + HKDF-SHA256 + AES-256-GCM（v0 选定，待 architect 复核） | `Cargo.toml` 实际依赖；`specs/e2e-encryption.md` 第 6 节 | 高 | ✅ |
| 22 | 不依赖 CA / 证书 / 系统 keychain；私钥存进程内存（v0 行为） | v0 实现；待 architect 在 ADR 中确认 | 中 | ✅ |
| 23 | LAN 内**抓包**应该看不到任何明文剪切板内容 | `specs/e2e-encryption.md` 第 4 节 AC | 高 | ✅ |

## 五、跨平台与分发

| # | 假设内容 | 出处 / 推断依据 | 置信度 | 用户校对 |
|---|---|---|---|---|
| 24 | 目标平台 = macOS（Apple Silicon + Intel）+ Windows（x64） | `Cargo.toml` / GitHub Actions matrix；`specs/cross-platform-build.md` | 高 | ✅ |
| 25 | **不**做 Linux 桌面分发（用户未提，v0 未做） | 排除假设 — 待用户确认 | 中 | ✅ |
| 26 | 分发形态：**便携可执行**（macOS .app / Windows .exe），双击即跑，**不需要安装器** | `specs/cross-platform-build.md` 第 3 节 | 高 | ✅ |
| 27 | macOS 提供 `.app`（universal）；Windows 提供 `.exe`（x64） + `.msi` 备选 | v0 CI 实际产物；`specs/cross-platform-build.md` 第 4 节 | 高 | ✅ |
| 28 | 不上 App Store / Microsoft Store（个人工具，免提审核）— 故 safety-bar.sh 把上传命令拦死 | 隐含；ADR-002 第 3 节 | 中 | ✅ |
| 29 | 不做代码签名 / 公证（用户首次启动会被系统警告，可手动放行） | v0 行为；`specs/cross-platform-build.md` 第 5 节"v0 历史" | 中 | ✅ |

## 六、项目阶段与团队

| # | 假设内容 | 出处 / 推断依据 | 置信度 | 用户校对 |
|---|---|---|---|---|
| 30 | 项目阶段 = 个人工具 / 早期 MVP，**无付费用户** | `CLAUDE.md` 第 1 节"团队"段 | 高 | ✅ |
| 31 | 团队规模 = 单人（zota957525）+ 10 个虚拟同事（agent） | `CLAUDE.md` 第 1 节 | 高 | ✅ |
| 32 | 不需要遵守 SOC2 / GDPR / HIPAA 等合规框架 | 隐含；个人工具 | 高 | ✅ |
| 33 | 不需要 SLA / 性能 NFR 数字指标（如 P99 < X ms）；只要"用着不卡"即可 | 隐含；用户原话"1 秒级延迟可接受" | 高 | ✅ |
| 34 | 不需要 SRE / 监控告警系统 | 个人工具，无生产环境 | 高 | ✅ |

## 七、数据持久化与历史

| # | 假设内容 | 出处 / 推断依据 | 置信度 | 用户校对 |
|---|---|---|---|---|
| 35 | 历史记录上限 = 50 条（FIFO，超过后老的自动丢） | `specs/history-list.md` 第 4 节 | 中 | ✅ |
| 36 | 历史记录关程序后**丢失**（不持久化到磁盘） | v0 行为；`specs/history-list.md` 第 5 节"v0 历史" — **待 v2 决议是否改** | 中 | ✅ |
| 37 | 配置（端口 / 设备名 / 历史信任组）持久化到用户目录 JSON | v0 行为；`specs/settings-panel.md` 第 4 节 | 高 | ✅ |
| 38 | 配置文件**不**加密（用户本地，攻击模型不覆盖本地物理访问） | 隐含；待 security-reviewer 确认 | 中 | ✅ |

## 八、未在 spec 体现但 PM 已假设的"边界外"事实

| # | 假设内容 | 出处 / 推断依据 | 置信度 | 用户校对 |
|---|---|---|---|---|
| 39 | 不需要 IPv6 支持（v0 仅 IPv4） | v0 实现 | 中 | ✅ |
| 40 | 不需要支持移动设备（iOS / Android）— 仅桌面 | 用户未提；v0 未做 | 中 | ✅ |
| 41 | 不需要"团队协作"功能（多人共组、权限分级、操作审计） | 单人多机定位 | 高 | ✅ |
| 42 | 不需要支持企业代理 / 域控环境的特殊配置 | 个人工具定位 | 中 | ✅ |
| 43 | 用户**愿意**给设备起独特名字（不会出现 5 台都叫"MacBook Pro"） | v0 实战未踩；本项目假定 | 低 | ✅ |
| 44 | 用户**接受**首次启动 firewall 弹框（macOS 首次开 server 端口会问） | v0 实战；用户已经接受过 | 高 | ✅ |

---

## 校对完成后下一步

1. 标 ❌ 或 ⚠ 的条目，对应的 spec 进入 SUPERSEDED 流程（PM 修订 spec → status 设为 SUPERSEDED → 写新版）
2. 全部 ✅ 后，本文件 frontmatter 的 `status` 改为 `APPROVED`
3. 主窗口在 `docs/handoff-lessons-learned.md` 第 9 段记账：'_assumptions.md 校对完成 + N 处事实层修正'

## 修订历史

| 版本 | 日期 | 修改内容 |
|---|---|---|
| v1 | 2026-05-08 | 初版 — 反向从 18 份 spec 提取 44 条假设。等用户校对 |
| v2 | 2026-05-08 | 用户校对完成。3 处事实层修正（A2 频率 / A14 非 PNG 路由 / A16 文件上限）+ 1 条 v0 实战 bug 入档（隐形掉线 → peer-heartbeat 新 AC）。其余 41 条 ✅。状态 → APPROVED_WITH_REVISIONS |
