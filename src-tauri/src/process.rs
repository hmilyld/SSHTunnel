//! 跨平台进程检测与终止。
//!
//! - **Windows**：`tasklist` 查询存活/PID 复用检测，`taskkill`（先优雅后 `/F`）终止；
//!   两者都是 Win10+ 自带工具，且用 `CREATE_NO_WINDOW` 避免闪黑窗。
//! - **macOS/Linux**：`libc::kill(pid, 0)` 探活、`kill(pid, SIGTERM/SIGKILL)` 终止；
//!   进程名通过 `/proc/<pid>/comm`（Linux）或 `ps -o comm=`（macOS）查询。
//!
//! 终止前强制校验进程名是 `ssh`，防止 PID 被系统复用后误杀无关进程。

use std::process::{Command, Output, Stdio};
use std::time::Duration;

use crate::error::AppError;

/// Windows：`creation_flags` 需要该 trait
#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Windows：不为子进程创建控制台窗口
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 执行一个系统查询命令并捕获输出（隐藏窗口）
fn run_hidden(program: &str, args: &[&str]) -> Option<Output> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.output().ok()
}

/// 查询 PID 对应的进程名；进程不存在或无权访问时返回 `None`
#[cfg(windows)]
pub fn process_name(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    let pid_s = pid.to_string();
    // 无匹配时 tasklist 输出本地化的 INFO 文本（不含 pid 列），因此按“第 2 列 == pid”匹配是可靠的
    let out = run_hidden("tasklist", &["/NH", "/FI", &format!("PID eq {pid_s}")])?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 2 && cols[1] == pid_s {
            return Some(cols[0].to_string());
        }
    }
    None
}

