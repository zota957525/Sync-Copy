# Sync Copy

**局域网多设备剪切板同步工具。同一 WiFi 下，复制即同步，无服务器、端到端加密、人工审批入组。**

[![CI](https://github.com/zota957525/sync-copy/actions/workflows/build.yml/badge.svg)](https://github.com/zota957525/sync-copy/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## 下载

前往 [Releases](https://github.com/zota957525/sync-copy/releases) 下载最新版本：

| 平台 | 文件 |
|---|---|
| macOS (Apple Silicon + Intel) | `SyncCopy-vX.X.X-macOS-universal.dmg` |
| Windows 10/11 x64 | `SyncCopy-vX.X.X-windows-x64-setup.exe` |

---

## 快速开始

1. 在所有设备上安装并启动 Sync Copy
2. 在 A 设备浮窗底部查看本机 `IP:PORT`（如 `192.168.1.50:5858`）
3. 在 B 设备点「加入」，输入 A 的 `IP:PORT`
4. A 收到审批弹框，点「同意」
5. 双方浮窗显示绿色状态点 → 复制任意内容，对方立刻可粘贴

详细使用说明：[使用说明.md](使用说明.md)

---

## 特性

- 文本、PNG 图片、文件（≤5 MB）三类同步
- X25519 + HKDF + AES-256-GCM 端到端加密，LAN 内抓包只见密文
- 人工审批入组，无密码，无账号，无中心服务器
- N 设备 Gossip Mesh 自动扩展，加入任一节点即全组互连
- 心跳掉线检测 + 隐形掉线修复（强制重建 TCP 连接）
- 历史列表最近 50 条，单击复用
- 悬浮球最小化，磨砂玻璃浮窗

---

## 开发

```bash
npm install
npm run tauri dev    # 开发模式（热重载）
npm run tauri build  # 本地构建
cargo test           # 运行 153 条单元测试
```

技术栈：Tauri 2 + SvelteKit 2 + Svelte 5 (runes) + Rust (tokio + axum + reqwest)

架构文档：[项目架构.md](项目架构.md) | 变更记录：[CHANGELOG.md](CHANGELOG.md)

---

## License

MIT
