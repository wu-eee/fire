use crate::container::Container;
use crate::errors::Result;
use crate::runtime::manager::RUNTIME_MANAGER;
use log::{error, info, warn};
use oci::Spec;
use std::fs;
use std::path::Path;

pub struct CreateCommand {
    pub id: String,
    pub bundle: String,
}

impl CreateCommand {
    pub fn new(id: String, bundle: Option<String>) -> Self {
        let bundle = bundle.unwrap_or_else(|| ".".to_string());
        Self { id, bundle }
    }
}

impl super::Command for CreateCommand {
    fn execute(&self) -> Result<()> {
        info!("创建容器: ID={}, Bundle={}", self.id, self.bundle);

        // 验证容器ID
        if self.id.is_empty() {
            return Err(crate::errors::FireError::InvalidSpec(
                "容器ID不能为空".to_string(),
            ));
        }

        // 验证bundle目录存在
        let bundle_path = Path::new(&self.bundle);
        if !bundle_path.exists() {
            return Err(crate::errors::FireError::InvalidSpec(format!(
                "Bundle目录不存在: {}",
                self.bundle
            )));
        }

        // 读取OCI配置文件
        let config_path = bundle_path.join("config.json");
        if !config_path.exists() {
            return Err(crate::errors::FireError::InvalidSpec(format!(
                "配置文件不存在: {}",
                config_path.display()
            )));
        }

        info!("读取OCI配置文件: {}", config_path.display());
        let spec = match Spec::load(config_path.to_str().unwrap()) {
            Ok(spec) => spec,
            Err(e) => {
                error!("无法读取OCI配置文件: {:?}", e);
                return Err(crate::errors::FireError::InvalidSpec(format!(
                    "无法读取OCI配置文件: {:?}",
                    e
                )));
            }
        };

        // 验证配置文件
        self.validate_spec(&spec)?;

        // 创建容器运行时目录
        let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let container_dir = format!("{}/.fire/{}", home_dir, self.id);
        fs::create_dir_all(&container_dir)?;
        info!("创建容器运行时目录: {}", container_dir);

        // 创建容器状态文件
        let state_file = format!("{}/state.json", container_dir);
        let state = oci::State {
            version: "1.0.0".to_string(),
            id: self.id.clone(),
            status: "created".to_string(),
            pid: 0,
            bundle: fs::canonicalize(&self.bundle)?
                .to_string_lossy()
                .to_string(),
            annotations: spec.annotations.clone(),
        };

        // 保存状态文件
        match state.to_string() {
            Ok(state_json) => {
                fs::write(&state_file, state_json)?;
                info!("保存容器状态文件: {}", state_file);
            }
            Err(e) => {
                error!("无法序列化容器状态: {:?}", e);
                return Err(crate::errors::FireError::Generic(format!(
                    "无法序列化容器状态: {:?}",
                    e
                )));
            }
        }

        // 创建容器实例并添加到全局管理器
        let container = Container::new(self.id.clone(), spec, self.bundle.clone())?;
        RUNTIME_MANAGER.lock().unwrap().create_container(self.id.clone(), container)?;

        info!("容器 {} 创建成功", self.id);
        Ok(())
    }
}

impl CreateCommand {
    fn validate_spec(&self, spec: &Spec) -> Result<()> {
        // 验证OCI版本
        if spec.version.is_empty() {
            warn!("OCI版本未设置，使用默认版本");
        }

        // 验证进程配置
        if spec.process.args.is_empty() {
            return Err(crate::errors::FireError::InvalidSpec(
                "进程参数不能为空".to_string(),
            ));
        }

        // 验证根文件系统
        if spec.root.path.is_empty() {
            return Err(crate::errors::FireError::InvalidSpec(
                "根文件系统路径不能为空".to_string(),
            ));
        }

        // 验证根文件系统是否存在
        let rootfs_path = Path::new(&self.bundle).join(&spec.root.path);
        if !rootfs_path.exists() {
            return Err(crate::errors::FireError::InvalidSpec(format!(
                "根文件系统不存在: {}",
                rootfs_path.display()
            )));
        }

        info!("OCI配置验证通过");
        Ok(())
    }
}
