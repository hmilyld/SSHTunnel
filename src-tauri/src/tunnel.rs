//! 隧道生命周期核心：启动 `ssh -L`、状态查询、停止与重启。
//!
//! 生命周期设计（记录持久化，除非用户明确删除）：
//! - **新建**：校验参数 → 启动 ssh → 等待本地端口进入监听 → 记录落盘（此后一直保留）
//! - **关闭**：终止进程（优雅→强制）→ **保留记录**，PID 清零、状态变为「已停止」
//! - **启动**（复用）：用保存的记录重新拉起 ssh，更新 PID；记录与 ID 不变
//! - **删除**：仅在记录已停止时由用户显式移除
//!
//! 启动细节：
//! 1. `tokio::process` 执行
//!    `ssh -N -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 -o ServerAliveCountMax=3 -L <spec> <host>`
//!    - Windows 下附加 `CREATE_NO_WINDOW`，不弹控制台黑窗
//!    - stderr 用后台任务持续读取（失败归因 + 防止管道写满阻塞 ssh）
//! 2. 每 150ms 探测本地 `bind:port` 是否进入监听：出现即就绪（ssh 认证通过后才会绑定）；
//!    进程自行退出则解析 stderr 给出友好失败原因；18s 仍无监听则超时失败
//! 3. 就绪后启动“回收任务”（ssh 退出时向前端广播 `tunnel-exited` 即时刷新）

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::error::AppError;
use crate::process;
use crate::store::{Store, TunnelRecord, TunnelView};

/// Tauri 托管的全局状态
pub struct AppState {
    pub store: Mutex<Store>,
}

impl AppState {
    pub fn new(store: Store) -> Self {
        Self {
            store: Mutex::new(store),
        }
    }

    /// 当前所有隧道（记录 + 实时存活状态）
    ///
    /// `alive` 使用 `is_ssh_process`：进程已退出、PID 为 0（已关闭）、或 PID 被其他
    /// 程序复用，都显示“已停止”。
    pub fn list_views(&self) -> Vec<TunnelView> {
        let store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        store
            .list()
            .iter()
            .map(|record| TunnelView {
                record: record.clone(),
                alive: record.pid > 0 && process::is_ssh_process(record.pid),
            })
            .collect()
    }
}

/// 前端“新建转发”表单（snake_case 与 JS 字段一致）
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct StartRequest {
    /// ~/.ssh/config 中的 Host 别名
    pub host: String,
    /// 本地绑定地址，默认 127.0.0.1
    pub bind: String,
    pub local_port: u32,
    /// 远程目标主机，默认 localhost
    pub remote_host: String,
    pub remote_port: u32,
}

impl StartRequest {
    fn validate(&self) -> Result<(), AppError> {
        if self.host.trim().is_empty() {
            return Err(AppError::Validation("请选择 SSH Host".into()));
        }
        if self.remote_host.trim().is_empty() {
            return Err(AppError::Validation("远程主机不能为空（默认 localhost）".into()));
        }
        if self.bind.trim().is_empty() {
            return Err(AppError::Validation("本地绑定地址不能为空（默认 127.0.0.1）".into()));
        }
        if !(1..=65535).contains(&self.local_port) {
            return Err(AppError::Validation(format!(
                "本地端口 {} 无效，应在 1-65535 之间",
                self.local_port
            )));
        }
        if !(1..=65535).contains(&self.remote_port) {
            return Err(AppError::Validation(format!(
                "远程端口 {} 无效，应在 1-65535 之间",
                self.remote_port
            )));
        }
        Ok(())
    }

    /// `-L` 参数：`bind:local_port:remote_host:remote_port`
    fn spec(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.bind, self.local_port, self.remote_host, self.remote_port
        )
    }
}

/// ssh 退出事件（由回收任务广播给前端，用于即时把状态刷成“已停止”）
#[derive(Debug, Clone, Serialize)]
pub struct TunnelExited {
    pub id: String,
    pub stderr: String,
}