/// 查询 PID 对应的进程名
#[cfg(unix)]
pub fn process_name(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    let pid_s = pid.to_string();

    // Linux：/proc/<pid>/comm
    let comm = std::path::PathBuf::from(format!("/proc/{pid_s}/comm"));
    if comm.exists() {
        if let Ok(s) = std::fs::read_to_string(&comm) {
            let s = s.trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }

    // macOS / BSD：ps -p <pid> -o comm=（输出为命令路径，如 /usr/bin/ssh）
    let out = run_hidden("ps", &["-p", &pid_s, "-o", "comm="])?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// PID 是否存活
pub fn is_pid_alive(pid: u32) -> bool {
    process_name(pid).is_some()
}

/// PID 对应的是否是 ssh 进程（`ssh` / `ssh.exe`），用于抵御 PID 复用
pub fn is_ssh_process(pid: u32) -> bool {
    let Some(name) = process_name(pid) else {
        return false;
    };
    let base = std::path::Path::new(&name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(name.as_str())
        .to_ascii_lowercase();
    // 精确匹配 ssh、ssh.exe；排除 ssh-add、ssh-agent 等同前缀程序
    base == "ssh" || base == "ssh.exe"
}

/// 结束进程：`force = false` 优雅终止（Windows: taskkill / Unix: SIGTERM），
/// `force = true` 强制终止（taskkill /F · SIGKILL）
#[cfg(windows)]
pub fn terminate(pid: u32, force: bool) -> Result<(), String> {
    let mut args = vec!["/PID".to_string(), pid.to_string()];
    if force {
        args.push("/F".to_string());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_hidden("taskkill", &refs).ok_or_else(|| "无法执行 taskkill".to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let detail = if stderr.trim().is_empty() {
            String::from_utf8_lossy(&out.stdout)
        } else {
            stderr
        };
        Err(detail.trim().to_string())
    }
}

/// 结束进程（见上）
#[cfg(unix)]
pub fn terminate(pid: u32, force: bool) -> Result<(), String> {
    let sig = if force { libc::SIGKILL } else { libc::SIGTERM };
    if unsafe { libc::kill(pid as i32, sig) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().to_string())
    }
}

/// 查询正在监听指定本地端口的进程，返回 `(pid, 进程名)`。
///
/// 仅在“端口被占用”的错误路径调用（单次 PowerShell，约 0.2~0.6s），
/// 用于把报错从“端口被占用”升级为“被谁占用”。
#[cfg(windows)]
pub fn port_listener_info(port: u16) -> Option<(u32, String)> {
    // 注意：format! 的花括号需转义；命令串内不使用引号，避免参数转义问题
    let script = format!(
        "$c=Get-NetTCPConnection -LocalPort {port} -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1; \
         if($c) {{ $p=Get-Process -Id $c.OwningProcess -ErrorAction SilentlyContinue; \
         Write-Output $c.OwningProcess; Write-Output $p.ProcessName }}"
    );
    let out = run_hidden(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", &script],
    )?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let pid = lines.next()?.parse::<u32>().ok()?;
    let name = lines.next().unwrap_or("unknown").to_string();
    Some((pid, name))
}

/// 查询正在监听指定本地端口的进程，返回 `(pid, 进程名)`（lsof 优先，ss 兜底）
#[cfg(unix)]
pub fn port_listener_info(port: u16) -> Option<(u32, String)> {
    // lsof -Fpc：输出字段行 p<pid> 与 c<command>
    if let Some(out) = run_hidden(
        "lsof",
        &["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-Fpc"],
    ) {
        let text = String::from_utf8_lossy(&out.stdout);
        let mut pid: Option<u32> = None;
        let mut name: Option<String> = None;
        for line in text.lines() {
            if let Some(v) = line.strip_prefix('p') {
                pid = v.trim().parse().ok();
            } else if let Some(v) = line.strip_prefix('c') {
                name = Some(v.trim().to_string());
            }
        }
        if let (Some(p), Some(n)) = (pid, name) {
            return Some((p, n));
        }
    }

    // ss -ltnp 兜底（对非本用户进程可能看不到 pid，尽力而为）
    let out = run_hidden("ss", &["-ltnp"])?;
    let text = String::from_utf8_lossy(&out.stdout);
    let port_tok = format!(":{port}");
    for line in text.lines() {
        // 行示例：LISTEN 0 511 127.0.0.1:5173 0.0.0.0:* users:(("node",pid=4968,fd=20))
        let local_ok = line
            .split_whitespace()
            .any(|tok| tok.ends_with(&port_tok));
        if !local_ok {
            continue;
        }
        if let Some(start) = line.find("pid=") {
            let rest = &line[start + 4..];
            let pid: u32 = rest
                .split(|c: char| c == ',' || c == ')')
                .next()?
                .trim()
                .parse()
                .ok()?;
            let name = line
                .find("(\"")
                .map(|i| line[i + 2..].split('"').next().unwrap_or("unknown").to_string())
                .unwrap_or_else(|| "unknown".to_string());
            return Some((pid, name));
        }
    }
    None
}

/// 优雅终止 → 最多等待 1.8s → 仍存活则强制终止 → 再等 1.8s。
/// 调用方需先用 [`is_ssh_process`] 确认是本应用启动的 ssh 进程。
pub async fn stop_process(pid: u32) -> Result<(), AppError> {
    if !is_ssh_process(pid) {
        // 探测与调用之间进程可能刚好退出
        return if is_pid_alive(pid) {
            Err(AppError::Process(format!(
                "PID {pid} 已不是 ssh 进程（可能被系统复用），为避免误杀已中止"
            )))
        } else {
            Ok(())
        };
    }

    match terminate(pid, false) {
        Ok(()) => {
            for _ in 0..12 {
                if !is_pid_alive(pid) {
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
        }
        Err(e) => {
            // Windows 下控制台进程通常无法优雅关闭，直接进入强制终止
            tracing::debug!("PID {pid} 优雅终止未成功（{e}），改为强制终止");
        }
    }

    tracing::warn!("PID {pid} 未响应优雅终止，强制终止");
    terminate(pid, true).map_err(AppError::Process)?;
    for _ in 0..12 {
        if !is_pid_alive(pid) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    Err(AppError::Process(format!("无法终止进程 PID {pid}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 端口占用检测：本测试进程自己监听一个随机端口，应能反查出自己的 pid/名字
    #[test]
    fn identifies_port_listener_process() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().expect("addr").port();

        let info = port_listener_info(port).expect("应能查询到监听进程");
        let (pid, name) = info;
        assert_eq!(pid, std::process::id(), "监听者应是本测试进程");
        assert!(!name.is_empty());
    }
}
