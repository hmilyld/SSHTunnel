//! 日志初始化：同时输出到 stdout（开发时可见）与 `app.log`（GUI 下可事后排查）。
//!
//! 日志文件位于应用数据目录：
//! - Windows: `%APPDATA%\ssh-tunnel-manager\app.log`
//! - macOS:   `~/Library/Application Support/ssh-tunnel-manager/app.log`
//!
//! 可用 `RUST_LOG=debug` 覆盖默认级别。

use std::fs::File;
use std::io::Write;
use std::sync::{Arc, Mutex};

use tracing_subscriber::fmt::MakeWriter;

/// 同时写 stdout 与日志文件
#[derive(Clone)]
pub struct FileAndStdout {
    file: Option<Arc<Mutex<File>>>,
}

impl<'a> MakeWriter<'a> for FileAndStdout {
    type Writer = TeeWriter;

    fn make_writer(&'a self) -> Self::Writer {
        TeeWriter {
            file: self.file.clone(),
        }
    }
}

/// 每条日志事件会被拆成多次 `write` 调用，这里直接透传，不做缓冲
pub struct TeeWriter {
    file: Option<Arc<Mutex<File>>>,
}

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stdout().write_all(buf);
        if let Some(file) = &self.file {
            if let Ok(mut file) = file.lock() {
                let _ = file.write_all(buf);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stdout().flush();
        Ok(())
    }
}

pub fn init() {
    let dir = crate::store::Store::data_dir();
    let log_path = dir.join("app.log");

    // 日志初始化早于 tunnels.json 的首次保存，因此数据目录必须由这里创建：
    // 否则全新安装的首次启动会静默拿不到 app.log。
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("无法创建数据目录 {}：{e}", dir.display());
    }

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| {
            eprintln!("无法打开日志文件 {}：{e}", log_path.display());
            e
        })
        .ok()
        .map(|f| Arc::new(Mutex::new(f)));

    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new(if cfg!(debug_assertions) { "debug" } else { "info" })
    });

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(FileAndStdout { file })
        .init();

    tracing::info!("日志初始化完成，文件：{}", log_path.display());
}
