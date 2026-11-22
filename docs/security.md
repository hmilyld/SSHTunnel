# 安全设计

安全漏洞报告流程见仓库根目录 [SECURITY.md](../SECURITY.md)。本文件说明当前实现的
安全边界与已知权衡。

## 凭据处理

- 密码仅通过 `SSH_ASKPASS`、`SSH_ASKPASS_REQUIRE=force` 与 `STM_ASKPASS_PWD`
  注入 ssh 子进程环境；
- 密码不写入 `tunnels.json`、不写入 `app.log`；
- askpass 辅助模式只向 stdout 输出密码后退出，不启动 GUI、不初始化日志；
- `main.rs` 仅在“存在参数且设置了 `SSH_ASKPASS`”时进入辅助模式。

密码会短暂存在于本进程与 ssh 子进程的环境变量中。同一用户的其他进程在操作系统
权限允许时可能读取该环境，这是当前设计的已知权衡。

## 命令与参数

所有外部命令通过参数数组执行，不拼接 shell 字符串：

- `ssh`
- `tasklist` / `taskkill`（Windows）
- `powershell`（仅端口占用错误路径）
- `lsof` / `ss`（Unix）

端口以 `u16` 传递。Host、绑定地址与远程主机会拒绝以 `-` 开头或包含空白、控制字符
的输入，避免 ssh 参数注入。合法输入的行为保持不变。

终止进程前会校验进程名是 `ssh`（或 `ssh.exe`），避免 PID 复用导致误杀其他进程。
`reveal_data_dir` 只打开后端解析的数据目录，不接受前端传入路径。

## 本地数据

`tunnels.json` 使用临时文件加 rename 的原子替换；损坏文件会备份为 `.bak`。
保存失败时恢复内存快照，避免内存与磁盘状态不一致。

日志文件位于应用数据目录，启动时超过 5 MB 轮转为 `app.log.1`。日志不记录密码。

## WebView 与 CSP

`tauri.conf.json` 启用了 CSP：

- `default-src` 限制为 `'self'` 与 Tauri 自定义协议；
- `connect-src` 允许 IPC 与本地开发地址；
- `img-src` 允许本地资源与 `data:`；
- `style-src` / `script-src` 当前包含 `'unsafe-inline'`，用于内联主题脚本与运行时样式。

后续可将内联脚本外置，并使用 hash 或 nonce 收紧 `script-src`。当前未加载远程内容，
未启用 `withGlobalTauri`，能力集仅包含主窗口所需权限。

## 供应链

- npm 依赖使用 `pnpm-lock.yaml`，CI 使用 `--frozen-lockfile`；
- Rust 依赖使用 `Cargo.lock`；
- CI 运行 `cargo-deny`（许可证、漏洞、来源）与 `cargo-audit`；
- 前端依赖运行 `pnpm audit --audit-level high`；
- 本地时间格式化使用维护中的 `jiff`，不再依赖 unmaintained 的 `chrono`。

许可证 allowlist 位于 `src-tauri/deny.toml`，当前允许 MIT、Apache-2.0、BSD-3-Clause、
ISC、MPL-2.0、Unicode-3.0、Zlib 等常见许可证。新增依赖需要检查许可证与安全影响。

## 已知限制

- macOS 安装包未签名、未公证；
- macOS 上 askpass 路径包含空格时的行为尚未在真机验证；
- CSP 的 `'unsafe-inline'` 仍是后续收紧项；
- GitHub Actions 首次实际运行与 macOS 回归需在对应环境完成。
