//! 启动 `ssh -L` 子进程：askpass、密码预检、端口监听判定与失败回收。
//!
//! 与 `tunnel.rs` 的生命周期/持久化分开，便于单独测试与维护。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::error::AppError;
use crate::process;
use crate::ssh_failure::{is_password_required, last_meaningful_line, ssh_exit_error};

/// 终止一个「尚未交给回收任务」的 ssh 子进程并回收它。
///
/// ssh 是 `kill_on_drop(false)` 启动的（隧道要能被用户长期持有），因此失败/超时路径
/// 必须显式 kill + `wait`：只 kill 不 wait 会留下僵尸进程句柄。
pub(crate) async fn kill_unstarted(child: &mut tokio::process::Child) {
    if let Err(e) = child.start_kill() {
        tracing::debug!("终止未启动完成的 ssh 进程失败：{e}");
    }
    if let Err(e) = child.wait().await {
        tracing::debug!("回收未启动完成的 ssh 进程失败：{e}");
    }
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
pub(crate) fn password_probe_args(host: &str) -> Vec<String> {
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
        child.stderr.take().map(|mut pipe| {
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
            })
        })
    };

    // 认证往返 + 服务器 PAM 处理通常 <1.5s；最多等 5s，超时则放弃判定（fail-open）
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if let Some(reader) = reader {
                    let _ = reader.await;
                }
                let detail = stderr_buf.lock().unwrap_or_else(|e| e.into_inner()).clone();
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
pub(crate) fn probe_addrs(bind: &str) -> Option<Vec<std::net::IpAddr>> {
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
pub(crate) async fn spawn_ssh(
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
        // 先看子进程是否已退出：端口可能被其他进程抢占，不能把“别人的监听”当成功。
        match child.try_wait() {
            Ok(Some(status)) => return Err(ssh_exit_error(status, &stderr_buf)),
            Ok(None) => {}
            Err(e) => {
                return Err(AppError::StartFailed {
                    reason: format!("等待 ssh 进程失败：{e}"),
                })
            }
        }

        if is_listening(bind, local_port).await {
            // 监听出现后再确认 ssh 仍存活；100ms 足以过滤“端口被抢占、ssh 随即退出”的假成功。
            tokio::time::sleep(Duration::from_millis(100)).await;
            match child.try_wait() {
                Ok(None) => {
                    tracing::info!("本地端口 {bind}:{local_port} 已监听 → 隧道就绪");
                    return Ok((child, stderr_buf));
                }
                Ok(Some(status)) => return Err(ssh_exit_error(status, &stderr_buf)),
                Err(e) => {
                    return Err(AppError::StartFailed {
                        reason: format!("等待 ssh 进程失败：{e}"),
                    })
                }
            }
        }

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
}
