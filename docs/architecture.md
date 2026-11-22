# 架构

## 技术栈

| 层 | 技术 |
| --- | --- |
| 桌面框架 | Tauri 2（Rust 后端 + WebView 前端） |
| 前端 | React 19、TypeScript（strict）、Tailwind CSS 4、shadcn 风格组件、Vite |
| 后端 | Rust 2021、Tokio、Serde、tracing、thiserror |
| SSH | 系统 `ssh`（OpenSSH），以子进程方式运行 `ssh -L` |
| 持久化 | 平台应用配置目录下的 `tunnels.json` |
| 测试 | Vitest + Testing Library；Rust 单元测试；cargo-llvm-cov |

## 目录结构

```text
.
├── src/                    React 前端
│   ├── components/         页面组件、基础 UI 组件与测试
│   ├── hooks/              主题等 React Hooks
│   ├── lib/                API 封装、表单纯函数、平台判定
│   ├── styles/             设计令牌与全局样式
│   └── test/               Vitest/jsdom 初始化
├── src-tauri/              Rust/Tauri 后端
│   ├── capabilities/       Tauri 权限能力
│   ├── icons/              应用与托盘图标
│   ├── src/                后端模块
│   ├── Cargo.toml
│   ├── deny.toml
│   └── tauri*.conf.json
├── scripts/                图标生成/校验与 UI 验证辅助脚本
├── docs/                   用户与维护者文档
├── .github/                CI、发布、Issue/PR 模板
└── README.md               项目入口与文档索引
```

构建配置、格式化与测试配置位于仓库根目录：`vite.config.ts`、`vitest.config.ts`、
`eslint.config.js`、`.prettierrc.json`、`tsconfig*.json`、`.nvmrc`、`rust-toolchain.toml`。

## 模块边界

### 前端

| 模块 | 职责 |
| --- | --- |
| `src/App.tsx` | 页面编排、轮询、事件订阅、弹窗与操作状态 |
| `src/lib/api.ts` | Tauri 命令的类型化封装与密码错误前缀解析 |
| `src/lib/tunnelForm.ts` | 端口校验、远程主机与绑定地址联动、命令预览（纯函数） |
| `src/lib/platform.ts` | 平台判定，决定是否渲染自绘标题栏 |
| `src/components/` | 标题栏、表格、新建/修改弹窗、密码弹窗、状态栏、主题切换 |
| `src/components/ui/` | shadcn 风格基础组件 |
| `src/hooks/useTheme.ts` | 深/浅主题与 localStorage 持久化 |

### 后端

| 模块 | 职责 |
| --- | --- |
| `commands.rs` | Tauri 命令层，参数与返回值直接对接前端 |
| `tunnel.rs` | 隧道生命周期编排、状态视图、持久化与事件广播 |
| `ssh_start.rs` | askpass 环境、密码预检、本地端口监听探测、`ssh -L` 启动 |
| `ssh_failure.rs` | ssh 退出状态与 stderr 归因、密码认证判定 |
| `ssh_config.rs` | `~/.ssh/config` 解析与 `Include` 展开 |
| `store.rs` | `tunnels.json` 读写、原子替换、损坏备份与回滚 |
| `process.rs` | 跨平台进程检测、端口占用查询与终止 |
| `tray.rs` | 系统托盘菜单与窗口切换 |
| `logging.rs` | stdout 与 `app.log` 双写，启动时日志轮转 |
| `error.rs` | 应用错误类型，序列化为中文文案 |
| `lib.rs` / `main.rs` | Builder 组装、单实例、命令注册与进程入口 |

## 数据流

```text
React UI
  │ invoke
  ▼
commands.rs
  │
  ├──▶ tunnel.rs ──▶ ssh_start.rs ──▶ tokio::process ssh -L
  │        │
  │        ├──▶ store.rs      tunnels.json（原子写）
  │        ├──▶ process.rs    进程存活检测 / 终止
  │        └──▶ ssh_failure.rs 退出归因
  │
  ├──▶ ssh_config.rs          Host 列表与 Include
  └──▶ data_dir / reveal_data_dir

启动后：
  前端每 3 s 调用 list_tunnels
  ssh 退出时后端 emit "tunnel-exited"，前端立即刷新
```

窗口隐藏时前端暂停轮询；恢复可见时立即刷新一次。

## 隧道生命周期

| 操作 | 行为 |
| --- | --- |
| 新建 | 校验参数 → 启动 ssh → 等待本地 `bind:port` 进入监听 → 写入记录 |
| 关闭 | 优雅终止，超时后强制终止；保留记录并将 `pid` 置 0 |
| 重启 | 复用已保存记录重新启动 ssh；ID 与创建时间不变 |
| 修改 | 仅修改已停止记录的配置；运行中的记录拒绝修改 |
| 删除 | 仅允许删除已停止记录；前端二次确认 |
| 状态 | `alive = pid > 0 && 进程名为 ssh`；PID 被复用时不误判 |

启动成功以本地端口实际进入监听为准，不以固定等待时间判断。ssh 进程退出时会
解析 stderr，归类为端口占用、认证失败、连接拒绝、主机名解析失败等错误。
监听出现后会额外确认 ssh 进程仍存活，避免端口被其他进程抢占时误报成功。

本地端口预检会查询占用进程名与 PID；Windows 使用 `Get-NetTCPConnection`，
Unix 使用 `lsof` 或 `ss`。该查询仅发生在错误路径。

## Tauri 命令

| 命令 | 参数 | 返回 |
| --- | --- | --- |
| `list_hosts` | 无 | `SshHost[]` |
| `list_tunnels` | 无 | `Tunnel[]`（记录加 `alive`） |
| `start_tunnel` | `{ request, password? }` | `Tunnel`；需要密码时返回 `PASSWORD_REQUIRED::…` |
| `stop_tunnel` | `{ id }` | `Tunnel[]`，保留记录 |
| `restart_tunnel` | `{ id, password? }` | `Tunnel[]` |
| `update_tunnel` | `{ id, request }` | `Tunnel[]` |
| `remove_tunnel` | `{ id }` | `Tunnel[]` |
| `ssh_config_path` | 无 | `string` |
| `data_dir` | 无 | `string` |
| `reveal_data_dir` | 无 | `()`，仅打开后端解析的数据目录 |

命令错误统一序列化为中文字符串，前端直接展示。字段命名与 Rust 端一致，使用
snake_case。

## 平台差异

| 项目 | Windows | macOS |
| --- | --- | --- |
| 标题栏 | 无边框窗口，自绘标题栏（拖拽、双击最大化、最小化/最大化/关闭） | 系统原生标题栏 |
| 托盘 | 彩色应用图标 | 单色模板图标，`icon_as_template(true)` |
| 子进程 | 使用 `CREATE_NO_WINDOW`，不创建控制台窗口 | 使用系统 `ssh` |
| 应用图标 | PNG/ICO 满画布 | ICNS 使用 Apple 824-on-1024 栅格 |

平台配置按 JSON Merge Patch（RFC 7396）合并，数组整体替换。`tauri.windows.conf.json`
中的窗口对象需要与 `tauri.conf.json` 保持字段同步，否则 Windows 会使用平台文件中的值。

单实例由 `tauri-plugin-single-instance` 提供：第二次启动聚焦已有主窗口，避免多个
GUI 进程同时写入 `tunnels.json`。日志超过 5 MB 时，启动阶段将 `app.log` 轮转为
`app.log.1`。

数据格式与 `~/.ssh/config` 细节见 [configuration.md](configuration.md)。
