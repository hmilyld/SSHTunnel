# SSH 隧道管理器（SSH Tunnel Manager）

运行在 **Windows 10/11** 与 **macOS 12+** 的桌面应用，用于图形化管理到 Linux 服务器的
SSH 本地端口转发（`ssh -L`）：新建转发、实时查看状态、一键关闭，并支持最小化到系统托盘。

- GUI 框架：**Tauri 2.0**（Rust 后端 + Web 前端）
- 前端：**React 19 + TypeScript + Tailwind CSS 4 + shadcn/ui 风格组件**（Vite 构建）
- SSH：调用**系统 `ssh` 命令**（`ssh -N -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 -L …`）
- 持久化：JSON 文件（`dirs` crate 解析的应用配置目录）
- 托盘：Tauri 内置 Tray API（左键切换窗口，右键菜单：显示主窗口 / 退出）

---

## 1. 项目结构

```
SSHTunnel/
├── package.json                 # 前端依赖与脚本（pnpm）
├── pnpm-workspace.yaml          # pnpm 11+ 构建脚本放行（allowBuilds: esbuild）
├── vite.config.ts               # Vite + React + Tailwind 插件、@ 别名
├── tsconfig*.json               # TS 严格模式（app / node 两份）
├── index.html                   # 前端入口（首帧前应用主题，避免闪白）
├── scripts/
│   ├── gen-icons.mjs            # 零依赖图标生成（PNG / ICO / ICNS）
│   ├── screenshot.ps1           # 全屏截图（验证用）
│   └── uiclick.ps1              # UI Automation 点击（验证用）
├── src/                         # ---- 前端（React）----
│   ├── main.tsx                 # 前端入口
│   ├── App.tsx                  # 页面主体：轮询、事件、增删改查
│   ├── styles/globals.css       # 设计令牌（深/浅主题 CSS 变量）+ 全局样式
│   ├── lib/
│   │   ├── api.ts               # tauri command 的类型化封装
│   │   └── utils.ts             # cn() 类名合并
│   ├── hooks/useTheme.ts        # 深色/浅色主题（localStorage 持久化）
│   └── components/
│       ├── TitleBar.tsx         # 自绘标题栏（拖拽移动 / 双击最大化 / 最小化·最大化·关闭）
│       ├── AppHeader.tsx        # 内容区顶栏：说明文字 / 刷新 / 新建转发（标题不重复）
│       ├── TunnelTable.tsx      # 隧道列表表格（含骨架屏、空状态）
│       ├── NewTunnelDialog.tsx  # 新建转发弹窗（表单校验 + 命令预览）
│       ├── StatusBar.tsx        # 底部状态栏（计数、路径提示）
│       ├── ThemeToggle.tsx      # 主题切换按钮
│       ├── icons.tsx            # 手写 lucide 风格 SVG 图标
│       └── ui/                  # shadcn/ui 风格基础组件
│           ├── button.tsx  input.tsx  label.tsx  badge.tsx
│           ├── dialog.tsx  select.tsx  table.tsx
└── src-tauri/                   # ---- 后端（Rust / Tauri）----
    ├── Cargo.toml               # Rust 依赖
    ├── build.rs                 # tauri-build（能力校验 + Windows 图标资源）
    ├── tauri.conf.json          # 窗口、托盘图标、dev/build 命令
    ├── capabilities/default.json# 权限能力（core:default）
    ├── icons/                   # 生成的应用图标（托盘与安装包使用）
    │   ├── 32x32.png  128x128.png  128x128@2x.png
    │   └── icon.png  icon.ico  icon.icns
    └── src/
        ├── main.rs              # 进程入口
        ├── lib.rs               # 组装 Builder：状态、命令、托盘、关窗隐藏
        ├── ssh_config.rs        # ~/.ssh/config 解析（含单元测试）
        ├── tunnel.rs            # 隧道生命周期：启动 800ms 检活、停止、事件广播
        ├── store.rs             # tunnels.json 持久化（原子写 + 损坏备份，含测试）
        ├── process.rs           # 跨平台进程检测/终止（tasklist·taskkill / kill）
        ├── commands.rs          # Tauri 命令层（前端接口）
        ├── tray.rs              # 系统托盘：菜单 + 左键切换窗口
        ├── logging.rs           # tracing：stdout + app.log 双写
        └── error.rs             # AppError（thiserror，序列化为中文文案）
```

## 2. 模块职责与数据流