/// 终止一个「尚未交给回收任务」的 ssh 子进程并回收它。
///
/// ssh 是 `kill_on_drop(false)` 启动的（隧道要能被用户长期持有），因此失败/超时路径
/// 必须显式 kill + `wait`：只 kill 不 wait 会留下僵尸进程句柄。
async fn kill_unstarted(child: &mut tokio::process::Child) {
    if let Err(e) = child.start_kill() {
        tracing::debug!("终止未启动完成的 ssh 进程失败：{e}");
    }
    if let Err(e) = child.wait().await {
        tracing::debug!("回收未启动完成的 ssh 进程失败：{e}");
    }
}

/// 提取 stderr 中最后一条有意义的行（跳过 Warning）
fn last_meaningful_line(stderr: &str) -> String {
    stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with("Warning:"))
        .unwrap_or("")
        .trim()
        .to_string()
}

/// 服务器是否“接受密码认证”（`Permission denied (publickey,password)`）
///
/// 满足两个条件才判定：明确拒绝认证，且方法列表里带 `password`——
/// 只允许 publickey 的服务器不会触发密码框。
fn is_password_required(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    lower.contains("permission denied") && lower.contains("password")
}

/// 把 ssh 的退出状态与 stderr 归纳成人话
fn explain_failure(status: std::process::ExitStatus, stderr: &str) -> String {
    let stderr = stderr.trim();
    let last_line = last_meaningful_line(stderr);

    let hint = {
        let lower = stderr.to_lowercase();
        if lower.contains("address already in use") || lower.contains("only one usage of each socket") {
            "本地端口已被占用"
        } else if lower.contains("permission denied") {
            "认证失败，请确认已配置密钥登录或 ssh-agent"
        } else if lower.contains("could not resolve hostname") || lower.contains("name or service not known")
        {
            "无法解析主机名，请检查 ~/.ssh/config"
        } else if lower.contains("connection refused") {
            "目标拒绝连接"
        } else if lower.contains("connection timed out") || lower.contains("operation timed out") {
            "连接超时"
        } else if lower.contains("network is unreachable") {
            "网络不可达"
        } else if lower.contains("no route to host") {
            "无法路由到目标主机"
        } else {
            ""
        }
    };

    if last_line.is_empty() {
        format!("ssh 进程已退出（{status}）")
    } else if hint.is_empty() {
        last_line
    } else {
        format!("{hint}（{last_line}）")
    }
}

/// 生成 8 位短 ID（截断 UUID v4）
fn short_id() -> String {
    let full = uuid::Uuid::new_v4().simple().to_string();
    full[..8].to_string()
}

/// 在 `store` 中生成一个未被占用的 8 位 ID。
///
/// 8 位十六进制空间虽大，但 ID 是记录的唯一定位键（`get`/`remove`/`update` 都按 ID 匹配）；
/// 一旦碰撞，后加入的记录将无法被单独操作、并可能留下杀不掉的 ssh 进程，
/// 因此这里显式排重，而不是假装碰撞不可能发生。
fn unique_id(store: &Store) -> String {
    loop {
        let id = short_id();
        if store.get(&id).is_none() {
            return id;
        }
        tracing::warn!("生成的隧道 ID {id} 已被占用，重新生成");
    }
}

/// `bind:port` 是否已被**运行中**的隧道占用（`exclude_id` 用于跳过记录自身）。
///
/// “运行中”= PID 非 0 且当前确实是 ssh 进程，与 [`AppState::list_views`] 的判定一致。
fn local_port_taken(store: &Store, exclude_id: &str, bind: &str, port: u16) -> bool {
    store.list().iter().any(|r| {
        r.id != exclude_id && r.bind == bind && r.local_port == port && r.pid > 0 && process::is_ssh_process(r.pid)
    })
}

