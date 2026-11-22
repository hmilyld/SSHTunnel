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

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::error::AppError;
use crate::process;
use crate::ssh_start::{kill_unstarted, spawn_ssh};
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
        // 先快照记录并释放锁，再查进程：is_ssh_process 在 Windows 上会启动 tasklist，
        // 不能持锁执行，否则 3s 轮询会阻塞其他命令。
        let records = {
            let store = self.store.lock().unwrap_or_else(|e| e.into_inner());
            store.list().to_vec()
        };
        records
            .into_iter()
            .map(|record| TunnelView {
                alive: record.pid > 0 && process::is_ssh_process(record.pid),
                record,
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

/// 校验会进入 ssh argv 的目标字段，防止 `-oProxyCommand=...` 之类的参数注入。
///
/// 合法值保持原行为；只拒绝以 `-` 开头、含空白或控制字符的输入。
fn validate_token(value: &str, label: &str) -> Result<(), AppError> {
    let v = value.trim();
    if v.starts_with('-') {
        return Err(AppError::Validation(format!("{label}不能以 - 开头")));
    }
    if v.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(AppError::Validation(format!(
            "{label}不能包含空白或控制字符"
        )));
    }
    Ok(())
}

impl StartRequest {
    fn validate(&self) -> Result<(), AppError> {
        if self.host.trim().is_empty() {
            return Err(AppError::Validation("请选择 SSH Host".into()));
        }
        validate_token(&self.host, "SSH Host")?;
        if self.remote_host.trim().is_empty() {
            return Err(AppError::Validation(
                "远程主机不能为空（默认 localhost）".into(),
            ));
        }
        validate_token(&self.remote_host, "远程主机")?;
        if self.bind.trim().is_empty() {
            return Err(AppError::Validation(
                "本地绑定地址不能为空（默认 127.0.0.1）".into(),
            ));
        }
        validate_token(&self.bind, "本地绑定地址")?;
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

/// 当前本地时间 `YYYY-MM-DD HH:MM:SS`（与历史 tunnels.json 格式保持一致）。
fn now_timestamp() -> String {
    jiff::Zoned::now().strftime("%Y-%m-%d %H:%M:%S").to_string()
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
    local_port_taken_in(store.list(), exclude_id, bind, port)
}

/// [`local_port_taken`] 的快照版本：调用方先在锁内克隆记录、释放锁，再做进程查询。
fn local_port_taken_in(records: &[TunnelRecord], exclude_id: &str, bind: &str, port: u16) -> bool {
    records.iter().any(|r| {
        r.id != exclude_id
            && r.bind == bind
            && r.local_port == port
            && r.pid > 0
            && process::is_ssh_process(r.pid)
    })
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
        let stderr = stderr.lock().unwrap_or_else(|e| e.into_inner()).clone();
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

    // 预检：同一 bind:local_port 已有运行中的转发，提前给出友好错误。
    // 先克隆记录再释放锁，进程查询不会阻塞其他命令。
    let records = {
        let store = state.store.lock().unwrap_or_else(|e| e.into_inner());
        store.list().to_vec()
    };
    if local_port_taken_in(&records, "", &req.bind, req.local_port as u16) {
        return Err(AppError::PortInUse(format!(
            "本地端口 {}:{} 已被占用 — 已有正在运行的相同转发",
            req.bind, req.local_port
        )));
    }

    let (mut child, stderr_buf) = spawn_ssh(
        &spec,
        req.host.trim(),
        password.as_deref(),
        &req.bind,
        req.local_port as u16,
    )
    .await?;
    let pid = child.id().ok_or_else(|| AppError::StartFailed {
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
            created_at: now_timestamp(),
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
        let backup = store.list().to_vec();
        if let Some(r) = store.get_mut(id) {
            r.pid = 0; // 记录保留，PID 清零表示“无运行进程”
        }
        store.save_or_restore(backup)?;
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
    let (record, records) = {
        let store = state.store.lock().unwrap_or_else(|e| e.into_inner());
        let record = store
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(id.to_string()))?;
        (record, store.list().to_vec())
    };

    // 已保存记录同样可能来自旧版本/手工编辑，启动前再校验一次
    validate_token(&record.host, "SSH Host")?;
    validate_token(&record.bind, "本地绑定地址")?;
    validate_token(&record.remote_host, "远程主机")?;

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
    if local_port_taken_in(&records, &record.id, &record.bind, record.local_port) {
        return Err(AppError::PortInUse(format!(
            "本地端口 {}:{} 已被占用 — 已有另一条正在运行的转发使用它",
            record.bind, record.local_port
        )));
    }

    let (mut child, stderr_buf) = spawn_ssh(
        &spec,
        &record.host,
        password.as_deref(),
        &record.bind,
        record.local_port,
    )
    .await?;
    let pid = child.id().ok_or_else(|| AppError::StartFailed {
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
    let backup = store.list().to_vec();
    if store.remove(id).is_none() {
        return Err(AppError::NotFound(id.to_string()));
    }
    store.save_or_restore(backup)?;
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

    let backup = store.list().to_vec();
    let Some(r) = store.get_mut(id) else {
        return Err(AppError::NotFound(id.to_string()));
    };
    r.host = req.host.trim().to_string();
    r.bind = req.bind.trim().to_string();
    r.local_port = req.local_port as u16;
    r.remote_host = req.remote_host.trim().to_string();
    r.remote_port = req.remote_port as u16;
    r.pid = 0; // 保持「已停止」状态
    store.save_or_restore(backup)?;
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
    use crate::ssh_failure::{is_password_required, last_meaningful_line};
    use crate::ssh_start::{password_probe_args, probe_addrs};

    #[test]
    fn detects_password_required_only_when_password_offered() {
        // 服务器接受密码认证 → 前端需要弹密码框
        assert!(is_password_required(
            "root@host's password: \nPermission denied (publickey,password)."
        ));
        // 大小写不敏感
        assert!(is_password_required(
            "Permission Denied (Publickey,Password)."
        ));
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

    fn valid_request() -> StartRequest {
        StartRequest {
            host: "dev".into(),
            bind: "127.0.0.1".into(),
            local_port: 4096,
            remote_host: "localhost".into(),
            remote_port: 4096,
        }
    }

    #[test]
    fn validate_rejects_blank_fields() {
        let mut req = valid_request();
        req.host = "   ".into();
        assert!(req
            .validate()
            .unwrap_err()
            .to_string()
            .contains("请选择 SSH Host"));

        let mut req = valid_request();
        req.remote_host = String::new();
        assert!(req
            .validate()
            .unwrap_err()
            .to_string()
            .contains("远程主机不能为空"));

        let mut req = valid_request();
        req.bind = String::new();
        assert!(req
            .validate()
            .unwrap_err()
            .to_string()
            .contains("本地绑定地址不能为空"));
    }

    #[test]
    fn validate_checks_port_ranges() {
        let mut req = valid_request();
        req.local_port = 0;
        assert!(req
            .validate()
            .unwrap_err()
            .to_string()
            .contains("本地端口 0 无效"));

        let mut req = valid_request();
        req.local_port = 65536;
        assert!(req
            .validate()
            .unwrap_err()
            .to_string()
            .contains("本地端口 65536 无效"));

        let mut req = valid_request();
        req.remote_port = 0;
        assert!(req
            .validate()
            .unwrap_err()
            .to_string()
            .contains("远程端口 0 无效"));

        let mut req = valid_request();
        req.remote_port = 65536;
        assert!(req
            .validate()
            .unwrap_err()
            .to_string()
            .contains("远程端口 65536 无效"));
    }

    #[test]
    fn validate_accepts_boundary_ports() {
        let mut req = valid_request();
        req.local_port = 1;
        req.remote_port = 65535;
        assert!(req.validate().is_ok());
    }

    #[test]
    fn validate_rejects_ssh_option_injection_and_control_chars() {
        let mut req = valid_request();
        req.host = "-oProxyCommand=calc".into();
        assert!(req
            .validate()
            .unwrap_err()
            .to_string()
            .contains("不能以 - 开头"));

        let mut req = valid_request();
        req.host = "dev host".into();
        assert!(req
            .validate()
            .unwrap_err()
            .to_string()
            .contains("不能包含空白或控制字符"));

        let mut req = valid_request();
        req.remote_host = "local\nhost".into();
        assert!(req
            .validate()
            .unwrap_err()
            .to_string()
            .contains("不能包含空白或控制字符"));

        let mut req = valid_request();
        req.bind = "-v".into();
        assert!(req
            .validate()
            .unwrap_err()
            .to_string()
            .contains("不能以 - 开头"));

        let mut req = valid_request();
        req.bind = "::1".into();
        assert!(req.validate().is_ok());
    }

    #[test]
    fn short_id_is_eight_hex_chars() {
        for _ in 0..32 {
            let id = short_id();
            assert_eq!(id.len(), 8);
            assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn timestamp_has_expected_shape() {
        let ts = now_timestamp();
        assert_eq!(ts.len(), 19);
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], " ");
        assert_eq!(&ts[13..14], ":");
        assert_eq!(&ts[16..17], ":");
        assert!(ts.chars().filter(|c| c.is_ascii_digit()).count() >= 14);
    }
}
