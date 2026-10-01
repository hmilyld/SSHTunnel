//! 隧道生命周期核心：启动 `ssh -L`、状态查询、停止与重启。
//!
//! 生命周期设计（记录持久化，除非用户明确删除）：
//! - **新建**：校验参数 → 启动 ssh → 800ms 检活 → 记录落盘（此后一直保留）
//! - **关闭**：终止进程（优雅→强制）→ **保留记录**，PID 清零、状态变为「已停止」
//! - **启动**（复用）：用保存的记录重新拉起 ssh，更新 PID；记录与 ID 不变
//! - **删除**：仅在记录已停止时由用户显式移除
//!
//! 启动细节：
//! 1. `tokio::process` 执行
//!    `ssh -N -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 -o ServerAliveCountMax=3 -L <spec> <host>`
//!    - Windows 下附加 `CREATE_NO_WINDOW`，不弹控制台黑窗
//!    - stderr 用后台任务持续读取（失败归因 + 防止管道写满阻塞 ssh）
//! 2. 等待 800ms 后 `try_wait()`：已退出则解析 stderr 给出友好失败原因
//! 3. 存活 → 启动“回收任务”（ssh 退出时向前端广播 `tunnel-exited` 即时刷新）

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

    let detail = if last_line.is_empty() {
        format!("ssh 进程已退出（{status}）")
    } else if hint.is_empty() {
        last_line
    } else {
        format!("{hint}（{last_line}）")
    };
    detail
}

/// 生成 8 位短 ID（截断 UUID v4）
fn short_id() -> String {
    let full = uuid::Uuid::new_v4().simple().to_string();
    full[..8].to_string()
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
                    let _ = child.start_kill();
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

/// 拉起一个 `ssh -L` 进程并完成 800ms 检活。
///
/// - `password = None`：追加 `-o BatchMode=yes`（禁止任何交互，认证失败快速返回，
///   由错误分类决定是否需要弹密码框）
/// - `password = Some`：追加 `-o NumberOfPasswordPrompts=1` 并通过 `SSH_ASKPASS`
///   把密码交给 ssh（askpass 即本程序的辅助模式）
///
/// 返回仍存活的 [`tokio::process::Child`] 与其 stderr 缓冲（由回收任务接管 wait）。
async fn spawn_ssh(
    spec: &str,
    host: &str,
    password: Option<&str>,
) -> Result<(tokio::process::Child, Arc<Mutex<String>>), AppError> {
    // ===== 密码模式：先预检认证，密码错误在此阶段直接返回（不会出现“成功后立刻失败”） =====
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

    // ===== 启动后等待稳定性窗口再判定（密码模式含 askpass 往返，窗口放宽到 2s） =====
    let settle = if password.is_some() {
        Duration::from_millis(2000)
    } else {
        Duration::from_millis(800)
    };
    tokio::time::sleep(settle).await;
    match child.try_wait() {
        Ok(Some(status)) => {
            let detail = stderr_buf
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            tracing::warn!("ssh 启动后立即退出：{status}；stderr: {detail}");
            // 分类一：服务器接受密码认证 → 交给前端弹密码框
            //（已提供密码仍失败 = 密码错误，同样走此路径让用户重新输入）
            if is_password_required(&detail) {
                return Err(AppError::PasswordRequired(last_meaningful_line(&detail)));
            }
            Err(AppError::StartFailed {
                reason: explain_failure(status, &detail),
            })
        }
        Ok(None) => Ok((child, stderr_buf)), // 存活
        Err(e) => Err(AppError::StartFailed {
            reason: format!("等待 ssh 进程失败：{e}"),
        }),
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
        let duplicated = store.list().iter().any(|r| {
            r.bind == req.bind
                && r.local_port == req.local_port as u16
                && r.pid > 0
                && process::is_ssh_process(r.pid)
        });
        if duplicated {
            return Err(AppError::PortInUse(format!("{}:{}", req.bind, req.local_port)));
        }
    }

    let (child, stderr_buf) = spawn_ssh(&spec, req.host.trim(), password.as_deref()).await?;
    let pid = child
        .id()
        .ok_or_else(|| AppError::StartFailed {
            reason: "无法获取 ssh 进程 PID".into(),
        })?;

    // ===== 持久化（关闭后也不会丢失，除非用户显式删除） =====
    let record = TunnelRecord {
        id: short_id(),
        host: req.host.trim().to_string(),
        bind: req.bind.trim().to_string(),
        local_port: req.local_port as u16,
        remote_host: req.remote_host.trim().to_string(),
        remote_port: req.remote_port as u16,
        pid,
        created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    };
    {
        let mut store = state.store.lock().unwrap_or_else(|e| e.into_inner());
        store.add(record.clone());
        store.save()?;
    }

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
        let duplicated = store.list().iter().any(|r| {
            r.id != record.id
                && r.bind == record.bind
                && r.local_port == record.local_port
                && r.pid > 0
                && process::is_ssh_process(r.pid)
        });
        if duplicated {
            return Err(AppError::PortInUse(format!(
                "{}:{}",
                record.bind, record.local_port
            )));
        }
    }

    let (child, stderr_buf) = spawn_ssh(&spec, &record.host, password.as_deref()).await?;
    let pid = child
        .id()
        .ok_or_else(|| AppError::StartFailed {
            reason: "无法获取 ssh 进程 PID".into(),
        })?;

    {
        let mut store = state.store.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(r) = store.get_mut(id) {
            r.pid = pid;
        }
        store.save()?;
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
}