/// askpass 可执行文件路径。
///
/// Windows 下部分 OpenSSH 版本用空格分隔拼命令行，安装到
/// `C:\Program Files\...` 等带空格路径时可能解析失败——优先取 8.3 短路径。
#[cfg(windows)]
fn askpass_exe_path() -> String {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetShortPathNameW(
            lpsz_long_path: *const u16,
            lpsz_short_path: *mut u16,
            cch_buffer: u32,
        ) -> u32;
    }

    let exe = std::env::current_exe().unwrap_or_default();
    let wide: Vec<u16> = exe
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let need = GetShortPathNameW(wide.as_ptr(), std::ptr::null_mut(), 0);
        if need > 0 {
            let mut buf = vec![0u16; need as usize];
            let got = GetShortPathNameW(wide.as_ptr(), buf.as_mut_ptr(), need);
            if got > 0 && got < need {
                let short = String::from_utf16_lossy(&buf[..got as usize]);
                if !short.contains(' ') {
                    return short;
                }
            }
        }
    }
    exe.to_string_lossy().into_owned()
}

/// Unix 侧 askpass 路径即当前可执行文件本身
#[cfg(unix)]
fn askpass_exe_path() -> String {
    std::env::current_exe()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// 给 ssh 注入 askpass 相关环境（密码只进子进程环境，不落盘、不进日志）
fn apply_askpass_env(cmd: &mut tokio::process::Command, password: &str) {
    cmd.env("SSH_ASKPASS", askpass_exe_path());
    cmd.env("SSH_ASKPASS_REQUIRE", "force");
    cmd.env("STM_ASKPASS_PWD", password);
}

/// 密码认证预检用的参数：只做认证不建转发（`-T`），且只走密码类认证
fn password_probe_args(host: &str) -> Vec<String> {
    vec![
        "-T".into(),
        "-o".into(),
        "NumberOfPasswordPrompts=1".into(),
        "-o".into(),
        "ConnectTimeout=10".into(),
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
        "-o".into(),
        "PreferredAuthentications=password,keyboard-interactive".into(),
        host.into(),
    ]
}

/// 密码认证预检：单独跑一次 `ssh -T`（不建立转发），确认密码正确后才真正启动隧道。
///
/// - 返回 `Err(PasswordRequired)`：服务器明确拒绝（密码错误）→ 前端在密码弹窗内红字提示
/// - 返回 `Ok(())`：认证通过；或预检无法判定（连接类错误/超时）——
///   交由后续真正的隧道启动给出真实错误，不阻塞用户
///
/// 注意预检只等 5s，而 `ssh` 自身的 `ConnectTimeout=10`：连不上的主机不会被这里判死，
/// 一律 fail-open，由真正的隧道启动（`ConnectTimeout=15` + 18s 监听超时）给出错误。
async fn verify_password_auth(host: &str, password: &str) -> Result<(), AppError> {
    let mut cmd = tokio::process::Command::new("ssh");
    cmd.args(password_probe_args(host))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        // 预检进程归本函数管理：超时/提前返回都会被回收
        .kill_on_drop(true);

    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    apply_askpass_env(&mut cmd, password);

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::SshNotFound
        } else {
            AppError::StartFailed {
                reason: format!("创建 ssh 预检进程失败：{e}"),
            }
        }
    })?;

    // 后台读取 stderr，进程退出后 join 拿到完整内容
    let stderr_buf: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let reader = {
        let buf = Arc::clone(&stderr_buf);
        if let Some(mut pipe) = child.stderr.take() {
            Some(tauri::async_runtime::spawn(async move {
                use tokio::io::AsyncReadExt;
                let mut chunk = [0u8; 4096];
                let mut text = String::new();
                loop {
                    match pipe.read(&mut chunk).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => text.push_str(&String::from_utf8_lossy(&chunk[..n])),
                    }
                }
                *buf.lock().unwrap_or_else(|e| e.into_inner()) = text;
            }))
        } else {
            None
        }
    };

    // 认证往返 + 服务器 PAM 处理通常 <1.5s；最多等 5s，超时则放弃判定（fail-open）
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if let Some(reader) = reader {
                    let _ = reader.await;
                }
                let detail = stderr_buf
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                if is_password_required(&detail) {
                    tracing::warn!("密码预检未通过：{}", last_meaningful_line(&detail));
                    return Err(AppError::PasswordRequired(last_meaningful_line(&detail)));
                }
                tracing::info!("密码预检结束（{status}，无密码拒绝）→ 继续启动隧道");
                return Ok(());
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    tracing::warn!("密码预检超过 5s 未结束，放弃判定并继续启动隧道");
                    kill_unstarted(&mut child).await;
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(e) => {
                return Err(AppError::StartFailed {
                    reason: format!("等待 ssh 预检失败：{e}"),
                })
            }
        }
    }
}

