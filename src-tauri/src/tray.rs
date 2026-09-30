//! 系统托盘：图标 + 菜单（显示主窗口 / 退出）+ 左键切换窗口。
//!
//! - Windows：任务栏通知区域；右键弹菜单，左键切换主窗口显隐
//! - macOS：菜单栏；行为一致（托盘图标由 tauri.conf.json 的 bundle.icon 提供）

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::process;
use crate::tunnel::AppState;

/// 显示并聚焦主窗口
fn show_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// 左键：在“显示 / 隐藏”之间切换
fn toggle_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let visible = win.is_visible().unwrap_or(false);
        let minimized = win.is_minimized().unwrap_or(false);
        if visible && !minimized {
            let _ = win.hide();
        } else {
            show_main_window(app);
        }
    }
}

/// 「退出应用（关闭隧道）」：退出前结束所有仍在运行的 ssh 进程。
/// 先发优雅终止信号，随后立即补一次强制终止（退出流程不等待轮询，保证确定性）。
fn close_all_tunnels(app: &AppHandle) {
    let state = app.state::<AppState>();
    let records = {
        let store = state.store.lock().unwrap_or_else(|e| e.into_inner());
        store.list().to_vec()
    };
    let mut closed = 0usize;
    for r in &records {
        if process::is_ssh_process(r.pid) {
            let _ = process::terminate(r.pid, false);
            let _ = process::terminate(r.pid, true);
            closed += 1;
        }
    }
    tracing::info!("退出前关闭了 {closed} 条运行中的隧道（共 {} 条记录）", records.len());
}

/// 创建托盘图标与菜单
pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    // 默认退出不杀隧道（进程与记录都保留，下次启动自动恢复状态）
    let quit_keep = MenuItem::with_id(app, "quit", "退出应用（保留隧道）", true, None::<&str>)?;
    // 可选：退出时一并关闭所有隧道
    let quit_stop = MenuItem::with_id(app, "quit_stop", "退出应用（关闭隧道）", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &separator, &quit_keep, &quit_stop])?;

    let icon = app
        .default_window_icon()
        .cloned()
        .expect("tauri.conf.json 中未配置托盘图标");

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("SSH 隧道管理器")
        .menu(&menu)
        // 左键交给 on_tray_icon_event 做窗口切换；菜单留给右键
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => {
                tracing::info!("从托盘退出应用（保留隧道）");
                app.exit(0);
            }
            "quit_stop" => {
                tracing::info!("从托盘退出应用（关闭所有隧道）");
                close_all_tunnels(app);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    tracing::info!("系统托盘已创建");
    Ok(())
}
