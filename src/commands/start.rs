use crate::errors::Result;
use crate::runtime::manager::RUNTIME_MANAGER;
use crate::container::Container;
use log::info;
use std::fs;
use std::path::Path;
use oci::Spec;

pub struct StartCommand {
    pub id: String,
}

impl StartCommand {
    pub fn new(id: String) -> Self {
        Self { id }
    }
}

impl super::Command for StartCommand {
    fn execute(&self) -> Result<()> {
        info!("启动容器: {}", self.id);

        // 检查容器状态文件是否存在
        let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let state_file = format!("{}/.fire/{}/state.json", home_dir, self.id);
        if !std::path::Path::new(&state_file).exists() {
            return Err(crate::errors::FireError::Generic(format!(
                "容器 {} 不存在",
                self.id
            )));
        }

        // 读取容器状态
        let state_content = fs::read_to_string(&state_file)?;
        let state: oci::State = serde_json::from_str(&state_content)?;

        // 检查容器当前状态
        if state.status != "created" {
            return Err(crate::errors::FireError::Generic(format!(
                "容器 {} 不在创建状态，当前状态: {}",
                self.id, state.status
            )));
        }

        // 检查容器是否已经在全局管理器中
        {
            let manager = RUNTIME_MANAGER.lock().unwrap();
            if manager.get_container(&self.id).is_none() {
                // 如果不存在，从状态文件重新创建
                drop(manager);
                
                // 从 bundle 重新读取 OCI 配置
                let config_path = Path::new(&state.bundle).join("config.json");
                if !config_path.exists() {
                    return Err(crate::errors::FireError::Generic(format!(
                        "配置文件不存在: {}",
                        config_path.display()
                    )));
                }

                let spec = Spec::load(config_path.to_str().unwrap())
                    .map_err(|e| crate::errors::FireError::Generic(format!(
                        "无法读取OCI配置文件: {:?}",
                        e
                    )))?;

                // 重新创建容器实例
                let container = Container::new(self.id.clone(), spec, state.bundle.clone())?;
                RUNTIME_MANAGER.lock().unwrap().create_container(self.id.clone(), container)?;
            }
        }

        // 启动容器
        RUNTIME_MANAGER.lock().unwrap().start_container(&self.id)?;

        // 获取容器信息以更新状态
        let pid = {
            let manager = RUNTIME_MANAGER.lock().unwrap();
            let container = manager.get_container(&self.id)
                .ok_or_else(|| crate::errors::FireError::Generic(
                    format!("容器 {} 未找到", self.id)
                ))?;
            container.get_main_process_pid().unwrap_or(0)
        };

        // 更新容器状态为running
        let new_state = oci::State {
            version: state.version,
            id: state.id,
            status: "running".to_string(),
            pid,
            bundle: state.bundle,
            annotations: state.annotations,
        };

        // 保存新状态
        let new_state_json = new_state
            .to_string()
            .map_err(|e| crate::errors::FireError::Generic(format!("状态序列化失败: {:?}", e)))?;
        fs::write(&state_file, new_state_json)?;

        info!("容器 {} 启动成功", self.id);
        Ok(())
    }
}
