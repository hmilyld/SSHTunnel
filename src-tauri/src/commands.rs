//! Tauri 命令层：薄封装，参数/返回值直接对接前端。
//!
//! 错误统一为 `AppError`（序列化为字符串），前端 toast 直接展示。

use tauri::{AppHandle, State};

use crate::error::AppError;
use crate::ssh_config;
use crate::store::TunnelView;
use crate::tunnel::{self, AppState, StartRequest};

/// 返回 `~/.ssh/config` 中可作为 `-L` 目标的 Host 别名列表
#[tauri::command]
pub fn list_hosts() -> Result<Vec<ssh_config::SshHost>, AppError> {
    let path = ssh_config::default_config_path();
    if !path.exists() {
        tracing::warn!("未找到 SSH 配置文件 {}", path.display());
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| AppError::SshConfig(format!("{}: {e}", path.display())))?;
    let hosts = ssh_config::parse(&content);
    tracing::debug!("解析到 {} 个 SSH Host", hosts.len());
    Ok(hosts)
}

/// 拉取全部隧道（附带按 PID 计算的实时状态），前端每 3s 轮询
#[tauri::command]
pub async fn list_tunnels(state: State<'_, AppState>) -> Result<Vec<TunnelView>, AppError> {
    Ok(state.list_views())
}

/// 新建并启动一条转发。
/// `password` 为空时先以 BatchMode 探测；服务器要求密码时返回
/// `PASSWORD_REQUIRED::…`，前端据此弹出密码框后带密码重试。
#[tauri::command]
pub async fn start_tunnel(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartRequest,
    password: Option<String>,
) -> Result<TunnelView, AppError> {
    tunnel::start_tunnel(&app, state.inner(), request, password).await
}

/// 关闭转发（终止 ssh 进程但**保留记录**，返回最新列表）
#[tauri::command]
pub async fn stop_tunnel(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<TunnelView>, AppError> {
    tunnel::stop_tunnel(state.inner(), &id).await
}

/// 用已保存的记录重新启动隧道（复用配置，仅更新 PID；密码语义同 start_tunnel）
#[tauri::command]
pub async fn restart_tunnel(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    password: Option<String>,
) -> Result<Vec<TunnelView>, AppError> {
    tunnel::restart_tunnel(&app, state.inner(), &id, password).await
}

/// 显式删除记录（仅允许删除已停止的记录）
#[tauri::command]
pub fn remove_tunnel(state: State<'_, AppState>, id: String) -> Result<Vec<TunnelView>, AppError> {
    tunnel::remove_record(state.inner(), &id)
}

/// 修改已停止记录的配置（不涉及进程，ID/创建时间不变）
#[tauri::command]
pub fn update_tunnel(
    state: State<'_, AppState>,
    id: String,
    request: StartRequest,
) -> Result<Vec<TunnelView>, AppError> {
    tunnel::update_record(state.inner(), &id, request)
}

/// ~/.ssh/config 路径（用于界面提示）
#[tauri::command]
pub fn ssh_config_path() -> String {
    ssh_config::default_config_path().display().to_string()
}

/// 数据目录（tunnels.json / app.log 所在目录，供界面做按钮提示）
#[tauri::command]
pub fn data_dir() -> String {
    crate::store::Store::data_dir().display().to_string()
}

/// 在系统文件管理器中打开数据目录（资源管理器 / 访达）。
///
/// 只打开**后端自己解析的**数据目录，不接受前端传入路径——
/// 这样即使 WebView 侧被注入脚本，也无法借此打开任意路径。
#[tauri::command]
pub fn reveal_data_dir() -> Result<(), AppError> {
    let dir = crate::store::Store::data_dir();
    // 目录正常由 logging::init 在启动时创建；这里再兜一次，避免早期手动清理后点击无反应
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Err(AppError::Process(format!(
            "无法创建数据目录 {}：{e}",
            dir.display()
        )));
    }

    #[cfg(windows)]
    let program = {
        // explorer.exe 打开目录；CREATE_NO_WINDOW 避免闪一下控制台黑窗
        "explorer"
    };
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let program = "xdg-open";

    let mut cmd = std::process::Command::new(program);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }

    cmd.arg(&dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| AppError::Process(format!("无法打开数据目录 {}：{e}", dir.display())))
}
