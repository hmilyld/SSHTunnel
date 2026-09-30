// Windows 下隐藏控制台窗口；其余平台不受影响
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ssh_tunnel_manager_lib::run();
}
