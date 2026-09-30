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

/// 把 ssh 的退出状态与 stderr 归纳成人话
fn explain_failure(status: std::process::ExitStatus, stderr: &str) -> String {
    let stderr = stderr.trim();
    let last_line = stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with("Warning:"))
        .unwrap_or("")
        .trim()
        .to_string();

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

/// 拉起一个 `ssh -L` 进程并完成 800ms 检活。
///
/// 返回仍存活的 [`tokio::process::Child`] 与其 stderr 缓冲（由回收任务接管 wait）。
async fn spawn_ssh(spec: &str, host: &str) -> Result<(tokio::process::Child, Arc<Mutex<String>>), AppError> {
    let mut cmd = tokio::process::Command::new("ssh");
    cmd.args([
        "-N",
        "-o",
        "ExitOnForwardFailure=yes",
        "-o",
        "ServerAliveInterval=30",
        "-o",
        "ServerAliveCountMax=3",
        "-L",
        spec,
        host,
    ])
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::piped())
    // 句柄被丢弃时不杀进程：隧道归用户手动管理
    .kill_on_drop(false);

    // Windows：不弹控制台黑窗
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

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

    // ===== 启动后等待约 800ms 检查进程是否存活 =====
    tokio::time::sleep(Duration::from_millis(800)).await;
    match child.try_wait() {
        Ok(Some(status)) => {
            let detail = stderr_buf
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            tracing::warn!("ssh 启动后立即退出：{status}；stderr: {detail}");
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
pub async fn start_tunnel(
    app: &AppHandle,
    state: &AppState,
    req: StartRequest,
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

    let (child, stderr_buf) = spawn_ssh(&spec, req.host.trim()).await?;
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

/// 用保存的记录重新启动隧道（复用配置，ID/创建时间不变，仅更新 PID）
pub async fn restart_tunnel(
    app: &AppHandle,
    state: &AppState,
    id: &str,
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

    let (child, stderr_buf) = spawn_ssh(&spec, &record.host).await?;
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
