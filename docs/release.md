# 发布

## 版本号

发布前同时更新以下文件的 `version`：

- `package.json`
- `src-tauri/tauri.conf.json`
- `src-tauri/Cargo.toml`

三处版本必须与 tag 一致。`release.yml` 在编译前执行行级文本校验；任一文件不匹配
都会终止发布。

## 触发方式

推送 `vX.Y.Z` 形式的 tag：

```bash
git tag v1.1.0
git push origin v1.1.0
```

`release.yml` 会以同名 tag 创建或复用 GitHub Release，并上传安装包。

## 质量门禁

发布工作流首先调用可复用的 `ci.yml`，执行：

- Prettier、ESLint、Vitest 与覆盖率；
- 前端构建与 npm 依赖审计；
- Rust fmt、clippy、test（Windows 与 macOS）；
- cargo-deny 与 cargo-audit；
- Rust 覆盖率。

任一门禁失败时，不编译也不发布安装包。

## 产物矩阵

| Runner | Target | 产物 |
| --- | --- | --- |
| `windows-latest` | `x86_64-pc-windows-msvc` | NSIS `*-setup.exe`、WiX `.msi` |
| `macos-latest` | `aarch64-apple-darwin` | `.dmg`（Apple Silicon） |
| `macos-latest` | `x86_64-apple-darwin` | `.dmg`（Intel） |

macOS 安装包当前未签名、未公证。首次打开需要在“系统设置 → 隐私与安全性”中放行。

## 资产命名

Release 资产使用统一命名：

```text
SSHManager_<version>_<platform>_<arch>[setup].<ext>
```

示例：`SSHManager_1.1.0_windows_x64-setup.exe`。

该规则仅影响下载文件名。`productName`、`identifier`、应用数据目录与安装身份保持不变。

## 权限与签名

- 工作流仅使用仓库默认 `GITHUB_TOKEN`，显式声明 `permissions: contents: write`；
- 未配置代码签名证书或公证凭据；
- 未配置 Tauri updater 签名，因此 `uploadUpdaterJson: false`。

如需启用签名、公证或自动更新，应单独配置证书、Secret 与发布流程，并补充迁移说明。
