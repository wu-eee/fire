use crate::errors::Result;
use crate::runtime::manager::RUNTIME_MANAGER;
use log::info;
use std::fs;

pub struct DeleteCommand {
    pub id: String,
    pub force: bool,
}

impl DeleteCommand {
    pub fn new(id: String, force: bool) -> Self {
        Self { id, force }
    }
}

impl super::Command for DeleteCommand {
    fn execute(&self) -> Result<()> {
        info!("删除容器: {}", self.id);

        let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let container_dir = format!("{}/.fire/{}", home_dir, self.id);
        let state_file = format!("{}/state.json", container_dir);

        // 检查容器是否存在
        if !std::path::Path::new(&state_file).exists() {
            return Err(crate::errors::FireError::Generic(format!(
                "容器 {} 不存在",
                self.id
            )));
        }

        // 读取容器状态
        let state_content = fs::read_to_string(&state_file)?;
        let state: oci::State = serde_json::from_str(&state_content)?;

        // 检查容器状态，只能删除已停止的容器
        if state.status == "running" && !self.force {
            return Err(crate::errors::FireError::Generic(format!(
                "容器 {} 正在运行，请先停止或使用 --force 参数",
                self.id
            )));
        }

        // 如果容器正在运行且使用了 force 参数，先停止容器
        if state.status == "running" && self.force {
            info!("强制停止容器 {}", self.id);
            if let Err(e) = RUNTIME_MANAGER.lock().unwrap().stop_container(&self.id) {
                info!("停止容器失败，继续删除: {}", e);
            }
        }

        // 清理容器资源
        {
            let mut manager = RUNTIME_MANAGER.lock().unwrap();
            if let Some(mut container) = manager.remove_container(&self.id) {
                info!("清理容器 {} 的资源", self.id);
                if let Err(e) = container.cleanup() {
                    info!("清理容器资源失败，继续删除: {}", e);
                }
            }
        }

        // 删除容器状态文件
        if std::path::Path::new(&state_file).exists() {
            fs::remove_file(&state_file)?;
            info!("删除容器状态文件: {}", state_file);
        }

        // 删除容器目录
        if std::path::Path::new(&container_dir).exists() {
            fs::remove_dir_all(&container_dir)?;
            info!("删除容器目录: {}", container_dir);
        }

        info!("容器 {} 删除成功", self.id);
        Ok(())
    }
}