/// 绑定地址 → 探测目标 IP 列表（`None` 表示是主机名，需要 DNS 解析）
///
/// - `0.0.0.0`（v4 通配）→ 连 127.0.0.1
/// - `localhost` → 127.0.0.1 与 ::1 都试（ssh 可能绑任一协议栈）
/// - `::`（v6 通配）→ 先 ::1 再 127.0.0.1
/// - 具体 IP → 直连该地址
fn probe_addrs(bind: &str) -> Option<Vec<std::net::IpAddr>> {
    use std::net::{IpAddr, Ipv4Addr};
    let v4loop: IpAddr = Ipv4Addr::LOCALHOST.into();
    let v6loop: IpAddr = std::net::Ipv6Addr::LOCALHOST.into();
    Some(match bind.trim() {
        "0.0.0.0" => vec![v4loop],
        "localhost" => vec![v4loop, v6loop],
        "::" => vec![v6loop, v4loop],
        s => match s.parse::<IpAddr>() {
            Ok(ip) => vec![ip],
            Err(_) => return None, // 主机名，调用方走 DNS
        },
    })
}

/// 本地端口是否已进入监听（250ms/地址 上限）
///
/// ssh 在**认证通过之后**才会绑定 `-L` 的本地监听端口，因此“监听出现”
/// 是比固定时间窗口可靠得多的“隧道就绪”信号。
async fn is_listening(bind: &str, port: u16) -> bool {
    let mut addrs = match probe_addrs(bind) {
        Some(a) => a,
        None => {
            // 绑定值是主机名：交给 DNS 解析（失败则视为未监听）
            match tokio::net::lookup_host((bind.trim(), port)).await {
                Ok(iter) => iter.map(|a| a.ip()).collect(),
                Err(_) => return false,
            }
        }
    };
    addrs.dedup();

    for ip in addrs {
        let fut = async {
            match tokio::net::TcpStream::connect((ip, port)).await {
                Ok(_s) => {
                    // 连上即证明监听存在；立即关闭，不产生任何数据传输
                    true
                }
                Err(_) => false,
            }
        };
        match tokio::time::timeout(Duration::from_millis(250), fut).await {
            Ok(true) => return true,
            Ok(false) => continue,
            Err(_) => continue, // 超时（防火墙黑洞等）→ 试下一个地址
        }
    }
    false
}