```
React UI ──invoke──▶ commands.rs ──▶ tunnel.rs ──▶ tokio::process ssh -L
   ▲                    │              │
   │                    │              ├─▶ store.rs   tunnels.json（原子写）
   │                    │              └─▶ process.rs  存活检测 / 终止
   │                    └─▶ ssh_config.rs  Host 下拉列表
   │
   ├─ 每 3s list_tunnels 轮询（alive = PID 存活且进程名为 ssh）
   └─ listen("tunnel-exited") ◀── tunnel.rs 回收任务（ssh 退出即时通知）
```

### 关键设计

| 需求点 | 实现 |
| --- | --- |
| 启动后 800ms 检活 | `tokio::time::sleep(800ms)` 后 `child.try_wait()`；退出则解析 stderr 给出原因（端口占用 / 认证失败 / 连接被拒 / 主机名无法解析…） |
| 状态判断 | `process.rs::is_ssh_process(pid)`：Windows 用 `tasklist`，macOS/Linux 用 `libc::kill(pid,0)` + `/proc` 或 `ps`；**校验进程名是 ssh**，防止 PID 复用误判/误杀 |
| 关闭转发 | 先优雅（Windows `taskkill` / Unix `SIGTERM`）→ 最多等 1.8s → `taskkill /F` 或 `SIGKILL`；**保留记录**（PID 清零 → 「已停止」），配置长期保存可随时「启动」复用 |
| 持久化 | `tunnels.json` 原子写（临时文件 + rename）；损坏时自动备份为 `.bak`；字段与需求文档一致（snake_case） |
| 托盘 | 左键显示/隐藏主窗口；右键菜单「显示主窗口 / 退出应用（保留隧道）/ **退出应用（关闭隧道）**」；**关闭窗口只隐藏不退出**；默认退出不杀隧道，可选退出时一并关闭全部隧道 |
| 无边框窗口 | `decorations: false` + 自绘标题栏：左侧图标与标题（整条可拖拽、双击最大化），右侧 46px 方形 最小化/最大化/关闭 按钮（关闭悬停 Windows 红 `#E81123`），关闭按钮 = 隐藏到托盘 |
| 无黑窗 | Windows 下以 `CREATE_NO_WINDOW` 创建 ssh / tasklist / taskkill 子进程 |
| 日志 | `tracing` 同时写 stdout 与 `<配置目录>/app.log`，`RUST_LOG` 可覆盖级别 |

### Tauri 命令（前端接口）

| 命令 | 参数 | 返回 |
| --- | --- | --- |
| `list_hosts` | – | `SshHost[]`（alias / hostname / user / port / identity_file） |
| `list_tunnels` | – | `Tunnel[]`（记录 + `alive`） |
| `start_tunnel` | `{ request: StartTunnelRequest }` | `Tunnel` |
| `stop_tunnel` | `{ id }` | `Tunnel[]`（关闭进程、**保留记录**） |
| `restart_tunnel` | `{ id }` | `Tunnel[]`（复用已保存记录重新拉起 ssh） |
| `remove_tunnel` | `{ id }` | `Tunnel[]`（显式删除；仅允许删除已停止的记录） |
| `remove_tunnel` | `{ id }` | `Tunnel[]` |
| `ssh_config_path` / `data_path` | – | `string` |

错误统一序列化为**中文字符串**，前端直接 toast 展示。

## 3. 数据文件

- Windows：`%APPDATA%\ssh-tunnel-manager\tunnels.json`、`app.log`
- macOS：`~/Library/Application Support/ssh-tunnel-manager/tunnels.json`、`app.log`

```json
[
  {
    "id": "3f7a9c21",
    "host": "dev",
    "bind": "127.0.0.1",
    "local_port": 4096,
    "remote_host": "localhost",
    "remote_port": 4096,
    "pid": 123456,
    "created_at": "2026-09-30 17:25:11"
  }
]
```

## 4. 编译运行

### 环境要求

| 平台 | 依赖 |
| --- | --- |
| 通用 | Rust stable（1.77+）、Node.js 20+、**pnpm 10+**、系统 `ssh`（OpenSSH） |
| Windows 10/11 | Visual Studio 2022（含“使用 C++ 的桌面开发”）、WebView2 Runtime（系统自带）；OpenSSH 客户端一般已预装 |
| macOS 12+ | Xcode Command Line Tools（`xcode-select --install`） |

### 开发模式（热更新）

```bash
pnpm install          # 首次安装前端依赖
pnpm tauri dev        # 启动 Vite dev server + 编译并运行 Rust 端
```

> 也可以手动分开跑：终端 A `pnpm dev`，终端 B `cargo run --manifest-path src-tauri/Cargo.toml`
>（`tauri.conf.json` 中 `devUrl` 指向 `http://localhost:5173`）。

