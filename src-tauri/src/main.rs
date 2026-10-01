// Windows 下隐藏控制台窗口；其余平台不受影响
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // ===== SSH_ASKPASS 辅助模式 =====
    //
    // 启动隧道且目标需要密码认证时，我们会给 ssh 注入：
    //   SSH_ASKPASS       = 本可执行文件
    //   SSH_ASKPASS_REQUIRE = force
    //   STM_ASKPASS_PWD   = 用户在 GUI 中输入的密码（仅驻留内存/子进程环境，不落盘）
    // ssh 在需要输入密码/口令时会以 `<SSH_ASKPASS> "<提示语>"` 拉起本程序，
    // 此处直接把密码写到 stdout 交给 ssh 后立即退出——不启动 GUI、不写日志。
    let mut args = std::env::args_os();
    args.next(); // 可执行文件自身
    if args.next().is_some() && std::env::var_os("SSH_ASKPASS").is_some() {
        use std::io::Write;
        let pwd = std::env::var("STM_ASKPASS_PWD").unwrap_or_default();
        let mut out = std::io::stdout();
        let _ = out.write_all(pwd.as_bytes());
        let _ = out.write_all(b"\n");
        let _ = out.flush();
        std::process::exit(0);
    }

    ssh_tunnel_manager_lib::run();
}