/// 拉起一个 `ssh -L` 进程并判定其真正就绪。
///
/// - `password = None`：追加 `-o BatchMode=yes`（禁止任何交互，认证失败快速返回）
/// - `password = Some`：先做密码预检（5s 上限、fail-open），再通过 `SSH_ASKPASS` 注入密码
///
/// **成功判定**：本地 `bind:port` 进入监听（而非固定等待时间）；
/// **失败判定**：进程退出（即时分类为密码错误/端口占用/连接失败等），
/// 或 18s 仍未监听（ssh 自身 `ConnectTimeout=15` 通常会先退出并给出真实原因）。
async fn spawn_ssh(
    spec: &str,
    host: &str,
    password: Option<&str>,
    bind: &str,
    local_port: u16,
) -> Result<(tokio::process::Child, Arc<Mutex<String>>), AppError> {
    // ===== 0. 环境预检：本地端口已被其他进程占用 → 立即明确报错（也不用浪费连接） =====
    if is_listening(bind, local_port).await {
        let mut msg = format!("本地端口 {bind}:{local_port} 已被占用");
        if let Some((pid, name)) = process::port_listener_info(local_port) {
            msg.push_str(&format!(" — 占用进程：{name}（PID {pid}）"));
            let lower = name.to_ascii_lowercase();
            if local_port == 5173 && lower.starts_with("node") {
                msg.push_str(
                    "。提示：5173 是 Vite 开发服务器默认端口，若正在运行 pnpm dev / pnpm tauri dev，\
                     请更换本地端口或先停止前端开发服务",
                );
            } else if lower.starts_with("ssh") {
                msg.push_str(
                    "。该进程疑似此前未正常关闭的 ssh -L 隧道残留，可先结束它或更换本地端口",
                );
            }
        }
        return Err(AppError::PortInUse(msg));
    }

    // ===== 1. 密码模式：先预检认证，密码错误在此阶段直接返回（不会出现“成功后立刻失败”） =====
    if let Some(pwd) = password {
        verify_password_auth(host, pwd).await?;
    }

    let mut args: Vec<String> = vec![
        "-N".into(),
        "-o".into(),
        "ExitOnForwardFailure=yes".into(),
        "-o".into(),
        "ServerAliveInterval=30".into(),
        "-o".into(),
        "ServerAliveCountMax=3".into(),
        // 连接阶段硬上限：连不上 ssh 会在 15s 内自行退出 → 走失败分类
        "-o".into(),
        "ConnectTimeout=15".into(),
        // 新主机自动按 TOFU 接受（已变更的主机密钥仍会硬失败），
        // 避免首次连接时 yes/no 询问在无终端环境下卡死
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
    ];
    if password.is_some() {
        args.push("-o".into());
        args.push("NumberOfPasswordPrompts=1".into());
    } else {
        args.push("-o".into());
        args.push("BatchMode=yes".into());
    }
    args.push("-L".into());
    args.push(spec.into());
    args.push(host.into());

    let mut cmd = tokio::process::Command::new("ssh");
    cmd.args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        // 句柄被丢弃时不杀进程：隧道归用户手动管理
        .kill_on_drop(false);

    // Windows：不弹控制台黑窗
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    // 密码模式：让 ssh 通过 askpass（本程序）取密码，密码只进子进程环境
    if let Some(pwd) = password {
        apply_askpass_env(&mut cmd, pwd);
    }

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::SshNotFound
        } else {
            AppError::StartFailed {
                reason: format!("创建 ssh 进程失败：{e}"),
            }
        }
    })?;

    // 持续读取 stderr：失败时用于归因；运行时也防止管道写满阻塞 ssh
    let stderr_buf: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    if let Some(mut pipe) = child.stderr.take() {
        let buf = Arc::clone(&stderr_buf);
        tauri::async_runtime::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut chunk = [0u8; 4096];
            let mut text = String::new();
            loop {
                match pipe.read(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => text.push_str(&String::from_utf8_lossy(&chunk[..n])),
                }
            }
            *buf.lock().unwrap_or_else(|e| e.into_inner()) = text;
        });
    }

    // ===== 2. 成功判定：本地端口进入监听（认证通过后 ssh 才会绑定） =====
    //    失败判定：进程退出（即时分类）；18s 仍未监听 → 杀进程并报超时。
    //    （ssh 自身 ConnectTimeout=15 会先退出给出真实原因，这里兜底“卡认证/黑洞”）
    let deadline = std::time::Instant::now() + Duration::from_secs(18);
    loop {
        if is_listening(bind, local_port).await {
            tracing::info!("本地端口 {bind}:{local_port} 已监听 → 隧道就绪");
            return Ok((child, stderr_buf));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let detail = stderr_buf
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                tracing::warn!("ssh 启动后退出：{status}；stderr: {detail}");
                // 分类一：服务器接受密码认证 → 交给前端弹密码框
                //（已提供密码仍失败 = 密码错误，同样走此路径让用户重新输入）
                if is_password_required(&detail) {
                    return Err(AppError::PasswordRequired(last_meaningful_line(&detail)));
                }
                return Err(AppError::StartFailed {
                    reason: explain_failure(status, &detail),
                });
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    // 超时兜底：杀掉并回收，绝不把 ssh 进程留在后台
                    kill_unstarted(&mut child).await;
                    return Err(AppError::StartFailed {
                        reason: format!(
                            "建立隧道超时：18 秒内 {bind}:{local_port} 未进入监听（服务器连接/认证过慢，或本地端口被防火墙拦截）"
                        ),
                    });
                }
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            Err(e) => {
                return Err(AppError::StartFailed {
                    reason: format!("等待 ssh 进程失败：{e}"),
                })
            }
        }
    }
}

