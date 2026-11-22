# Contributing

感谢参与 SSH 隧道管理器。请先阅读本文件与 [README](README.md)、
[SECURITY](SECURITY.md) 和 [CODE_OF_CONDUCT](CODE_OF_CONDUCT.md)。

## 开发环境

- Node.js **22.22+**（CI 使用 Node 22；仓库提供 `.nvmrc`）；
- pnpm **11+**（CI 固定 12.5.1）；
- Rust **1.90+**（`rust-toolchain.toml` 会启用 stable + rustfmt/clippy）；
- 系统 `ssh`（OpenSSH）；
- Windows：Visual Studio 2022（C++ 桌面开发）、WebView2 Runtime；
- macOS：Xcode Command Line Tools。

## 本地命令

```bash
pnpm install
pnpm tauri dev

pnpm format:check
pnpm lint
pnpm test
pnpm build

cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cd src-tauri && cargo deny check
cd src-tauri && cargo audit --ignore RUSTSEC-2026-0543
```

CI 会运行同一套检查；提交前请确保本地通过。

## 代码风格

- 前端：Prettier（`printWidth: 100`）+ ESLint flat config；不要用 `any` /
  `@ts-ignore` 掩盖类型问题。
- Rust：`cargo fmt` + clippy（`-D warnings`）；不要用全局 `allow` 静默警告。
- 注释与用户可见文案使用中文；错误信息保持面向用户、可直接展示。
- 不提交 `node_modules`、`dist`、`src-tauri/target`、`coverage` 或截图产物。

## 兼容性约束

以下内容默认必须保持兼容，除非 PR 明确说明并提供迁移方案：

- Tauri 命令名、参数、返回结构；
- `PASSWORD_REQUIRED::` 错误前缀；
- `tunnels.json` 字段、`created_at` 格式、应用数据目录；
- `~/.ssh/config` 解析语义与 `ssh -L` 参数/超时；
- Windows/macOS 的窗口、托盘与安装包标识。

## 测试

- 前端：Vitest + Testing Library；新增纯逻辑请放到 `src/lib/` 并补测试。
- Rust：优先补单元测试；涉及 Store/时间/进程的测试不要触碰真实用户数据或杀真实进程。
- 手工 UI 验证可使用 `scripts/screenshot.ps1` / `scripts/uiclick.ps1`。

## 提交与 PR

- 使用 Conventional Commits：`feat(...)` / `fix(...)` / `docs(...)` / `chore(...)` /
  `ci(...)`。
- 一个 PR 只做一件事；格式化、重构、行为修复分开。
- PR 描述请包含：变更摘要、验证命令与结果、兼容性影响、截图/日志（已脱敏）。
- 依赖变更请同时说明理由、许可证与安全影响；不要擅自升级大版本。
- 安全漏洞请走私密渠道（见 [SECURITY](SECURITY.md)），不要开公开 Issue。

## 发布

发布由维护者推送 `vX.Y.Z` tag 触发；版本号需同时更新
`package.json`、`src-tauri/tauri.conf.json` 和 `src-tauri/Cargo.toml`。