### 仅校验 / 测试

```bash
pnpm build                      # 前端类型检查 + 产出 dist/
cargo test  --manifest-path src-tauri/Cargo.toml   # Rust 单元测试
cargo check --manifest-path src-tauri/Cargo.toml   # Rust 编译检查
```

### 打包分发

```bash
pnpm tauri build
```

- Windows：`src-tauri/target/release/bundle/msi/*.msi` 与 `nsis/*-setup.exe`
  （首次打包会自动下载 WiX/NSIS 打包器）
- macOS：`src-tauri/target/release/bundle/dmg/*.dmg`、`*.app`
  （分发需在 `tauri.conf.json → bundle.macOS` 配置签名与公证）

### 重新生成图标

```bash
node scripts/gen-icons.mjs
```

## 5. 使用说明

1. **新建转发**：点击右上角「新建转发」→ 选择 `~/.ssh/config` 中的 Host →
   填远程主机（默认 `localhost`）、远程端口、本地端口（默认跟随远程端口）、
   绑定地址（默认 `127.0.0.1`，可改 `0.0.0.0`）→「启动转发」。
   弹窗会实时预览将执行的 `ssh -N -L …` 命令；约 1 秒后返回列表并显示**运行中**。
2. **查看状态**：表格展示 ID / Host / 本地地址 / 远程地址 / PID / 创建时间 / 状态；
   每 3 秒自动刷新，也可点「刷新」；ssh 进程中途退出会立即收到通知并转为**已停止**。
3. **关闭 / 复用 / 删除**：点「关闭」→ 终止进程但**保留记录**（已停止）；
   已停止的记录显示「**启动**」（用保存的配置重新拉起）与「**删除**」（显式移除）按钮，
   关闭多少次记录都不会丢，除非你主动删除。
4. **托盘**：关闭窗口 = 隐藏到托盘；左键托盘图标切换窗口；右键菜单可显示窗口或
   「退出应用（保留隧道）」——默认不杀隧道，下次启动会自动恢复列表与状态。

### 认证说明

默认假设你已配置**密钥免密登录**（或 ssh-agent）。应用通过
`stdin=null + stderr=pipe` 后台拉起 ssh，**不处理交互式密码输入**；
如需密码登录，请配置密钥认证，或设置 `SSH_ASKPASS` 程序。

`~/.ssh/config` 解析：支持 `Host`（一行多别名）、`HostName` / `User` / `Port` /
`IdentityFile`、`Key=Value` 写法与 `#` 注释；忽略含 `*` `?` `!` 的通配符 Host 与 `Match` 块。

## 6. 界面描述（布局）

```
┌──────────────────────────────────────────────────────────────┐
│ ▣ SSH 隧道管理器                              ─   □    ✕    │  自绘标题栏（可拖拽/双击最大化）
├──────────────────────────────────────────────────────────────┤
│ 管理到 Linux 服务器的 ssh -L 本地端口转发   [☾] [↻ 刷新]  [＋ 新建转发] │  顶栏
├──────────────────────────────────────────────────────────────┤
│ ID      Host   本地地址:端口   远程地址:端口  PID  时间  状态 │  表格
│ 3f7a9c21 dev   127.0.0.1:4096 localhost:4096  …    …   ●运行中│  （可滚动，
│ a1b2c3d4 web   0.0.0.0:8080   127.0.0.1:80  …    …   ●已停止│   空状态/骨架屏）
│                              [关闭] / [启动][删除]           │
├──────────────────────────────────────────────────────────────┤
│ 共 2 条隧道 · ●运行中 1 · ●已停止 1        每3秒刷新 · 托盘  │  状态栏
└──────────────────────────────────────────────────────────────┘
```

- 深/浅主题：右上角月亮/太阳按钮切换，`<html class="dark">` + localStorage 持久化，
  首帧前由内联脚本应用，无闪白。
- 右键：全局禁用界面右键菜单（`contextmenu` preventDefault，WebView2 默认菜单不再弹出）；
  托盘图标的原生右键菜单保留。
- 新建弹窗：表单式布局 + 命令实时预览 + 字段级中文校验提示；端口范围 1-65535。
- 反馈：操作结果通过右上角 Toast（sonner）提示，失败信息为后端原始中文错误。

## 7. 已验证（Windows 11 实机，2026-09-30）

### 构建与测试

| 项目 | 结果 |
| --- | --- |
| `cargo check` / `cargo build`（MSVC，433 个依赖） | ✅ 0 error / 0 warning |
| `cargo test`（ssh_config 解析 ×2、store JSON 序列化 ×1） | ✅ 3 passed |
| `pnpm build`（`tsc -b` 严格模式 + Vite 产物 398 kB） | ✅ exit 0 |

