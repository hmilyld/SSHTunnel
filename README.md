# SSH 隧道管理器（SSH Tunnel Manager）

运行在 **Windows 10/11** 与 **macOS 12+** 的桌面应用，用于图形化管理到 Linux 服务器的
SSH 本地端口转发（`ssh -L`）：新建转发、实时查看状态、一键关闭，并支持最小化到系统托盘。

- GUI 框架：**Tauri 2.0**（Rust 后端 + Web 前端）
- 前端：**React 19 + TypeScript + Tailwind CSS 4 + shadcn/ui 风格组件**（Vite 构建）
- SSH：调用**系统 `ssh` 命令**（`ssh -N -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 -L …`）
- 持久化：JSON 文件（`dirs` crate 解析的应用配置目录）
- 托盘：Tauri 内置 Tray API（左键切换窗口，右键菜单：显示主窗口 / 退出）

> **版权所有 © 2026 [hmilyld.com](https://hmilyld.com)** ·
> 仓库：[github.com/hmilyld/SSHTunnel](https://github.com/hmilyld/SSHTunnel)（MIT License）

---

## 1. 项目结构

```
SSHTunnel/
├── package.json                 # 前端依赖与脚本（pnpm）
├── pnpm-workspace.yaml          # pnpm 11+ 构建脚本放行（allowBuilds: esbuild）
├── vite.config.ts               # Vite + React + Tailwind 插件、@ 别名
├── tsconfig*.json               # TS 严格模式（app / node 两份）
├── index.html                   # 前端入口（首帧前应用主题，避免闪白）
├── .github/workflows/
│   └── release.yml              # 打 vX.Y.Z tag 时自动编译各平台安装包并发布 Release
├── scripts/
│   ├── gen-icons.mjs            # 零依赖图标生成（PNG / ICO / ICNS；.icns 用 macOS 824-on-1024 栅格，另出单色托盘模板图）
│   ├── verify-icons.mjs         # 图标校验：解码 PNG/ICNS/ICO，报告颜色与不透明区边距
│   ├── screenshot.ps1           # 全屏截图（验证用）
│   └── uiclick.ps1              # UI Automation 点击（验证用）
├── src/                         # ---- 前端（React）----
│   ├── main.tsx                 # 前端入口
│   ├── App.tsx                  # 页面主体：轮询、事件、增删改查
│   ├── styles/globals.css       # 设计令牌（深/浅主题 CSS 变量）+ 全局样式
│   ├── lib/
│   │   ├── api.ts               # tauri command 的类型化封装
│   │   ├── platform.ts          # 是否 macOS（决定用原生标题栏还是自绘标题栏）
│   │   └── utils.ts             # cn() 类名合并
│   ├── hooks/useTheme.ts        # 深色/浅色主题（localStorage 持久化）
│   └── components/
│       ├── TitleBar.tsx         # 自绘标题栏（**仅 Windows**：拖拽移动 / 双击最大化 / 最小化·最大化·关闭）
│       ├── AppHeader.tsx        # 内容区顶栏：说明文字 / 刷新 / 新建转发（标题不重复）
│       ├── TunnelTable.tsx      # 隧道列表表格（含骨架屏、空状态）
│       ├── NewTunnelDialog.tsx  # 新建转发弹窗（表单校验 + 命令预览）
│       ├── PasswordDialog.tsx   # 密码认证弹窗（SSH_ASKPASS 注入，不落盘）
│       ├── StatusBar.tsx        # 底部状态栏（计数、路径提示、版权 © hmilyld.com）
│       ├── ThemeToggle.tsx      # 主题切换按钮
│       ├── icons.tsx            # 手写 lucide 风格 SVG 图标
│       └── ui/                  # shadcn/ui 风格基础组件
│           ├── button.tsx  input.tsx  label.tsx  badge.tsx
│           ├── dialog.tsx  select.tsx  table.tsx
└── src-tauri/                   # ---- 后端（Rust / Tauri）----
    ├── Cargo.toml               # Rust 依赖
    ├── build.rs                 # tauri-build（能力校验 + Windows 图标资源）
    ├── tauri.conf.json          # 窗口、托盘图标、dev/build 命令
    ├── tauri.windows.conf.json  # Windows 覆盖：decorations:false（无边框 + 自绘标题栏）
    ├── capabilities/default.json# 权限能力（core:default）
    ├── icons/                   # 生成的应用图标（托盘与安装包使用）
    │   ├── 32x32.png  128x128.png  128x128@2x.png
    │   └── icon.png  icon.ico  icon.icns
    └── src/
        ├── main.rs              # 进程入口
        ├── lib.rs               # 组装 Builder：状态、命令、托盘、关窗隐藏
        ├── ssh_config.rs        # ~/.ssh/config 解析（含单元测试）
        ├── tunnel.rs            # 隧道生命周期：启动判定（本地端口监听探测）、停止、事件广播
        ├── store.rs             # tunnels.json 持久化（原子写 + 损坏备份，含测试）
        ├── process.rs           # 跨平台进程检测/终止（tasklist·taskkill / kill）
        ├── commands.rs          # Tauri 命令层（前端接口）
        ├── tray.rs              # 系统托盘：菜单 + 左键切换窗口（macOS 用单色模板图标）
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
| 启动成功判定 | **成功 = 本地 `bind:port` 真正进入监听**（每 150ms TCP 探测；ssh 认证通过后才会绑定 `-L` 监听），监听一出现立即返回，慢服务器不谎报成功；**失败 = 进程退出**（即时解析 stderr 归因：端口占用 / 密码错误 / 认证失败 / 连接被拒 / 主机名无法解析…）**或 18s 无监听**（杀进程报超时；ssh 自身 `ConnectTimeout=15` 通常先退出给出真实原因）。启动前还会预检本地端口是否已被**其他进程**占用（立即明确报错） |
| 端口占用报错 | 报错中附带**占用进程名与 PID**（Windows 用 `Get-NetTCPConnection`，Unix 用 `lsof`/`ss`，仅错误路径触发）；占用者是 Node 且端口正好是 **5173（Vite 开发服务器默认端口）**时额外提示，疑似 ssh 残留（进程名以 `ssh` 开头）时也会提示——**只查本地监听，与远程端口是否存在无关** |
| 状态判断 | `process.rs::is_ssh_process(pid)`：Windows 用 `tasklist`，macOS/Linux 用 `libc::kill(pid,0)` + `/proc` 或 `ps`；**校验进程名是 ssh**，防止 PID 复用误判/误杀 |
| 关闭转发 | 先优雅（Windows `taskkill` / Unix `SIGTERM`）→ 最多等 1.8s → `taskkill /F` 或 `SIGKILL`；**保留记录**（PID 清零 → 「已停止」），配置长期保存可随时「启动」复用 |
| 持久化 | `tunnels.json` 原子写（临时文件 + rename）；损坏时自动备份为 `.bak`；字段与需求文档一致（snake_case） |
| 托盘 | 左键显示/隐藏主窗口；右键菜单「显示主窗口 / 退出应用（保留隧道）/ **退出应用（关闭隧道）**」；**关闭窗口只隐藏不退出**；默认退出不杀隧道，可选退出时一并关闭全部隧道 |
| 托盘图标（按平台） | **macOS 菜单栏按 Apple 约定用单色模板图**：单独生成 `icons/tray-icon.png`（黑 + alpha，箭头镂空）并用 `icon_as_template(true)` 内嵌，系统自动在浅色菜单栏画黑、深色画白——**菜单栏不放彩色图标**；Windows 通知区域仍用彩色应用图标（`default_window_icon`） |
| 标题栏（按平台） | **macOS/Linux 用系统原生标题栏**（`tauri.conf.json` → `decorations: true`，红绿灯由系统绘制）；**Windows 才是无边框窗口**（`tauri.windows.conf.json` 覆盖为 `decorations: false`）+ 自绘标题栏：左侧图标与标题（整条可拖拽、双击最大化），右侧 46px 方形 最小化/最大化/关闭 按钮（关闭悬停 Windows 红 `#E81123`），关闭按钮 = 隐藏到托盘。前端据此决定是否渲染 `TitleBar`（`src/lib/platform.ts`）——**不会出现两条标题栏** |
| 无黑窗 | Windows 下以 `CREATE_NO_WINDOW` 创建 ssh / tasklist / taskkill 子进程 |
| 应用图标（macOS 26） | `.icns` 按 Apple 的 **824-on-1024 图标栅格**渲染：圆角方块 824×824 居中放在 1024 画布上、四周 100px 透明边距、圆角半径 185.4（`scripts/gen-icons.mjs`）；Windows/Linux 的 PNG/ICO 仍满画布。macOS 26 (Tahoe) 会把不符合该栅格的图标缩小并套进灰色圆角底框（社区叫 "icon jail"，观感就是灰白方块），且满画布图标在 Dock 里一向比系统图标更大 |
| 日志 | `tracing` 同时写 stdout 与 `<配置目录>/app.log`，`RUST_LOG` 可覆盖级别 |

> 维护提醒：平台配置按 **JSON Merge Patch（RFC 7396）** 合并，**数组是整体替换、不会逐项合并**。
> 所以 `tauri.windows.conf.json` 的 `app.windows` 必须完整复制 `tauri.conf.json` 里的窗口对象
> （目前只有 `decorations` 不同）——改窗口尺寸/标题等字段时**两个文件都要改**，否则 Windows 上
> 会静默用回平台文件里的旧值。

### Tauri 命令（前端接口）

| 命令 | 参数 | 返回 |
| --- | --- | --- |
| `list_hosts` | – | `SshHost[]`（alias / hostname / user / port / identity_file） |
| `list_tunnels` | – | `Tunnel[]`（记录 + `alive`） |
| `start_tunnel` | `{ request: StartTunnelRequest, password?: string }` | `Tunnel`（需要密码时返回 `PASSWORD_REQUIRED::…`） |
| `stop_tunnel` | `{ id }` | `Tunnel[]`（关闭进程、**保留记录**） |
| `restart_tunnel` | `{ id, password?: string }` | `Tunnel[]`（复用已保存记录重新拉起 ssh） |
| `update_tunnel` | `{ id, request }` | `Tunnel[]`（修改已停止记录的配置） |
| `remove_tunnel` | `{ id }` | `Tunnel[]`（显式删除；仅允许删除已停止的记录） |
| `ssh_config_path` / `data_dir` | – | `string`（`data_dir` = tunnels.json / app.log 所在目录） |
| `reveal_data_dir` | – | `()`（在资源管理器 / 访达中打开数据目录；路径由后端解析，不接受前端传参） |

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

### 自动发版（GitHub Actions）

`.github/workflows/release.yml`：推送形如 `vX.Y.Z` 的 tag 时自动编译安装包，
并以同名 tag 创建 GitHub Release、上传安装包。

```bash
# 1. 先把版本号改好（工作流会校验 tag 与 package.json / tauri.conf.json 一致）
#    package.json、src-tauri/tauri.conf.json、src-tauri/Cargo.toml 的 version
# 2. 提交后打 tag 并推送
git tag v0.3.0 && git push origin v0.3.0
```

- 触发条件：tag 匹配 `v*`；tag 必须是 `vX.Y.Z`，且与 `package.json`、
  `src-tauri/tauri.conf.json` 的 `version` 完全一致，否则任务在校验步骤直接失败
  （避免发出版本号与包内版本对不上的安装包）。
- **CI 与本地刻意使用不同的包管理器**：
  - CI（GitHub Actions）：**npm** —— `npm ci --no-audit --no-fund`，依赖**提交的
    `package-lock.json`**（lockfileVersion 3，220 个包）；CI 上不安装 pnpm。
  - 本地开发：**pnpm** —— 依赖 `pnpm-lock.yaml`，延续仓库原有工作流
    （`pnpm install` / `pnpm tauri dev` / `pnpm tauri build`）。
  - **改依赖时要两个都更新**：`pnpm add <pkg>` 之后再跑一次
    `npm install --package-lock-only`（若本机 node_modules 是 pnpm 结构，直接在工作区跑
    npm 可能报错，可先把 `package.json` 复制到空目录生成 lockfile 再拷回）。
    只改一个的后果：CI 的 `npm ci` 会因 lockfile 与 package.json 不匹配而失败
    ——这是有意的失败快停，而不是静默装上不同版本的依赖。
  - 两者解析出的版本一致（同一份 semver 范围），差异只在 node_modules 布局
    与依赖提升方式，不影响构建产物。
  - `tauri.conf.json` 的 `beforeBuildCommand` 用 **`npm run build`**（而不是 `pnpm build`）：
    它必须在只有 npm 的 CI 里也能跑通。本地即使依赖是 pnpm 装的也能正常执行
    ——`npm run build` 读的是同一份 `package.json` scripts；`beforeDevCommand` 仍是
    `pnpm dev`，本地开发流程不变。
- 产物矩阵：
  - `windows-latest` → `x86_64-pc-windows-msvc`：NSIS `*-setup.exe` 与 WiX `.msi`
  - `macos-latest` → `aarch64-apple-darwin`：`.dmg`（未做签名/公证）
- Release 正文包含下载指引，并自动追加 GitHub 生成的 Release Notes；
  同时把安装包作为 workflow artifact 归档一份，便于排查。
- 权限：仅需仓库默认 `GITHUB_TOKEN`（工作流显式声明 `permissions: contents: write`），
  无需配置任何 secret。
- 未配置签名密钥，因此不生成 Tauri updater 清单（`uploadUpdaterJson: false`）。

## 5. 使用说明

1. **新建转发**：点击右上角「新建转发」→ 选择 `~/.ssh/config` 中的 Host →
   填远程主机（默认 `localhost`）、远程端口、本地端口（默认跟随远程端口）、
   绑定地址（远程主机为 `localhost` / `127.0.0.1` 时按联动规则取同名本机地址，
   也可手动改为 `0.0.0.0`）→「启动转发」。
   弹窗会实时预览将执行的 `ssh -N -L …` 命令；**等本地端口真正进入监听**才返回列表
   并显示**运行中**（慢服务器不会提前报成功，失败会给出具体原因）。
2. **查看状态**：表格展示 ID / Host / 本地地址 / 远程地址 / PID / 创建时间 / 状态；
   每 3 秒自动刷新，也可点「刷新」；ssh 进程中途退出会立即收到通知并转为**已停止**。
3. **关闭 / 修改 / 复用 / 删除**：点「关闭」→ 终止进程但**保留记录**（已停止）；
   已停止的记录显示三个操作：
   - 「**启动**」——用保存的配置重新拉起 ssh；
   - 「**修改**」——编辑 Host/绑定/端口等配置（仅保存，不涉及进程；ID 与创建时间不变，
     运行中的记录不可修改、新端口与其他运行中记录冲突会被拦截）；
   - 「**删除**」——显式移除记录。
   关闭多少次记录都不会丢，除非你主动删除。
4. **托盘**：关闭窗口 = 隐藏到托盘；左键托盘图标切换窗口；右键菜单可显示窗口或
   「退出应用（保留隧道）」——默认不杀隧道，下次启动会自动恢复列表与状态。

### 认证说明（密钥优先，密码自动探测）

1. **密钥认证**（推荐）：已配置密钥或 ssh-agent 时直接启动，无额外步骤。
   另外新增 `-o StrictHostKeyChecking=accept-new`，首次连接新主机不再因
   yes/no 询问在无终端环境下失败（已变更的主机密钥仍会硬失败）。
2. **密码认证**：
   - 首次启动带 `-o BatchMode=yes` 快速探测（禁止交互、不会挂起）；
     若 stderr 返回 `Permission denied (publickey,password)`（**方法列表里含
     password** 才判定，仅允许公钥的服务器不会弹框），后端返回
     `PASSWORD_REQUIRED::…`，前端弹出**密码输入框**。
   - **确认后先做认证预检**：单独执行一次 `ssh -T`（不建立转发，限定
     `PreferredAuthentications=password,keyboard-interactive`、
     `NumberOfPasswordPrompts=1`）验证密码——密码错误在预检阶段就红字提示，
     **不会出现“提示成功后立刻失败”**；预检通过才建立真正的隧道
     （最终成功判定以本地端口是否进入监听为准，慢服务器不会误报成功）。
   - 预检超时（>5s）或连接类错误时 fail-open，交由隧道启动给出真实错误。
   - 密码通过 `SSH_ASKPASS=<本程序> + SSH_ASKPASS_REQUIRE=force +
     STM_ASKPASS_PWD=<密码>` 注入 ssh 子进程环境；
     程序以辅助模式（带参数且检测到 `SSH_ASKPASS` 环境变量）被 ssh 拉起，
     仅向 stdout 输出密码后立即退出，不启动 GUI、不写日志。
   - 密码**只驻留内存与子进程环境，不写入 tunnels.json、不进日志**；
     Windows 下 askpass 路径优先取 8.3 短路径，兼容 `Program Files` 等带空格目录。
   - 「启动已保存的转发」走同一条密码流程。
3. 新建弹窗与命令预览中均可看到将执行的完整 `ssh -N -L …` 命令。

`~/.ssh/config` 解析：支持 `Host`（一行多别名）、`HostName` / `User` / `Port` /
`IdentityFile`、`Key=Value` 写法与 `#` 注释；忽略含 `*` `?` `!` 的通配符 Host 与 `Match` 块。

## 6. 界面描述（布局）

```
┌──────────────────────────────────────────────────────────────┐
│ ▣ SSH 隧道管理器                              ─   □    ✕    │  Windows 自绘标题栏（可拖拽/双击最大化）
├──────────────────────────────────────────────────────────────┤
│ 管理到 Linux 服务器的 ssh -L 本地端口转发   [☾] [↻ 刷新]  [＋ 新建转发] │  顶栏
├──────────────────────────────────────────────────────────────┤
│ ID      Host   本地地址:端口   远程地址:端口  PID  时间  状态 │  表格
│ 3f7a9c21 dev   127.0.0.1:4096 localhost:4096  …    …   ●运行中│  （可滚动，
│ a1b2c3d4 web   0.0.0.0:8080   127.0.0.1:80  …    …   ●已停止│   空状态/骨架屏）
│                              [关闭] / [启动][修改][删除]      │
├──────────────────────────────────────────────────────────────┤
│ 共 2 条隧道 · ●运行中 1 · ●已停止 1        v0.2.0 · © 2026 hmilyld.com · [📁] │  底部信息栏
└──────────────────────────────────────────────────────────────┘
```

- 说明：上图是 **Windows** 的界面（无边框 + 自绘标题栏）；macOS 用系统原生标题栏，
  窗口左上角是系统红绿灯，`TitleBar` 不渲染。
- 深/浅主题：右上角月亮/太阳按钮切换，`<html class="dark">` + localStorage 持久化，
  首帧前由内联脚本应用，无闪白。
- 右键：全局禁用界面右键菜单（`contextmenu` preventDefault，WebView2 默认菜单不再弹出）；
  托盘图标的原生右键菜单保留。
- 新建弹窗：表单式布局 + 命令实时预览 + 字段级中文校验提示；端口范围 1-65535。
- 反馈：操作结果通过右上角 Toast（sonner）提示，失败信息为后端原始中文错误。
- **底部信息栏**（左：状态统计；右：版本 / 版权 / 数据目录入口）：
  - 版本号 `vX.Y.Z` 取自 `tauri.conf.json`（Tauri `getVersion()`，无需额外命令），
    与安装包版本天然一致；取不到时该段隐藏，不显示占位符。
  - 版权 `© 2026 hmilyld.com`（纯文本，不做外链跳转——不为此引入 opener 权限）。
  - 数据目录入口是**按钮**而非整行路径：点击在资源管理器 / 访达中打开数据目录，
    悬停提示里给出完整路径。长路径不再铺在界面上（既占位又不可点）。
    安全边界：命令只打开**后端自己解析**的数据目录，不接受前端传入路径，
    因此即使 WebView 侧被注入也无法借它打开任意路径。
  - 已移除「每 3 秒自动刷新」「关闭窗口将最小化到托盘」两条静态说明：
    前者对使用无帮助，后者在标题栏关闭按钮的 tooltip 里已有说明。

## 7. 已验证（Windows 11 实机，2026-09-30）

### 构建与测试

| 项目 | 结果 |
| --- | --- |
| `cargo check --all-targets` | ✅ 0 error / 0 warning |
| `cargo build`（MSVC，433 个依赖） | ✅ 0 error（仅 MSVC 链接器输出一条 `linker stdout` 提示，非代码告警） |
| `cargo test`（ssh_config ×2、store ×1、process ×1、tunnel ×5） | ✅ 9 passed |
| `pnpm build`（`tsc -b` 严格模式 + Vite 产物 421.4 kB，gzip 132.7 kB） | ✅ exit 0 |
### 功能 E2E（真实 UI 操作 + 文件/进程断言）

| 场景 | 结果与证据 |
| --- | --- |
| 启动加载 | 日志 `已加载 N 条隧道记录`、`系统托盘已创建`；`tunnels.json` 正确读入 ✅ |
| 状态检测（运行中/已停止/死 PID） | 3 条记录状态全部正确，PID 划线、按钮按状态切换（运行中→关闭，已停止→启动/修改/删除）✅ |
| 进程外部退出自动翻转 | 演示 ssh 进程定时到期死亡 → 轮询 3s 内 UI 自动转为「已停止」（记录保留）✅ |
| 关闭转发 E2E | 点击「关闭」→ **524ms** 内进程被终止（优雅→强制）→ 记录 3→2 → 绿色 Toast「已关闭转发 staging-web」，`tunnels.json` 同步 ✅ |
| 删除记录 | 两条已停止记录逐个删除 → `tunnels.json` = `[]`，界面进入空状态 ✅ |
| 新建转发弹窗 | UIA 点击打开 → 命令实时预览、端口占位符/校验、无 Host 时的琥珀色配置提示均正常 ✅ |
| 深/浅主题 | 切换生效且按钮图标联动（日/月），localStorage 持久化 ✅ |
| 关闭窗口 → 托盘 | 点标题栏 ✕ 后窗口隐藏、进程仍存活（日志无退出）✅ |
| **托盘图标点击切换** | 在 Windows 溢出浮层中**物理点击**托盘图标：窗口隐藏 → 再点 → 窗口恢复，**E2E PASS** ✅ |
| 3 秒轮询 | 多次采样期间状态/计数保持一致 ✅ |

### 第二轮调整（同日）验证

| 调整项 | 结果 |
| --- | --- |
| 取消窗口置顶 | `WS_EX_TOPMOST` 已清除（`GetWindowLong` 实测 bit 0x8 = CLEARED）✅ |
| 无边框 + 自绘标题栏（Windows） | `tauri.windows.conf.json` → `decorations: false`；标题栏可拖拽（模拟拖动 dx=120/dy=80 精确跟随）、双击最大化、最小化/最大化/还原按钮全部 E2E PASS（依赖 capabilities 新增的 window 权限）✅ |
| 绑定地址新增 localhost | 下拉 4 项齐全：127.0.0.1 / **localhost（等价 127.0.0.1）** / 0.0.0.0 / ::1 ✅ |
| 托盘菜单新增「退出应用（关闭隧道）」 | 菜单项真实点击执行：app.log 记录 `从托盘退出应用（关闭所有隧道）` → `退出前关闭了 N 条运行中的隧道` → 进程正常退出 ✅ |
| 项目迁移 `D:\Codespace\SSHTunnel` | robocopy 迁移（67 文件 0 失败）→ 原地 `pnpm install/build + cargo build` 全绿 → 实机运行验证 → 旧目录已删除 ✅ |

### 第三轮调整（同日）

| 调整项 | 内容 | 状态 |
| --- | --- | --- |
| 关闭后保留记录 | `stop_tunnel` 不再删除记录（PID 清零→已停止）；新增 `restart_tunnel` 复用记录重新拉起；`remove_tunnel` 仅允许删除已停止记录 | `cargo check` / `pnpm build` 通过，交互由用户实测 |
| 标题去重 | 自绘标题栏保留窗口标题；内容区去掉重复 logo+大标题，仅留说明文字与操作按钮 | 同上 |
| 标题栏按平台拆分 | macOS 改用**系统原生标题栏**（红绿灯）：基础 `tauri.conf.json` → `decorations: true`；无边框 + 自绘标题栏只留给 Windows（`tauri.windows.conf.json` 覆盖为 `decorations: false`），前端由 `src/lib/platform.ts::usesNativeTitleBar()` 决定是否渲染 `TitleBar`，避免出现两条标题栏 | `cargo check` / `cargo test`（9 passed）/ `pnpm build` 通过；macOS 侧待真机确认（见第 9 节） |
| macOS 图标改为 824-on-1024 栅格 | 用户反馈 macOS Dock 里图标是「黑白方块」：图标文件本身验证为正常彩色（发布包 `Resources/icon.icns` 与仓库哈希一致），根因是 macOS 26 (Tahoe) 对不符合 Apple 图标栅格的图标做「缩小 + 灰色底框」处理。改为 `.icns` 按 824×824（r=185.4）居中放进 1024 画布渲染，PNG/ICO 不变 | `node scripts/verify-icons.mjs`：ic10 = 1024px、不透明区 824×824、左边距 9.77%、彩色占比 90.6%（= 栅格精确命中）✅ |
| macOS 菜单栏图标改为单色模板图 | 用户指正：macOS **菜单栏（状态栏）图标按 Apple 约定应为黑白**，不能直接用彩色应用图标。新增 `icons/tray-icon.png`（36×36 单色：黑 + alpha、箭头镂空），`tray.rs` 在 macOS 用它并置 `icon_as_template(true)`（浅色菜单栏黑 / 深色白，由系统渲染）；Windows 托盘仍用彩色应用图标 | `verify-icons.mjs`：tray-icon.png 平均色差 0.0、彩色占比 0.0%、RGB 全 0（单色）；alpha 扫描：不透明 1104 / 透明 148，36×36、水平中线为 `###.......##`（箭头处确实镂空）✅；`cargo check` / `cargo test`（9 passed）/ `pnpm build` 通过 |

## 8. 代码复查与清理（2026-09-30）

本轮对全部源码（Rust 10 个模块 + 前端 22 个文件 + 配置）做了简洁性 / 规范性 / 安全性 /
逻辑复查，改动如下（均已通过 `cargo check --all-targets` + `cargo test`（9 passed）+
`pnpm build` 重新验证）：

### 新增功能（同日）

| 功能 | 实现 |
| --- | --- |
| **版权显示** | 底部信息栏常驻 `© 2026 hmilyld.com`；元数据同步到 `package.json`（`author`/`homepage`/`repository`/`license`）与 `src-tauri/Cargo.toml`（`authors`/`homepage`/`repository`）；README 顶部加版权行 |
| **打 tag 自动发版** | 新增 `.github/workflows/release.yml`：推送 `vX.Y.Z` tag → 校验 tag 与 `package.json`/`tauri.conf.json` 版本一致 → 矩阵编译 `windows-latest`（NSIS `*-setup.exe` + WiX `.msi`）与 `macos-latest`（`.dmg`）→ 以 tag 创建 Release 并上传安装包；另归档一份 workflow artifact。**CI 依赖 npm（`package-lock.json`），本地仍是 pnpm** |
| **底部信息栏重设计** | 去掉整行数据路径（改为「打开数据目录」按钮 + 悬停提示完整路径）；新增版本号 `vX.Y.Z`（`getVersion()`）；移除两条无信息量的静态说明。新增 `data_dir` / `reveal_data_dir` 命令（后者只打开后端解析的目录，不接受前端传参），并清理 `capabilities/default.json` 中 4 条未使用的窗口权限 |

### 逻辑修复

| 问题 | 位置 | 修复 |
| --- | --- | --- |
| **全新安装首次启动不写 `app.log`**：日志初始化早于 `tunnels.json` 首次保存，而数据目录此前只由 `Store::save` 创建 | `logging.rs` | 初始化时先 `create_dir_all` 数据目录 |
| **保存失败会留下“无人管理”的 ssh 进程**：记录先入内存、`save()` 失败即返回错误，进程仍在跑且不在列表里 | `tunnel.rs::start_tunnel` / `restart_tunnel` | `save()` 失败时回滚内存记录（PID 归零）并 `kill + wait` 回收刚拉起的 ssh |
| **超时/预检失败的 ssh 子进程未回收**：`kill_on_drop(false)` 下只 `start_kill()` 不 `wait()`，会留下僵尸句柄 | `tunnel.rs` | 新增 `kill_unstarted()`，统一 `start_kill + wait`，三处失败路径复用 |
| **8 位短 ID 未排重**：ID 是记录唯一键，碰撞会让记录无法单独操作、ssh 进程杀不掉 | `tunnel.rs` | 新增 `unique_id()`，生成时对现有记录排重并告警 |
| 端口冲突判定在三处重复实现 | `tunnel.rs` | 抽出 `local_port_taken(store, exclude_id, bind, port)`，语义与 `list_views` 的存活判定保持一致 |

### 清理（删除无用代码 / 修正与代码不符的说明）

- 删除未被任何地方使用的图标 `ArrowRightIcon`、`ActivityIcon`、`FileIcon`
  （`FileIcon` 随状态栏路径行一起下线）。
- 删除随路径行一起失去调用方的 `data_path` 命令与 `api.dataPath()` 包装；
  新增的 `data_dir` 只用于按钮的悬停提示。
- `capabilities/default.json` 去掉未被任何前端调用使用的
  `core:window:allow-maximize` / `allow-unmaximize` / `allow-show` / `allow-set-focus`
  （`toggleMaximize` 自带最大化与还原，不需要单独权限）。
- `useTheme` 不再对外返回无调用方的 `setTheme`（内部保留为 `applyTheme`）。
- 修正与代码不符的注释：`tunnel.rs` 模块头仍写「800ms 检活」、`error.rs::StartFailed`
  仍写「800ms 内退出」——实际实现是「每 150ms 探测本地端口监听，18s 超时」。
- `README.md`：测试数量 3 → 9、命令表删除重复的 `remove_tunnel` 行、
  新建转发的绑定地址默认值改为与 `NewTunnelDialog` 联动规则一致、
  「刷新」中的「清理」（按钮实际已更名「删除」）等表述对齐。
- 保留 `src/components/ui/` 下 shadcn 风格基础组件中暂未使用的 variant / 子组件
  （`Button` 的 `destructive`/`link`、`Dialog` 的 `DialogTrigger`/`DialogClose`、
  `Select` 的 `SelectGroup`/`Label`/`Separator`、`Badge` 的 `danger`/`warning` 等）：
  它们是组件库的约定 API，删除会让后续新增界面必须改基础组件，属于脚手架而非死代码。

### 安全性结论（未发现漏洞，供后续维护参考）

- 密码只以子进程环境变量（`SSH_ASKPASS` + `STM_ASKPASS_PWD`）传递，不落盘、不进日志；
  `askpass` 辅助模式只写 stdout。已确认全部 `tracing` 调用点都不含密码。
- `main.rs` 的辅助模式需要「带参数 + 有 `SSH_ASKPASS` 环境变量」两个条件同时成立，
  正常 GUI 启动（无参数）不会误打印密码。
- 所有外部命令（`ssh` / `tasklist` / `taskkill` / `powershell` / `lsof` / `ss`）都通过
  参数数组传参，端口是 `u16`，不存在命令注入路径；终止进程前强制校验进程名是 `ssh`，
  避免 PID 复用导致误杀。
- 已知项：`tauri.conf.json` 的 `csp` 为 `null`（未设置 CSP）。因为不加载任何远程内容、
  未开启 `withGlobalTauri`、能力集只放开了自绘标题栏所需的窗口权限，风险很低；
  若要加固可在此补一条 `default-src 'self'` 级别的策略。

## 9. 已知边界 / 待真机补充

- **启动成功路径**需要真实可达的 SSH 主机与密钥：功能验证阶段本机尚未配置
  `~/.ssh/config`，故对 `start_tunnel` 覆盖的是其失败分支（端口占用预检、
  18s 监听超时与 stderr 归因），
  `stop_tunnel` / `restart_tunnel` 覆盖完整成功分支；后续已接入真实配置（5 个 Host 的
  下拉解析正常），业务主机连通性由日常使用验证。
- macOS 侧代码路径（`libc::kill`、`ps -o comm`、ICNS 图标）已按平台条件编译实现，
  需在 macOS 12+ 上执行 `pnpm tauri dev / build` 做最终回归。
- **标题栏按平台拆分**（macOS 用原生红绿灯、只有 Windows 自绘）只在本机验证了 Windows 侧
  （`cargo check` / `cargo test` / `pnpm build` 全绿 + 配置合并结果比对），
  macOS 上「原生标题栏且不渲染 `TitleBar`」需在 Mac 上跑一次 `pnpm tauri dev` 确认。
- **macOS 26 (Tahoe) 图标**：本轮把 `.icns` 改成 Apple 的 824-on-1024 栅格（不再满画布），
  以退出「缩小 + 灰色底框」的 icon jail；但**真正拿到 Liquid Glass 外观还需要 `.icon`
  bundle 编译出的 `Assets.car`**——`actool` 只在 macOS + Xcode 26 上有，本机（Windows）无法
  生成，也无法本地验证。Tauri 2.11+ 的 `bundle.icon` 已支持直接给 `.icon` / `Assets.car`
  （tauri-apps/tauri#14671），需要时可在 Mac 上用 Icon Composer 做一版再挂进 `tauri.macos.conf.json`。
  若栅格修正后 Dock 里仍是灰底方块，请拍一张 Dock 截图再决定是否走这条路。
- 打包分发已在本机执行：`pnpm tauri build` 产出
  `SSH Tunnel Manager_0.2.0_x64_en-US.msi`（2.14 MB）与
  `SSH Tunnel Manager_0.2.0_x64-setup.exe`（1.53 MB）；
  macOS（dmg/app）需在 macOS 机器上执行同样的 `pnpm tauri build`。
