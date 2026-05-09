---
status: SPEC_DRAFTED
owner: product-strategist
related_adrs: []
related_specs: [00-product-overview, floating-window]
created: 2026-05-06
updated: 2026-05-06
revised: 2026-05-06 — P1-5 一致性 review
priority: P0
---

# local-ip-display — 浮窗底部展示本机 LAN IP:PORT 与点击复制

## 1. 问题（为什么做）

加入小组的唯一交互方式是"在 B 上输入 A 的 `IP:PORT`"。如果 A 的浮窗不展示这个地址，用户必须自己去系统设置 / 终端查 `ipconfig` / `ifconfig` —— 这是极差的体验，也违反"60 秒内两台设备互通"的项目级 SLA（00 总览 第 4 节）。本 feature 让 A 在浮窗左下角直接看到自己的 `192.168.x.x:5858`，点一下复制到剪切板，告诉 B 即可。这看似小但是 v2 加入流程的首屏入口，必须 P0 交付。

外加一道"过滤虚拟网卡 / Clash fake-IP / WSL"的工程门槛——v0 经多轮迭代才稳定的网卡选择策略，v2 必须明文继承。

## 2. 用户故事

- As the host of a 2-device join flow, I want my LAN IP:PORT shown clearly at the bottom-left of the floating window, so that I can read it out (or copy-paste it) to the joining device without consulting any system tool.
- As a user with VPN / Docker / WSL installed, I want the displayed IP to be my real LAN IP (192.168.* / 10.* / 172.16-31.*) and not a fake-IP from Clash or a virtual NIC, so that the address I share actually works.
- As a user, I want to click the IP:PORT to copy it and see a tiny confirmation, so that I know the copy succeeded.

## 3. 范围

**in scope**：
- 后端 `get_local_ip` 命令枚举 `if-addrs::get_if_addrs()` 所有 IPv4 网卡，按规则过滤+排序，返回**最优**一个 IP 字符串（或 `None`）
- 过滤规则（v0 已收敛，v2 直接继承）：
  - 跳过 loopback、APIPA `169.254/16`、Benchmark `198.18-19/16`（Clash fake-IP）
  - 跳过名字含 `vethernet / wsl / virtualbox / vmware / hyper-v / docker / virtual / loopback` 或以 `utun / awdl / llw` 开头的网卡
- 优先级排序：`192.168/16` > `10/8` > `172.16-31/12` > 其它
- 前端：浮窗底部左下角展示 `IP:PORT`（PORT 从 `Config.port` 读，默认 5858）
- 单击该地址 → 复制到系统剪切板 → 显示一个 1.5 秒消失的绿色 `已复制` 微提示
- 当 `get_local_ip` 返回 `None` 时（罕见：完全无可用网卡），底部显示灰色 `IP 不可用`
- 数值响应窗口生命周期：每次 `window-shown` 事件 + 应用启动时刷新一次（不需要持续轮询）

**out of scope**（v2 这个 feature 不做）：
- 同时展示多张网卡 IP 让用户挑（v0 只挑一个，v2 同；多 IP 选择留 P2 视用户反馈）
- 自动从 Clash / VPN 检测并提示用户"你正在 VPN 下"（仅靠网卡名过滤）
- IPv6 支持
- 二维码生成（让 B 用手机扫；v2 不做，需要相机权限或第三方库）
- 自动 `ip:port` 链接共享给同 LAN 的对端（与"无自动发现"原则冲突）

## 4. 验收标准（Definition of Done）

