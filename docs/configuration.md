# 配置与数据

## 数据目录

| 平台 | 路径 |
| --- | --- |
| Windows | `%APPDATA%\ssh-tunnel-manager\` |
| macOS | `~/Library/Application Support/ssh-tunnel-manager/` |

目录内文件：

- `tunnels.json`：隧道记录；
- `app.log`：应用日志；启动时超过 5 MB 会轮转为 `app.log.1`。

底部信息栏的文件夹按钮调用 `reveal_data_dir`，由后端解析数据目录并在系统文件
管理器中打开。该命令不接受前端传入路径。

## tunnels.json

文件内容为记录数组，字段使用 snake_case：

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

| 字段 | 说明 |
| --- | --- |
| `id` | 8 位十六进制短 ID，作为记录唯一键 |
| `host` | `~/.ssh/config` 中的 Host 别名 |
| `bind` | 本地绑定地址，如 `127.0.0.1`、`localhost`、`0.0.0.0`、`::1` |
| `local_port` | 本地端口，1–65535 |
| `remote_host` | 从 SSH 服务器视角访问的地址 |
| `remote_port` | 远程端口，1–65535 |
| `pid` | 当前 ssh 进程 PID；0 表示已停止 |
| `created_at` | 本地时间，格式 `YYYY-MM-DD HH:MM:SS` |

写入采用临时文件加 rename 的原子替换。文件损坏时，启动阶段会将其备份为
`tunnels.json.bak` 并以空列表继续。保存失败时，内存记录会恢复到操作前的快照。

## ~/.ssh/config

解析器支持：

- `Host`，包括一行多个别名；
- `HostName`、`User`、`Port`、`IdentityFile`；
- `Key Value`、`Key=Value` 与 `#` 注释；
- `Include`：相对路径基于配置文件所在目录（默认即 `~/.ssh`），支持 `~` 与 glob；
  循环引用跳过，嵌套上限 16 层。

重复的 `Host` 块遵循 OpenSSH 的“每个参数首个值生效”规则。包含 `*`、`?` 或以
`!` 开头的通配符 Host 会被忽略；`Match` 块不参与静态解析。

## 认证

### 密钥认证

优先使用 `~/.ssh/config` 中的密钥、`IdentityFile` 或 ssh-agent。启动参数包含
`StrictHostKeyChecking=accept-new`：首次连接的新主机自动接受，已变更的主机密钥
仍会失败。

### 密码认证

1. 首次启动附加 `BatchMode=yes`，禁止交互式提示。
2. 若 stderr 返回 `Permission denied (publickey,password)`，且方法列表中包含
   `password`，后端返回 `PASSWORD_REQUIRED::…`，前端显示密码输入框。
3. 用户确认后，先执行一次 `ssh -T` 预检，限定
   `PreferredAuthentications=password,keyboard-interactive` 与
   `NumberOfPasswordPrompts=1`。
4. 预检通过后启动隧道。预检超时或连接类错误时 fail-open，由实际启动过程给出错误。
5. 密码通过 `SSH_ASKPASS`、`SSH_ASKPASS_REQUIRE=force` 与 `STM_ASKPASS_PWD`
   传入 ssh 子进程环境；不会写入 `tunnels.json` 或日志。

Windows 下 askpass 路径优先使用 8.3 短路径，以兼容包含空格的安装目录。

## 启动参数

隧道进程使用以下参数（省略密码预检参数）：

```text
ssh -N
  -o ExitOnForwardFailure=yes
  -o ServerAliveInterval=30
  -o ServerAliveCountMax=3
  -o ConnectTimeout=15
  -o StrictHostKeyChecking=accept-new
  -L <bind>:<local_port>:<remote_host>:<remote_port>
  <host>
```

密码模式附加 `-o NumberOfPasswordPrompts=1`；密钥模式附加 `-o BatchMode=yes`。
