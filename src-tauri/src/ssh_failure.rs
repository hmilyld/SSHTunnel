//! ssh 退出状态与 stderr 的归因。
//!
//! 这些函数只做纯文本分类，便于单元测试，也避免 `tunnel.rs` 继续膨胀。

use std::sync::{Arc, Mutex};

use crate::error::AppError;

/// 提取 stderr 中最后一条有意义的行（跳过 Warning）
pub(crate) fn last_meaningful_line(stderr: &str) -> String {
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
pub(crate) fn is_password_required(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    lower.contains("permission denied") && lower.contains("password")
}

/// 把 ssh 的退出状态与 stderr 归纳成人话
pub(crate) fn explain_failure(status: std::process::ExitStatus, stderr: &str) -> String {
    let stderr = stderr.trim();
    let last_line = last_meaningful_line(stderr);

    let hint = {
        let lower = stderr.to_lowercase();
        if lower.contains("address already in use")
            || lower.contains("only one usage of each socket")
        {
            "本地端口已被占用"
        } else if lower.contains("permission denied") {
            "认证失败，请确认已配置密钥登录或 ssh-agent"
        } else if lower.contains("could not resolve hostname")
            || lower.contains("name or service not known")
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

/// 把 ssh 的退出状态与 stderr 归纳为前端错误（启动成功判定路径共用）。
pub(crate) fn ssh_exit_error(
    status: std::process::ExitStatus,
    stderr_buf: &Arc<Mutex<String>>,
) -> AppError {
    let detail = stderr_buf.lock().unwrap_or_else(|e| e.into_inner()).clone();
    tracing::warn!("ssh 启动后退出：{status}；stderr: {detail}");
    // 服务器接受密码认证 → 交给前端弹密码框
    //（已提供密码仍失败 = 密码错误，同样走此路径让用户重新输入）
    if is_password_required(&detail) {
        AppError::PasswordRequired(last_meaningful_line(&detail))
    } else {
        AppError::StartFailed {
            reason: explain_failure(status, &detail),
        }
    }
}