- [ ] 在一台正常 WiFi 联网的 Mac/Win 上启动应用，浮窗左下角显示一个形如 `192.168.1.42:5858` 的地址
- [ ] 在装了 Clash 且 fake-IP 模式开启的 Mac 上启动，底部仍显示真实 LAN IP（如 `192.168.1.x`），不显示 `198.18.x.x`
- [ ] 在装了 Docker Desktop / WSL2 的 Win 上启动，底部不显示 `172.x.x.x` 的虚拟网卡 IP（除非真没别的可选）
- [ ] 单击底部 IP:PORT 区域，系统剪切板内容变为该字符串，旁边短暂显示绿色 `已复制`，1.5 秒后消失
- [ ] 完全断网时（没有任何符合条件的网卡），底部显示灰色 `IP 不可用` 而不是空字符串或异常
- [ ] 用户在 `settings-panel` 修改端口后（P1 阶段），底部 PORT 立即更新到新值

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的
`legacy-prototype` 分支 `src-tauri/src/commands.rs` 的 `get_local_ip` 函数：枚举 `if_addrs::get_if_addrs()`，对每条网卡跑过滤（loopback、虚拟名、APIPA、198.18/19）→ 提取 IPv4 → 算优先级（192.168 → 0、10 → 1、172.16-31 → 2、其它 → 3）→ 按最低优先级值取胜。`Cargo.toml` 的 `if-addrs = "0.13"`。前端在 `+page.svelte` 底部把 IP 渲染为可点击元素，绑 click → `navigator.clipboard.writeText` → 一个 `$state` 控制的"已复制"toast。

### 5.2 v0 暴露的具体坑
- 网卡名过滤是经多次用户反馈才覆盖到 `awdl / llw / utun`（Mac VPN 与 AirDrop 的虚拟接口）的，每次新平台 / 新 VPN 软件可能引入新名字
- 198.18/19 是 Clash 默认 fake-IP CIDR，但 Clash 用户可自定义到别的段——过滤策略仍是"已知坏例黑名单"，理论上漏掉
- 用户如果**所有**网卡都被过滤掉（如断网），v0 返回 `None`，前端 UI 处理过但消息不够明确（`IP 未知`）
- v0 在前端用 `navigator.clipboard.writeText` 而非 Tauri 命令——浏览器在某些场景（无焦点 / 非用户手势）会拒绝；体验有时不一致

### 5.3 v2 应继承
- 黑名单过滤策略与优先级排序（与 v0 完全一致）
- `if-addrs` 0.13 依赖
- 单击底部 IP 复制 + 微提示
- 仅在 `window-shown` / 启动时刷新一次（不持续轮询）

### 5.4 v2 应挑战
- 是否在前端用 Tauri 的 `clipboard-manager` plugin 写剪切板，避免 web `navigator.clipboard` 的焦点权限坑？
- 是否给"无 IP 可用" / "可能在 Clash fake-IP 下"提供更主动的提示（toast 或链到 FAQ）？
- 是否记忆"上次成功使用的网卡名"作为下次启动的优先级 hint，避免多网卡切换时 IP 抖动？
- v0 把 `get_local_ip` 放在 `commands.rs` 与一堆其它命令同文件——v2 是否单独成 `network/lan_ip.rs` 模块？

## 6. UX 段（占位）

> 待 ux-designer 在后续阶段填写。建议覆盖：
> - 底部 IP 区域的视觉权重：用户要能"扫一眼就看见"，但又不能盖过历史列表的主体性
> - "已复制"微提示的位置（贴在 IP 旁还是 toast 风格的全局横幅？）
> - "IP 不可用" 状态的视觉处理（灰色文字 + 问号提示？）

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 1 条] [P1 2 条] [P2 2 条]

- [P0] [架构师] PORT 显示的来源是 `Config.port` 还是 `state.actual_listening_port`（启动时端口被占用可能 fallback 到其它端口）？v0 仅显示 Config 值——可能与实际监听端口不一致；与 `group-discovery` 第 7 节 [P1] 端口 fallback 议题联动
- [P1] [架构师] 是否把虚拟网卡名黑名单提升为可配置（`config.json` 里允许用户加自定义关键字）？v0 硬编码会随时间僵化
- [P1] [安全] IP 在剪切板里是非敏感信息（同 LAN 内）但仍属"地址泄露"——若用户在公司机器误把 IP 复制到聊天软件无害，可不特殊处理；但 spec 里要明确这一立场
- [P2] [架构师] `get_local_ip` 仅返回单值还是返回 `Vec<(priority, ip)>` 让前端展示并允许用户切换？v0 单值，简单——v2 是否升级？
- [P2] [UX] 复制后的视觉反馈停留时长（1.5s vs 2.5s）需 UX 拍板

## 8. Review 段（占位）

> code-reviewer / tech-architect 后续填写。