/// 回收任务：ssh 退出（服务端断开/被外部 kill/被关闭）时广播 `tunnel-exited`
fn spawn_watcher(
    app: &AppHandle,
    id: String,
    mut child: tokio::process::Child,
    stderr: Arc<Mutex<String>>,
) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match child.wait().await {
            Ok(status) => tracing::info!("隧道 {id} 的 ssh 进程退出：{status}"),
            Err(e) => tracing::warn!("隧道 {id} 等待 ssh 退出失败：{e}"),
        }
        let stderr = stderr
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let _ = app.emit("tunnel-exited", TunnelExited { id, stderr });
    });
}

/// 新建一条隧道：启动 ssh + 记录落盘（记录此后长期保留）
///
/// `password` 为空时以 BatchMode 探测；若服务器要求密码会返回
/// [`AppError::PasswordRequired`]，前端弹出密码框后带密码再次调用。
pub async fn start_tunnel(
    app: &AppHandle,
    state: &AppState,
    req: StartRequest,
    password: Option<String>,
) -> Result<TunnelView, AppError> {
    req.validate()?;
    let spec = req.spec();

    // 预检：同一 bind:local_port 已有运行中的转发，提前给出友好错误
    {
        let store = state.store.lock().unwrap_or_else(|e| e.into_inner());
        if local_port_taken(&store, "", &req.bind, req.local_port as u16) {
            return Err(AppError::PortInUse(format!(
                "本地端口 {}:{} 已被占用 — 已有正在运行的相同转发",
                req.bind, req.local_port
            )));
        }
    }

    let (mut child, stderr_buf) =
        spawn_ssh(&spec, req.host.trim(), password.as_deref(), &req.bind, req.local_port as u16)
            .await?;
    let pid = child
        .id()
        .ok_or_else(|| AppError::StartFailed {
            reason: "无法获取 ssh 进程 PID".into(),
        })?;

    // ===== 持久化（关闭后也不会丢失，除非用户显式删除） =====
    // 锁必须在任何 .await 之前释放（`AppState.store` 是 std::sync::Mutex）。
    let saved = {
        let mut store = state.store.lock().unwrap_or_else(|e| e.into_inner());
        let record = TunnelRecord {
            id: unique_id(&store),
            host: req.host.trim().to_string(),
            bind: req.bind.trim().to_string(),
            local_port: req.local_port as u16,
            remote_host: req.remote_host.trim().to_string(),
            remote_port: req.remote_port as u16,
            pid,
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };
        store.add(record.clone());
        match store.save() {
            // 落盘失败就撤回刚加入的内存记录，交给下面的清理逻辑杀掉 ssh
            Err(e) => {
                store.remove(&record.id);
                Err(e)
            }
            Ok(()) => Ok(record),
        }
    };
    let record = match saved {
        Ok(record) => record,
        Err(e) => {
            kill_unstarted(&mut child).await;
            return Err(e);
        }
    };

    spawn_watcher(app, record.id.clone(), child, Arc::clone(&stderr_buf));

    tracing::info!("隧道已启动 {spec} -> {}（PID {pid}）", record.id);
    Ok(TunnelView {
        record,
        alive: true,
    })
}

