## 变更摘要

<!-- 这个 PR 做了什么？为什么？ -->

## 类型

- [ ] Bug fix
- [ ] Feature
- [ ] Refactor / 工程化
- [ ] Docs
- [ ] CI / Release
- [ ] Dependencies / Security

## 验证

<!-- 粘贴实际运行的命令与结果；没有运行的说明原因 -->

```bash
pnpm format:check
pnpm lint
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

## 兼容性

- [ ] Tauri 命令 / `PASSWORD_REQUIRED::` 前缀不变
- [ ] `tunnels.json` 字段 / 数据目录不变
- [ ] `~/.ssh/config` 解析与 `ssh -L` 参数不变
- [ ] 窗口 / 托盘 / 安装包标识不变
- [ ] 有破坏性变更（请说明迁移方案）

## 截图 / 日志

<!-- 已脱敏；不要包含密码、私钥、token、完整配置文件 -->

## Checklist

- [ ] 已阅读 CONTRIBUTING.md
- [ ] 已补/更新测试（如适用）
- [ ] 已更新 CHANGELOG.md（如适用）
- [ ] 未提交密钥、凭证或个人信息
