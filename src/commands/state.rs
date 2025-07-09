use crate::errors::Result;
use crate::container::Container;
use log::info;
use std::fs;
use oci::Spec;

pub struct StateCommand {
    pub id: String,
}

impl StateCommand {
    pub fn new(id: String) -> Self {
        Self { id }
    }
}

impl super::Command for StateCommand {
    fn execute(&self) -> Result<()> {
        info!("获取容器状态: {}", self.id);

        let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let state_file = format!("{}/.fire/{}/state.json", home_dir, self.id);

        // 检查容器状态文件是否存在
        if !std::path::Path::new(&state_file).exists() {
            return Err(crate::errors::FireError::Generic(format!(
                "容器 {} 不存在",
                self.id
            )));
        }

        // 读取容器状态
        let state_content = fs::read_to_string(&state_file)?;
        let state: oci::State = serde_json::from_str(&state_content)?;

        // 输出基本状态信息
        println!("容器状态信息:");
        println!("  ID: {}", state.id);
        println!("  状态: {}", state.status);
        println!("  进程ID: {}", state.pid);
        println!("  Bundle路径: {}", state.bundle);
        println!("  OCI版本: {}", state.version);

        // 尝试获取namespace信息
        if let Ok(spec) = self.load_container_spec(&state.bundle) {
            if let Ok(container) = Container::new(state.id.clone(), spec, state.bundle.clone()) {
                let namespace_info = container.get_namespace_info();
                if !namespace_info.is_empty() {
                    println!("  Namespace信息:");
                    for (ns_type, info) in namespace_info {
                        println!("    {}: {}", ns_type, info);
                    }
                } else {
                    println!("  Namespace信息: 无");
                }
            }
        }

        // 输出注解信息
        if !state.annotations.is_empty() {
            println!("  注解:");
            for (key, value) in state.annotations {
                println!("    {}: {}", key, value);
            }
        }

        Ok(())
    }
}

impl StateCommand {
    fn load_container_spec(&self, bundle_path: &str) -> Result<Spec> {
        let config_path = format!("{}/config.json", bundle_path);
        
        if !std::path::Path::new(&config_path).exists() {
            return Err(crate::errors::FireError::InvalidSpec(format!(
                "配置文件不存在: {}",
                config_path
            )));
        }

        match Spec::load(&config_path) {
            Ok(spec) => Ok(spec),
            Err(e) => Err(crate::errors::FireError::InvalidSpec(format!(
                "无法读取OCI配置文件: {:?}",
                e
            ))),
        }
    }
}