/// 关闭一条隧道：**终止进程但保留记录**（PID 清零 → 状态「已停止」，可随时重新启动）
pub async fn stop_tunnel(state: &AppState, id: &str) -> Result<Vec<TunnelView>, AppError> {
    let record = {
        let store = state.store.lock().unwrap_or_else(|e| e.into_inner());
        store
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(id.to_string()))?
    };

    if record.pid > 0 && process::is_pid_alive(record.pid) {
        if process::is_ssh_process(record.pid) {
            process::stop_process(record.pid).await?;
        } else {
            // PID 已被其他程序复用：不动别人的进程，只把记录标记为已停止
            tracing::warn!(
                "PID {} 已被其他进程复用，跳过终止，仅将记录 {} 标记为已停止",
                record.pid,
                record.id
            );
        }
    }

    let views = {
        let mut store = state.store.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(r) = store.get_mut(id) {
            r.pid = 0; // 记录保留，PID 清零表示“无运行进程”
        }
        store.save()?;
        drop(store);
        state.list_views()
    };
    tracing::info!("隧道 {id} 已关闭（记录保留，可重新启动）");
    Ok(views)
}

/// 用保存的记录重新启动隧道（复用配置，ID/创建时间不变，仅更新 PID）。
/// 密码语义与 [`start_tunnel`] 一致。
pub async fn restart_tunnel(
    app: &AppHandle,
    state: &AppState,
    id: &str,
    password: Option<String>,
) -> Result<Vec<TunnelView>, AppError> {
    let record = {
        let store = state.store.lock().unwrap_or_else(|e| e.into_inner());
        store
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(id.to_string()))?
    };

    // 已在运行则拒绝重复启动
    if record.pid > 0 && process::is_ssh_process(record.pid) {
        return Err(AppError::Process(format!(
            "隧道 {} 已在运行（PID {}）",
            record.id, record.pid
        )));
    }

    // 与新建完全一致的启动链路（含重复 bind:port 预检）
    let spec = format!(
        "{}:{}:{}:{}",
        record.bind, record.local_port, record.remote_host, record.remote_port
    );
    {
        let store = state.store.lock().unwrap_or_else(|e| e.into_inner());
        if local_port_taken(&store, &record.id, &record.bind, record.local_port) {
            return Err(AppError::PortInUse(format!(
                "本地端口 {}:{} 已被占用 — 已有另一条正在运行的转发使用它",
                record.bind, record.local_port
            )));
        }
    }

    let (mut child, stderr_buf) = spawn_ssh(
        &spec,
        &record.host,
        password.as_deref(),
        &record.bind,
        record.local_port,
    )
    .await?;
    let pid = child
        .id()
        .ok_or_else(|| AppError::StartFailed {
            reason: "无法获取 ssh 进程 PID".into(),
        })?;

    // 落盘：失败时回退内存中的 PID，并把刚拉起的 ssh 交给下面的清理逻辑杀掉。
    // 注意锁必须在任何 .await 之前释放（`AppState.store` 是 std::sync::Mutex）。
    let save_result = {
        let mut store = state.store.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(r) = store.get_mut(id) {
            r.pid = pid;
        }
        let result = store.save();
        if result.is_err() {
            if let Some(r) = store.get_mut(id) {
                r.pid = 0;
            }
        }
        result
    };
    if let Err(e) = save_result {
        kill_unstarted(&mut child).await;
        return Err(e);
    }

    spawn_watcher(app, record.id.clone(), child, stderr_buf);

    tracing::info!("隧道 {id} 已重新启动 {spec}（PID {pid}）");
    Ok(state.list_views())
}

/// 显式删除记录（仅允许删除已停止的记录，避免留下孤儿进程）
pub fn remove_record(state: &AppState, id: &str) -> Result<Vec<TunnelView>, AppError> {
    let mut store = state.store.lock().unwrap_or_else(|e| e.into_inner());
    let record = store
        .get(id)
        .ok_or_else(|| AppError::NotFound(id.to_string()))?
        .clone();
    if record.pid > 0 && process::is_ssh_process(record.pid) {
        return Err(AppError::Validation(
            "该隧道仍在运行，请先关闭再删除记录".into(),
        ));
    }
    store.remove(id);
    store.save()?;
    drop(store);
    Ok(state.list_views())
}

