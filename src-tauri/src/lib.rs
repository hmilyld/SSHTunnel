//! 库入口：组装 Tauri 应用。
//!
//! 模块划分（对应需求“代码模块化”）：
//! - [`ssh_config`]：`~/.ssh/config` 解析
//! - [`tunnel`]：隧道生命周期（启动/停止/状态）与全局状态
//! - [`store`]：JSON 持久化
//! - [`process`]：跨平台进程检测与终止
//! - [`tray`]：系统托盘
//! - [`commands`]：前端命令层
//! - [`logging`] / [`error`]：日志与错误

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod error;
mod logging;
mod process;
mod ssh_config;
mod store;
mod tray;
mod tunnel;

use tauri::Manager;

pub use error::AppError;
pub use store::{Store, TunnelRecord, TunnelView};
pub use tunnel::AppState;

pub fn run() {
    logging::init();

    tauri::Builder::default()
        .setup(|app| {
            // 启动时读取持久化记录（进程存活状态在前端首次 list_tunnels 时计算）
            let store = Store::load();
            app.manage(AppState::new(store));
            tray::init(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_hosts,
            commands::list_tunnels,
            commands::start_tunnel,
            commands::stop_tunnel,
            commands::restart_tunnel,
            commands::remove_tunnel,
            commands::update_tunnel,
            commands::ssh_config_path,
            commands::data_path,
        ])
        // 关闭主窗口 = 隐藏到托盘，应用继续在后台运行
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                tracing::debug!("窗口关闭请求 -> 隐藏到托盘");
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("运行 SSH 隧道管理器失败");
}
