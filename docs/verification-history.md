# 历史验证记录

本文件记录开发和维护过程中的验证结果，用于追溯。当前版本的验证命令见
[development.md](development.md)；已知限制见 [troubleshooting.md](troubleshooting.md)。

## 当前状态（2026-11-22）

| 检查 | 结果 |
| --- | --- |
| 前端测试 | 15 passed |
| 前端构建 | 通过（131 modules） |
| Rust 测试 | 27 passed |
| rustfmt / clippy | 通过 |
| cargo-deny / cargo-audit | 通过 |
| 前端覆盖率 | 约 20% statements |
| Rust 覆盖率 | 约 39.8% regions / 38.9% lines |
| release 二进制构建 | `pnpm tauri build --no-bundle` 通过 |

未完成：GitHub Actions 首次实际运行、macOS 真机回归、安装包签名/公证。

## 2026-09-30：Windows 11 基线

### 构建与测试

| 项目 | 结果 |
| --- | --- |
| `cargo check --all-targets` | 0 error / 0 warning |
| `cargo build`（MSVC） | 通过 |
| `cargo test` | 9 passed |
| `pnpm build` | 通过 |

### 功能验证

在 Windows 11 实机上通过 UI 操作、文件与进程断言验证：

- 启动加载、托盘创建与 `tunnels.json` 读取；
- 运行中、已停止、失效 PID 的状态显示与按钮切换；
- ssh 进程外部退出后，3 秒轮询内转为已停止；
- 关闭转发时进程终止、记录保留与 Toast；
- 删除已停止记录后进入空状态；
- 新建弹窗的命令预览、端口校验与无 Host 提示；
- 深/浅主题切换与 localStorage 持久化；
- 关闭窗口隐藏到托盘、托盘图标点击切换窗口；
- 3 秒轮询期间状态与计数一致。

## 同日调整

| 项目 | 结果 |
| --- | --- |
| 取消窗口置顶 | `WS_EX_TOPMOST` 已清除 |
| Windows 无边框与自绘标题栏 | 拖拽、双击最大化、最小化/最大化/还原可用 |
| 绑定地址新增 `localhost` | 下拉项与联动规则生效 |
| 托盘菜单新增“退出应用（关闭隧道）” | 退出前终止运行中的 ssh，日志记录数量 |
| 项目迁移到当前工作目录 | 迁移后重新构建与实机运行通过 |

## 后续调整

| 项目 | 结果 |
| --- | --- |
| 关闭后保留记录 | `stop_tunnel` 保留记录；`restart_tunnel` 复用；`remove_tunnel` 仅删除已停止记录 |
| 标题去重 | 自绘标题栏保留窗口标题，内容区不再重复大标题 |
| 标题栏按平台拆分 | Windows 自绘；macOS 使用原生标题栏 |
| macOS 图标栅格 | `.icns` 改为 824-on-1024 栅格，PNG/ICO 保持满画布 |
| macOS 菜单栏图标 | 使用 36×36 单色模板图，`icon_as_template(true)` |
| 密码预检 | `ssh -T` 预检后才建立隧道 |
| 成功判定 | 以本地端口监听为准，替代固定时间窗口 |

## 代码复查与清理

2026-09-30 对 Rust 与前端源码进行了规范性、安全性和逻辑复查，主要包括：

- 日志初始化先创建数据目录，保证首次启动写入 `app.log`；
- `start_tunnel` / `restart_tunnel` 保存失败时回滚内存记录并回收子进程；
- 抽取 `kill_unstarted()`，统一 kill + wait，避免僵尸句柄；
- 短 ID 生成时排重；
- 端口冲突判定统一为 `local_port_taken`；
- 清理未使用的图标、`data_path` 命令与多余窗口权限；
- 修正与实现不一致的注释和 README 表述。

详细变更记录见 [CHANGELOG.md](../CHANGELOG.md)。

## 验证范围说明

历史记录中的 E2E 验证主要覆盖 Windows。macOS 的 `libc::kill`、`ps`、ICNS 与原生
标题栏路径按平台条件编译实现，但仍需在 macOS 12+ 上执行 `pnpm tauri dev` 与
`pnpm tauri build` 做最终回归。
