use crate::errors::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub state_dir: PathBuf,
    pub log_level: String,
    pub log_file: Option<PathBuf>,
    pub max_containers: usize,
    pub enable_systemd: bool,
    pub cgroup_manager: String,
    pub default_runtime: String,
    pub hooks_dir: Option<PathBuf>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        Self {
            state_dir: PathBuf::from(format!("{}/.fire", home_dir)),
            log_level: "info".to_string(),
            log_file: None,
            max_containers: 1000,
            enable_systemd: false,
            cgroup_manager: "cgroupfs".to_string(),
            default_runtime: "fire".to_string(),
            hooks_dir: None,
        }
    }
}

impl RuntimeConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_from_file(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: RuntimeConfig = serde_json::from_str(&content)?;
        Ok(config)
    }

    pub fn save_to_file(&self, path: &str) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        // 验证状态目录
        if !self.state_dir.exists() {
            std::fs::create_dir_all(&self.state_dir)?;
        }

        // 验证日志级别
        match self.log_level.as_str() {
            "trace" | "debug" | "info" | "warn" | "error" => {}
            _ => {
                return Err(crate::errors::FireError::InvalidSpec(format!(
                    "无效的日志级别: {}",
                    self.log_level
                )));
            }
        }

        // 验证cgroup管理器
        match self.cgroup_manager.as_str() {
            "cgroupfs" | "systemd" => {}
            _ => {
                return Err(crate::errors::FireError::InvalidSpec(format!(
                    "无效的cgroup管理器: {}",
                    self.cgroup_manager
                )));
            }
        }

        Ok(())
    }

    pub fn get_container_state_dir(&self, container_id: &str) -> PathBuf {
        self.state_dir.join(container_id)
    }

    pub fn get_container_state_file(&self, container_id: &str) -> PathBuf {
        self.get_container_state_dir(container_id)
            .join("state.json")
    }
}