### 功能 E2E（真实 UI 操作 + 文件/进程断言）

| 场景 | 结果与证据 |
| --- | --- |
| 启动加载 | 日志 `已加载 N 条隧道记录`、`系统托盘已创建`；`tunnels.json` 正确读入 ✅ |
| 状态检测（运行中/已停止/死 PID） | 3 条记录状态全部正确，PID 划线、按钮按状态切换（运行中→关闭，已停止→清理）✅ |
| 进程外部退出自动翻转 | 演示 ssh 进程定时到期死亡 → 轮询 3s 内 UI 自动转为「已停止」（记录保留）✅ |
| 关闭转发 E2E | 点击「关闭」→ **524ms** 内进程被终止（优雅→强制）→ 记录 3→2 → 绿色 Toast「已关闭转发 staging-web」，`tunnels.json` 同步 ✅ |
| 清理记录 | 两条已停止记录逐个清理 → `tunnels.json` = `[]`，界面进入空状态 ✅ |
| 新建转发弹窗 | UIA 点击打开 → 命令实时预览、端口占位符/校验、无 Host 时的琥珀色配置提示均正常 ✅ |
| 深/浅主题 | 切换生效且按钮图标联动（日/月），localStorage 持久化 ✅ |
| 关闭窗口 → 托盘 | 点标题栏 ✕ 后窗口隐藏、进程仍存活（日志无退出）✅ |
| **托盘图标点击切换** | 在 Windows 溢出浮层中**物理点击**托盘图标：窗口隐藏 → 再点 → 窗口恢复，**E2E PASS** ✅ |
| 3 秒轮询 | 多次采样期间状态/计数保持一致 ✅ |

### 第二轮调整（同日）验证

| 调整项 | 结果 |
| --- | --- |
| 取消窗口置顶 | `WS_EX_TOPMOST` 已清除（`GetWindowLong` 实测 bit 0x8 = CLEARED）✅ |
| 无边框 + 自绘标题栏 | `decorations: false`；标题栏可拖拽（模拟拖动 dx=120/dy=80 精确跟随）、双击最大化、最小化/最大化/还原按钮全部 E2E PASS（依赖 capabilities 新增的 window 权限）✅ |
| 绑定地址新增 localhost | 下拉 4 项齐全：127.0.0.1 / **localhost（等价 127.0.0.1）** / 0.0.0.0 / ::1 ✅ |
| 托盘菜单新增「退出应用（关闭隧道）」 | 菜单项真实点击执行：app.log 记录 `从托盘退出应用（关闭所有隧道）` → `退出前关闭了 N 条运行中的隧道` → 进程正常退出 ✅ |
| 项目迁移 `D:\Codespace\SSHTunnel` | robocopy 迁移（67 文件 0 失败）→ 原地 `pnpm install/build + cargo build` 全绿 → 实机运行验证 → 旧目录已删除 ✅ |

### 第三轮调整（同日）

| 调整项 | 内容 | 状态 |
| --- | --- | --- |
| 关闭后保留记录 | `stop_tunnel` 不再删除记录（PID 清零→已停止）；新增 `restart_tunnel` 复用记录重新拉起；`remove_tunnel` 仅允许删除已停止记录 | `cargo check` / `pnpm build` 通过，交互由用户实测 |
| 标题去重 | 自绘标题栏保留窗口标题；内容区去掉重复 logo+大标题，仅留说明文字与操作按钮 | 同上 |

### 已知边界 / 待真机补充

- **启动成功路径**需要真实可达的 SSH 主机与密钥：功能验证阶段本机尚未配置
  `~/.ssh/config`，故对 `start_tunnel` 覆盖的是其失败分支（800ms 检活 + stderr 归因），
  `stop_tunnel` / `restart_tunnel` 覆盖完整成功分支；后续已接入真实配置（5 个 Host 的
  下拉解析正常），业务主机连通性由日常使用验证。
- macOS 侧代码路径（`libc::kill`、`ps -o comm`、ICNS 图标）已按平台条件编译实现，
  需在 macOS 12+ 上执行 `pnpm tauri dev / build` 做最终回归。
- 打包分发已在本机执行：`pnpm tauri build` 产出
  `SSH Tunnel Manager_0.1.0_x64_en-US.msi`（2.1 MB）与
  `SSH Tunnel Manager_0.1.0_x64-setup.exe`（1.5 MB）；
  macOS（dmg/app）需在 macOS 机器上执行同样的 `pnpm tauri build`。