/// 修改一条已保存记录的配置（不涉及进程；ID 与创建时间不变）
///
/// 约束：
/// - 正在运行的记录不可修改（先关闭）
/// - 新的 bind:local_port 不能与**其他正在运行**的记录冲突
pub fn update_record(
    state: &AppState,
    id: &str,
    req: StartRequest,
) -> Result<Vec<TunnelView>, AppError> {
    req.validate()?;

    let mut store = state.store.lock().unwrap_or_else(|e| e.into_inner());
    let record = store
        .get(id)
        .ok_or_else(|| AppError::NotFound(id.to_string()))?
        .clone();
    if record.pid > 0 && process::is_ssh_process(record.pid) {
        return Err(AppError::Validation(
            "该隧道正在运行，请先关闭再修改".into(),
        ));
    }

    let conflict = local_port_taken(&store, &record.id, &req.bind, req.local_port as u16);
    if conflict {
        return Err(AppError::PortInUse(format!(
            "本地端口 {}:{} 已被占用 — 与另一条正在运行的转发冲突",
            req.bind, req.local_port
        )));
    }

    if let Some(r) = store.get_mut(id) {
        r.host = req.host.trim().to_string();
        r.bind = req.bind.trim().to_string();
        r.local_port = req.local_port as u16;
        r.remote_host = req.remote_host.trim().to_string();
        r.remote_port = req.remote_port as u16;
        r.pid = 0; // 保持「已停止」状态
    }
    store.save()?;
    drop(store);

    tracing::info!(
        "隧道 {id} 配置已更新：{}:{} -> {}:{}",
        req.bind,
        req.local_port,
        req.remote_host,
        req.remote_port
    );
    Ok(state.list_views())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_password_required_only_when_password_offered() {
        // 服务器接受密码认证 → 前端需要弹密码框
        assert!(is_password_required(
            "root@host's password: \nPermission denied (publickey,password)."
        ));
        // 大小写不敏感
        assert!(is_password_required("Permission Denied (Publickey,Password)."));
        // 只允许公钥 → 不弹密码框（弹了也没用）
        assert!(!is_password_required("Permission denied (publickey)."));
        // 网络类错误与密码无关
        assert!(!is_password_required(
            "ssh: connect to host x port 22: Connection refused"
        ));
    }

    #[test]
    fn password_error_carries_frontend_prefix() {
        let e = AppError::PasswordRequired("Permission denied (publickey,password).".into());
        assert!(e.to_string().starts_with("PASSWORD_REQUIRED::"));
    }

    #[test]
    fn last_line_skips_warnings() {
        let s = "Warning: Permanently added [1.2.3.4] (ED25519) to the list of known hosts.\nPermission denied (publickey,password).";
        assert_eq!(
            last_meaningful_line(s),
            "Permission denied (publickey,password)."
        );
    }

    #[test]
    fn password_probe_only_uses_password_auth() {
        let args = password_probe_args("dev");
        let joined = args.join(" ");
        // 只做认证不建转发
        assert!(joined.starts_with("-T "));
        // 限定密码类认证，且只尝试一次（错误密码立即返回）
        assert!(joined.contains("PreferredAuthentications=password,keyboard-interactive"));
        assert!(joined.contains("NumberOfPasswordPrompts=1"));
        // 主机名在最后
        assert_eq!(args.last().map(String::as_str), Some("dev"));
    }

    #[test]
    fn probe_addrs_follow_bind_rules() {
        use std::net::IpAddr;
        let v4: IpAddr = "127.0.0.1".parse().unwrap();
        let v6: IpAddr = "::1".parse().unwrap();
        // 通配 / 具体 IP 的映射
        assert_eq!(probe_addrs("0.0.0.0"), Some(vec![v4]));
        assert_eq!(probe_addrs("127.0.0.1"), Some(vec![v4]));
        assert_eq!(probe_addrs("::1"), Some(vec![v6]));
        assert_eq!(probe_addrs("::"), Some(vec![v6, v4]));
        // localhost 双栈都试
        assert_eq!(probe_addrs("localhost"), Some(vec![v4, v6]));
        // 主机名 → 交给 DNS（返回 None）
        assert_eq!(probe_addrs("my-server.lan"), None);
    }
}