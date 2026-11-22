# 开发

## 环境要求

| 平台 | 依赖 |
| --- | --- |
| 通用 | Rust 1.90+、Node.js 22.22+、pnpm 11+、系统 `ssh`（OpenSSH） |
| Windows 10/11 | Visual Studio 2022（使用 C++ 的桌面开发）、WebView2 Runtime |
| macOS 12+ | Xcode Command Line Tools（`xcode-select --install`） |

仓库提供 `.nvmrc`（Node 22）与 `rust-toolchain.toml`（stable + rustfmt/clippy）。
CI 固定 pnpm 12.5.1。

## 安装与开发

```bash
pnpm install
pnpm tauri dev
```

也可以分开运行：终端 A 执行 `pnpm dev`，终端 B 执行
`cargo run --manifest-path src-tauri/Cargo.toml`。`tauri.conf.json` 的 `devUrl`
指向 `http://localhost:5173`。

## 验证命令

```bash
pnpm format:check
pnpm lint
pnpm test
pnpm test:coverage
pnpm build
pnpm audit --audit-level high

cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml

cd src-tauri && cargo deny check
cd src-tauri && cargo audit
cargo llvm-cov --manifest-path src-tauri/Cargo.toml --lcov --output-path lcov.info
```

`cargo llvm-cov` 需要 `llvm-tools-preview` 与 `cargo-llvm-cov`。前端覆盖率使用
`@vitest/coverage-v8`。覆盖率报告上传到 CI artifact，当前不设硬阈值。

## 代码风格

- 前端使用 Prettier 与 ESLint flat config；不要用 `any` 或 `@ts-ignore` 掩盖类型错误。
- Rust 使用 `cargo fmt` 与 clippy；不要把 `-D warnings` 降级为全局 allow。
- 用户可见文案与错误信息使用中文。
- 新增纯逻辑优先放入 `src/lib/`，并补充 Vitest 测试。
- 涉及 Store、时间、进程的 Rust 测试不得使用真实用户数据或终止真实进程。

## CI

`.github/workflows/ci.yml` 在 push 到 `main` 与 PR 时运行：

- 前端：安装依赖、Prettier、ESLint、Vitest + 覆盖率、npm audit、构建；
- Rust 格式：`cargo fmt --check`；
- Rust 检查与测试：Windows 与 macOS 矩阵，执行 clippy 与 test；
- 依赖审计：cargo-deny、cargo-audit；
- Rust 覆盖率：cargo-llvm-cov，上传 LCOV。

发布工作流复用同一份 CI 作为质量门禁，见 [release.md](release.md)。

## 图标

```bash
node scripts/gen-icons.mjs
node scripts/verify-icons.mjs
```

`gen-icons.mjs` 生成 PNG、ICO、ICNS 与 macOS 单色托盘模板图。`verify-icons.mjs`
检查颜色、透明度与不透明区边距。ICNS 使用 Apple 824-on-1024 栅格；PNG/ICO 保持满画布。

## 目录约定

- 应用源码位于 `src/` 与 `src-tauri/src/`；
- 用户与维护者文档位于 `docs/`；
- 图标生成/校验与 UI 验证脚本位于 `scripts/`；
- CI、发布与协作模板位于 `.github/`；
- 构建产物、覆盖率与日志不提交（见 `.gitignore`）。
