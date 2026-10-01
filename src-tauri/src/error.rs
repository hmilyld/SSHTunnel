//! 应用级错误类型。
//!
//! 约定：所有命令（`#[tauri::command]`）返回 `Result<T, AppError>`，
//! `AppError` 被序列化为**字符串**交给前端，前端可直接 toast 展示，
//! 因此每个变体的 `#[error("...")]` 都写成面向用户的中文话术。

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// 表单参数校验失败（端口范围、必填项等）
    #[error("参数校验失败：{0}")]
    Validation(String),

    /// 系统中找不到 ssh 可执行文件
    #[error(
        "未找到系统 ssh 命令。Windows 请安装 OpenSSH 客户端：powershell -c Add-WindowsCapability -Online -Name OpenSSH.Client~~~~0.0.1.0"
    )]
    SshNotFound,

    /// 服务器要求密码认证（前缀供前端识别并弹出密码框；内容为服务器原始提示）
    #[error("PASSWORD_REQUIRED::{0}")]
    PasswordRequired(String),

    /// ssh 进程启动后 800ms 内退出（端口占用、认证失败、连接被拒等）
    #[error("SSH 隧道启动失败：{reason}")]
    StartFailed { reason: String },

    /// 本地绑定地址+端口已存在运行中的转发
    #[error("本地端口 {0} 已被占用，或已存在相同的转发")]
    PortInUse(String),

    /// 按 id 找不到隧道记录
    #[error("未找到 id 为 {0} 的隧道记录")]
    NotFound(String),

    /// tunnels.json 读写失败
    #[error("保存隧道记录失败：{0}")]
    Store(String),

    /// 进程检测/终止失败
    #[error("进程操作失败：{0}")]
    Process(String),

    /// ~/.ssh/config 读取失败
    #[error("读取 SSH 配置失败：{0}")]
    SshConfig(String),
}

/// Tauri 要求命令错误类型实现 Serialize；这里直接序列化为错误文案。
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
