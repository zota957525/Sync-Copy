---
status: SPEC_DRAFTED
owner: product-strategist
related_adrs: []
related_specs: [00-product-overview]
created: 2026-05-06
updated: 2026-05-06
revised: 2026-05-06 — P1-5 一致性 review
priority: P0
---

# cross-platform-build — Mac universal + Windows x64 双平台 CI 构建与产物分发

## 1. 问题（为什么做）

Sync Copy 是个人桌面工具，用户安装路径是"GitHub Releases / Actions artifacts 下载安装包，双击安装"——没有 brew/winget/store。开发者本机只有一个平台的工具链，手工跑两遍 build 既慢又易出错；而且 macOS universal binary 必须有 aarch64+x86_64 双 target 工具链，本地配齐是负担。CI 在每次 push main 后自动跑出"版本号命名"的安装包 + 免安装版（macOS `.app.zip` / Windows raw `.exe`），点开 Actions 页面就能直接下载。是 v2 第一个落地的 feature——后续每个 feature 都依赖它把改动跑过 CI 才合并。

## 2. 用户故事

- As a maintainer, I want every push to main to produce downloadable Mac+Win artifacts within ~10 minutes, so that I can hand "this commit's build" to a tester without local cross-compile.
- As an end user, I want to download a versioned, double-click-installable file (`.dmg` / `-setup.exe`) or a portable variant (`.app.zip` / raw `.exe`) without unpacking complex archives.

## 3. 范围

**in scope**：
- GitHub Actions workflow 在 push main / PR / `workflow_dispatch` 触发
- 矩阵构建：`macos-latest` (`universal-apple-darwin` target) + `windows-latest` (默认 x64)
- 产物命名按统一约定：`SyncCopy-v<version>-<platform>-<variant>.<ext>`（version 来自 `package.json`）
- macOS 输出：`-macos-universal.dmg` + `-macos-universal-portable.app.zip`
- Windows 输出：`-win-x64-setup.exe`（NSIS）+ `-win-x64.msi` + `-win-x64-portable.exe`（raw binary）
- artifacts 通过 `actions/upload-artifact@v4` 暴露，保留 14 天
- Rust 缓存（`Swatinem/rust-cache@v2`）+ npm 缓存
- 自动从 `package.json` 读 version 注入产物名

**out of scope**（v2 这个 feature 不做）：
- 代码签名 / 公证（Apple notarization、Windows Authenticode）——v2 不申请开发者证书，用户首次启动看到"未识别开发者"对话框是已知体验
- 自动发布到 GitHub Releases（仍是 artifacts；正式 release 由 release-engineer 在 P5-4 里手工 tag）
- Linux 构建（见总览 第 3 节 out of scope）
- 自动版本号 bump / changelog 生成（独立 release 流程负责）
- 增量构建 / sccache（Swatinem cache 已够用）

## 4. 验收标准（Definition of Done）

- [ ] push 一个普通 commit 到 main，10 分钟内 Actions 页面出现两个绿勾构建（mac + win），各上传一份命名规范的 artifact 包
- [ ] 下载 `SyncCopy-v<version>-macos-universal.dmg`，在 Apple Silicon 与 Intel Mac 上双击均能装
- [ ] 下载 `SyncCopy-v<version>-win-x64-setup.exe`，在 Win10/11 x64 上双击安装并能启动
- [ ] 下载 portable 变体（`.app.zip` 解压即跑 / raw `.exe` 双击即跑），无需安装也能运行一次完整剪切板同步流程
- [ ] 任一构建失败时 workflow 报错信息明确指向失败步骤（不是黑盒）
- [ ] CI 运行 ≤ 15 分钟（含两个 OS 并行；Rust cache 命中后单 job ≤ 8 分钟）

## 5. v0 历史 / 已知坑

### 5.1 v0 怎么做的
`legacy-prototype` 分支 `.github/workflows/build.yml` 已实现矩阵构建：`actions/checkout@v4` → `setup-node@v4` (Node 20) → `dtolnay/rust-toolchain@stable` (extra_targets: `aarch64-apple-darwin,x86_64-apple-darwin` 仅 mac) → `swatinem/rust-cache@v2` → `npm ci` → 读 `package.json` version → `npm run tauri build` → 一组 shell 步骤把产物 cp 到 `dist/` 重命名 → `upload-artifact`。`tauri.conf.json` 的 `bundle.targets="all"`、`bundle.icon` 列出两套图标。

### 5.2 v0 暴露的具体坑
- raw exe 路径 `src-tauri/target/release/sync-copy.exe` 是个隐式约定（来自 `Cargo.toml` 的 `name = "sync-copy"`），改 crate name 会静默断 portable 变体
- portable `.app.zip` 用 `zip -r -y` 保留符号链接是踩过坑后才加的；早期版本压缩出来的 .app 在 Mac 上启动会因为缺 `Frameworks/` 软链而失败
- 没有签名 → Mac 用户看到"未识别开发者"必须右键打开；Windows 用户看到 SmartScreen 警告必须"仍然运行"——文档写过但用户首次仍困惑
- workflow 没跑 `npm run check` / `cargo clippy` / 任何测试，CI 仅是"能 build 出来"，不是"代码健康"
- 没有矩阵失败 fast-fail（`fail-fast: false` 是故意的）但也没有失败摘要 comment

### 5.3 v2 应继承
- 矩阵：mac universal + win x64（v2 不扩 Linux）
- 产物命名 schema `SyncCopy-v<version>-<platform>-<variant>.<ext>`
- `Swatinem/rust-cache@v2` + npm cache
- `tauri.conf.json` 的 `bundle.targets="all"`
- 14 天 artifact 保留

### 5.4 v2 应挑战
- 是否在 build 之前加 lint / clippy / test gate？（CI 不只是"能编译"——也应是"代码健康度门槛"）
- 是否给出 SHA256 校验文件让用户验证 artifact 完整性？
- 是否支持 `workflow_dispatch` 时手动指定 version 而不是仅读 package.json（用于 hot patch）？

## 6. UX 段（占位）

> 本 feature 不涉及应用 UI；但 GitHub Actions 页面下载体验属于"项目对外门面"，可由 docs-writer 在 README 写一段"如何下载"对应。第 6 节 内 UI 部分 N/A。

## 7. 已知风险 / 未决问题

> **优先级统计**：[P0 1 条] [P1 3 条] [P2 1 条]

- [P0] [架构师] `package.json` 的 version 与 `Cargo.toml` 的 version 在 v0 是手工同步的（两处都写 0.1.0），v2 是否改为单一来源（package.json 注入到 Cargo）？版本号不一致直接破坏产物命名约定
- [P1] [架构师] 是否在 build 前增加 `cargo clippy --deny warnings` 与 `npm run check` 作为 CI gate？是否在第一版就引入还是留 P1 加？
- [P1] [安全] artifact 是否生成 SHA256SUMS 并打到 release 页面？没有签名时这是用户唯一能校验完整性的手段
- [P1] [架构师] `tauri-action` 官方 action vs 手写步骤——v0 选了手写，v2 是否切换以减少维护？两者对产物命名的可控性差异需评估
- [P2] [架构师] runner 池子选 `macos-latest`（M1 runner，有时排队）还是固定到 `macos-14` / `macos-15` 显式版本？后者可重现性更好但需要定期 bump

## 8. Review 段（占位）

> code-reviewer / tech-architect 后续填写。
