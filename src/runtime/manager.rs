use crate::container::Container;
use crate::errors::Result;
use std::collections::HashMap;
use std::sync::Mutex;
use log::{info, error};
use lazy_static::lazy_static;

lazy_static! {
    pub static ref RUNTIME_MANAGER: Mutex<RuntimeManager> = {
        let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let state_dir = format!("{}/.fire", home_dir);
        Mutex::new(RuntimeManager::new(state_dir))
    };
}

pub struct RuntimeManager {
    containers: HashMap<String, Container>,
    state_dir: String,
}

impl RuntimeManager {
    pub fn new(state_dir: String) -> Self {
        Self {
            containers: HashMap::new(),
            state_dir,
        }
    }

    pub fn create_container(&mut self, id: String, container: Container) -> Result<()> {
        if self.containers.contains_key(&id) {
            crate::bail!("容器 {} 已存在", id);
        }
        info!("创建容器 {}", id);
        self.containers.insert(id, container);
        Ok(())
    }

    pub fn start_container(&mut self, id: &str) -> Result<()> {
        let container = self.containers.get_mut(id)
            .ok_or_else(|| crate::errors::FireError::Generic(
                format!("容器 {} 不存在", id)
            ))?;
        
        container.start()
    }

    pub fn stop_container(&mut self, id: &str) -> Result<()> {
        let container = self.containers.get_mut(id)
            .ok_or_else(|| crate::errors::FireError::Generic(
                format!("容器 {} 不存在", id)
            ))?;
        
        container.stop()
    }

    pub fn pause_container(&mut self, id: &str) -> Result<()> {
        let container = self.containers.get_mut(id)
            .ok_or_else(|| crate::errors::FireError::Generic(
                format!("容器 {} 不存在", id)
            ))?;
        
        container.pause()
    }

    pub fn resume_container(&mut self, id: &str) -> Result<()> {
        let container = self.containers.get_mut(id)
            .ok_or_else(|| crate::errors::FireError::Generic(
                format!("容器 {} 不存在", id)
            ))?;
        
        container.resume()
    }

    pub fn kill_container(&mut self, id: &str, signal: i32) -> Result<()> {
        let container = self.containers.get(id)
            .ok_or_else(|| crate::errors::FireError::Generic(
                format!("容器 {} 不存在", id)
            ))?;
        
        if let Some(ref main_process) = container.main_process {
            main_process.kill(signal)?;
        } else {
            return Err(crate::errors::FireError::Generic(
                format!("容器 {} 没有主进程", id)
            ));
        }
        
        Ok(())
    }

    pub fn get_container(&self, id: &str) -> Option<&Container> {
        self.containers.get(id)
    }

    pub fn get_container_mut(&mut self, id: &str) -> Option<&mut Container> {
        self.containers.get_mut(id)
    }

    pub fn remove_container(&mut self, id: &str) -> Option<Container> {
        self.containers.remove(id)
    }

    pub fn list_containers(&self) -> Vec<&Container> {
        self.containers.values().collect()
    }

    pub fn cleanup_all(&mut self) -> Result<()> {
        info!("清理所有容器资源");
        
        for (id, container) in self.containers.iter_mut() {
            info!("清理容器 {} 的资源", id);
            if let Err(e) = container.cleanup() {
                error!("清理容器 {} 失败: {}", id, e);
            }
        }
        
        self.containers.clear();
        info!("所有容器资源清理完成");
        Ok(())
    }
}
