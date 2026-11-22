# Changelog

本项目遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 与
[Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### Added

- 新增 MIT `LICENSE`，并在 `Cargo.toml` 声明 `license = "MIT"`。
- 新增 push/PR CI：Prettier、ESLint、Vitest、构建、依赖审计、Rust fmt/clippy/test；
  发布前复用同一套质量门禁。
- 新增 ESLint flat config、Prettier 配置与格式化/lint 脚本。
- 新增 Vitest + Testing Library 前端测试（端口校验、绑定联动、命令预览、密码前缀/弹窗）。
- 新增 Rust 测试：Store 保存/加载/损坏备份/保存失败回滚、端口校验、Host 参数校验、
  ssh_config 重复 Host 语义、日志轮转。
- 新增单实例：第二次启动聚焦已有窗口，避免并发写 `tunnels.json`。
- `app.log` 超过 5 MB 时在启动阶段轮转为 `app.log.1`。
- 新增 `cargo-deny`（许可证/漏洞/来源）与 `cargo-audit` CI 检查。
- 新增 Rust 覆盖率（cargo-llvm-cov）并上传 LCOV 报告。
- 启用 Tauri CSP（限制远程资源，保留 IPC/内联样式与脚本所需来源）。
- 新增 `SECURITY.md`、`CONTRIBUTING.md`、`CODE_OF_CONDUCT.md`、Issue 模板与 PR 模板。

### Changed

- 包管理统一为 pnpm，删除 `package-lock.json`；CI 与 Tauri `beforeBuildCommand` 均使用 pnpm。
- 最低 Rust 版本提升到 1.90（单实例插件要求）；新增 `rust-toolchain.toml`。
- Node 要求提升到 22.22+（Vitest 5/jsdom 31），新增 `.nvmrc` 与 `engines`。
- macOS 发布矩阵增加 Intel `x86_64-apple-darwin`。
- Release 资产文件名改为 `SSHManager_<version>_<platform>_<arch>...`，仅改下载名，不动安装身份。
- `~/.ssh/config` 的重复 `Host` 块改为“每个参数首个值生效”，与 OpenSSH 一致；
  明确当前不展开 `Include`。
- 启动判定先检查 ssh 子进程状态，并在本地端口监听后加短暂存活确认，降低端口被抢占时的假成功。
- 命令预览可选中/一键复制；删除记录前二次确认；轮询失败只提示一次；窗口隐藏时暂停轮询。

### Fixed

- 升级 Vite 到 6.4.6、`source-map-js` 到 1.2.4，清除 high 级审计告警。
- 升级 tokio 到 1.54.1、`yoke-derive` 到 0.8.4，修复 RustSec / yanked 告警。
- `stop/update/remove` 保存失败时恢复内存快照，避免 UI 与磁盘状态分叉。
- 校验 Host/Bind/Remote，拒绝以 `-` 开头或含空白/控制字符的输入，防止 ssh 参数注入。
- 修正 `cargo fmt` / clippy 基线问题（含一处 `manual_map`）。

## [1.0.2] - 2026-10-05

### Fixed

- macOS 菜单栏改用单色模板图标（`icon_as_template(true)`），不再在菜单栏放彩色应用图标。

## [1.0.1] - 2026-10-05

### Fixed

- macOS `.icns` 按 Apple 824-on-1024 栅格渲染，避免 macOS 26 的灰色底框（icon jail）。
- macOS 使用系统原生标题栏；自绘标题栏仅保留给 Windows。

## [1.0.0] - 2026-10-01

### Added

- 跨平台 SSH 本地端口转发管理器（Tauri 2 + React 19 + Rust）。
- 新建/启动/关闭/重启/修改/删除转发，状态轮询与 `tunnel-exited` 事件。
- 密钥与密码认证、`SSH_ASKPASS` 注入、本地端口监听成功判定。
- 系统托盘、深浅主题、底部版本/版权/数据目录入口。
- GitHub Actions tag 发布 Windows NSIS/MSI 与 macOS DMG。

### Fixed

- 审计修复、版权信息与底部信息栏重设计。

[Unreleased]: https://github.com/hmilyld/SSHTunnel/compare/v1.0.2...HEAD
[1.0.2]: https://github.com/hmilyld/SSHTunnel/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/hmilyld/SSHTunnel/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/hmilyld/SSHTunnel/releases/tag/v1.0.0
