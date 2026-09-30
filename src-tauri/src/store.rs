//! 隧道记录的持久化（JSON 文件，位于系统应用配置目录）。
//!
//! 路径：
//! - Windows: `%APPDATA%\ssh-tunnel-manager\tunnels.json`
//! - macOS:   `~/Library/Application Support/ssh-tunnel-manager/tunnels.json`
//!
//! 写入采用 “临时文件 + rename” 的原子替换，避免进程被杀时写坏数据；
//! 文件损坏时自动备份为 `tunnels.json.bak` 后从空列表重建。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// 一条隧道记录（与需求文档中的 JSON 结构一致，使用 snake_case 字段）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelRecord {
    /// 8 位短 ID
    pub id: String,
    /// `~/.ssh/config` 中的 Host 别名
    pub host: String,
    /// 本地绑定地址，如 `127.0.0.1`
    pub bind: String,
    pub local_port: u16,
    /// 远程目标主机（被转发方视角的地址）
    pub remote_host: String,
    pub remote_port: u16,
    /// ssh 进程 PID
    pub pid: u32,
    /// 创建时间 `YYYY-MM-DD HH:MM:SS`
    pub created_at: String,
}

/// 返回给前端的视图：记录 + 根据 PID 实时计算的存活状态
#[derive(Debug, Clone, Serialize)]
pub struct TunnelView {
    #[serde(flatten)]
    pub record: TunnelRecord,
    /// 进程是否存活（运行中 / 已停止）
    pub alive: bool,
}

/// 内存中的记录集合 + 落盘
#[derive(Debug)]
pub struct Store {
    path: PathBuf,
    records: Vec<TunnelRecord>,
}

impl Store {
    /// 应用数据目录（由 `dirs` crate 按平台解析）
    pub fn data_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("ssh-tunnel-manager")
    }

    pub fn file_path() -> PathBuf {
        Self::data_dir().join("tunnels.json")
    }

    /// 启动时加载；任何异常都降级为空列表并记日志，绝不阻塞应用启动
    pub fn load() -> Self {
        let path = Self::file_path();
        let mut store = Self {
            path,
            records: Vec::new(),
        };
        if !store.path.exists() {
            return store;
        }
        match std::fs::read_to_string(&store.path) {
            Ok(text) => match serde_json::from_str::<Vec<TunnelRecord>>(&text) {
                Ok(records) => {
                    tracing::info!("已加载 {} 条隧道记录", records.len());
                    store.records = records;
                }
                Err(e) => {
                    let backup = store.path.with_extension("json.bak");
                    tracing::error!("{} 解析失败（{e}），已备份为 {}", store.path.display(), backup.display());
                    let _ = std::fs::rename(&store.path, &backup);
                }
            },
            Err(e) => tracing::error!("读取 {} 失败：{e}", store.path.display()),
        }
        store
    }

    /// 原子写回磁盘
    pub fn save(&self) -> Result<(), AppError> {
        let dir = self
            .path
            .parent()
            .ok_or_else(|| AppError::Store("无法确定数据目录".into()))?;
        std::fs::create_dir_all(dir).map_err(|e| AppError::Store(format!("{}: {e}", dir.display())))?;

        let json =
            serde_json::to_string_pretty(&self.records).map_err(|e| AppError::Store(e.to_string()))?;

        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| AppError::Store(format!("{}: {e}", tmp.display())))?;
        // Windows 上 rename 会覆盖目标文件
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| AppError::Store(format!("{}: {e}", self.path.display())))?;
        Ok(())
    }

    pub fn list(&self) -> &[TunnelRecord] {
        &self.records
    }

    pub fn get(&self, id: &str) -> Option<&TunnelRecord> {
        self.records.iter().find(|r| r.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut TunnelRecord> {
        self.records.iter_mut().find(|r| r.id == id)
    }

    pub fn add(&mut self, record: TunnelRecord) {
        self.records.push(record);
    }

    pub fn remove(&mut self, id: &str) -> Option<TunnelRecord> {
        let idx = self.records.iter().position(|r| r.id == id)?;
        Some(self.records.remove(idx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_roundtrips_through_json() {
        let r = TunnelRecord {
            id: "3f7a9c21".into(),
            host: "dev".into(),
            bind: "127.0.0.1".into(),
            local_port: 4096,
            remote_host: "localhost".into(),
            remote_port: 4096,
            pid: 123456,
            created_at: "2026-09-30 17:25:11".into(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"local_port\":4096"));
        let back: TunnelRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, r.id);
        assert_eq!(back.local_port, 4096);
    }
}
