# 故障排查与已知限制

## 本地端口被占用

启动前会检查本地 `bind:port` 是否已有监听。错误信息会附带占用进程名与 PID。

在 `pnpm tauri dev` 运行时，Vite 默认占用 5173 端口。若隧道本地端口设为 5173，
启动会失败并提示 Node 进程占用，这是预期行为。请更换端口或停止开发服务器。

## 密码认证失败

- 服务器仅允许公钥认证时不会弹出密码框；
- 密码错误会在 `ssh -T` 预检阶段返回，不会先提示成功再失败；
- 预检超时或连接类错误会 fail-open，由实际隧道启动给出错误；
- 密码不会写入磁盘或日志。

## 进程状态显示“已停止”

状态判断要求 PID 存活且进程名为 `ssh`。以下情况会显示已停止：

- ssh 进程已退出；
- 记录中的 PID 为 0；
- PID 被系统复用给其他程序；
- 进程名不是 `ssh` / `ssh.exe`。

## 日志与数据

- Windows：`%APPDATA%\ssh-tunnel-manager\`
- macOS：`~/Library/Application Support/ssh-tunnel-manager/`

`app.log` 超过 5 MB 会轮转为 `app.log.1`。界面底部文件夹按钮可直接打开数据目录。
`tunnels.json` 损坏时会备份为 `tunnels.json.bak`。

## `Include` 与 `~/.ssh/config`

`Include` 的相对路径基于配置文件所在目录（默认即 `~/.ssh`）。支持 `~` 与 glob；
循环引用会跳过，嵌套上限 16 层。通配符 Host 与 `Match` 块不参与解析。

## 单实例

第二次启动会聚焦已有窗口，不会创建新的 GUI 进程。若窗口不可见，可从托盘菜单
选择“显示主窗口”，或结束残留的 `ssh-tunnel-manager` 进程后重新启动。

## macOS

- 使用系统原生标题栏，自绘标题栏仅用于 Windows；
- 菜单栏使用单色模板图标；
- ICNS 使用 824-on-1024 栅格；完整的 Liquid Glass 外观需要 `.icon` / `Assets.car`，
  该构建需要 macOS 与 Xcode 26；
- 安装包未签名、未公证，首次打开需要在系统设置中放行；
- macOS 端代码路径需在 macOS 12+ 实机执行 `pnpm tauri dev` 与 `pnpm tauri build` 回归。

## Windows

- 子进程使用 `CREATE_NO_WINDOW`，不会出现控制台窗口；
- 标题栏按钮依赖 `src-tauri/capabilities/default.json` 中的窗口权限；
- 首次打包会下载 WiX/NSIS 打包器。

## 构建与工具链

- Node 需 22.22+；Vitest 5 / jsdom 31 不支持 Node 25；
- Rust 需 1.90+；
- 依赖变更后需提交 `pnpm-lock.yaml` 与 `Cargo.lock`；
- 发布前运行 `pnpm tauri build`，确认当前平台的安装包可生成。
