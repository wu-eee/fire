use crate::container::Container;
use crate::errors::Result;
use manager::RuntimeManager;
use std::sync::{Arc, Mutex};
use log::info;

pub mod config;
pub mod hooks;
pub mod manager;

lazy_static::lazy_static! {
    static ref RUNTIME_MANAGER: Arc<Mutex<RuntimeManager>> = {
        let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let state_dir = format!("{}/.fire", home_dir);
        Arc::new(Mutex::new(RuntimeManager::new(state_dir)))
    };
}

#[derive(Debug)]
pub struct Runtime {
    // 运行时配置和状态
}

impl Runtime {
    pub fn new() -> Self {
        Self {}
    }

    pub fn create_container(&mut self, container: Container) -> Result<()> {
        let id = container.id.clone();
        let mut manager = RUNTIME_MANAGER.lock().unwrap();
        manager.create_container(id, container)
    }

    pub fn start_container(&mut self, id: &str) -> Result<()> {
        let mut manager = RUNTIME_MANAGER.lock().unwrap();
        manager.start_container(id)
    }

    pub fn stop_container(&mut self, id: &str) -> Result<()> {
        let mut manager = RUNTIME_MANAGER.lock().unwrap();
        manager.stop_container(id)
    }

    pub fn pause_container(&mut self, id: &str) -> Result<()> {
        let mut manager = RUNTIME_MANAGER.lock().unwrap();
        manager.pause_container(id)
    }

    pub fn resume_container(&mut self, id: &str) -> Result<()> {
        let mut manager = RUNTIME_MANAGER.lock().unwrap();
        manager.resume_container(id)
    }

    pub fn kill_container(&mut self, id: &str, signal: i32) -> Result<()> {
        let mut manager = RUNTIME_MANAGER.lock().unwrap();
        manager.kill_container(id, signal)
    }

    pub fn get_container(&self, id: &str) -> Option<Container> {
        let manager = RUNTIME_MANAGER.lock().unwrap();
        manager.get_container(id).cloned()
    }

    pub fn remove_container(&mut self, id: &str) -> Option<Container> {
        let mut manager = RUNTIME_MANAGER.lock().unwrap();
        manager.remove_container(id)
    }

    pub fn list_containers(&self) -> Vec<Container> {
        let manager = RUNTIME_MANAGER.lock().unwrap();
        manager.list_containers().into_iter().cloned().collect()
    }

    pub fn cleanup_all(&mut self) -> Result<()> {
        let mut manager = RUNTIME_MANAGER.lock().unwrap();
        manager.cleanup_all()
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

// 运行时初始化
pub fn init() -> Result<()> {
    info!("初始化 Fire 运行时");
    
    // 初始化 cgroups
    crate::cgroups::init();
    
    // 检查 cgroup 是否可用
    crate::cgroups::check_cgroup_mounted()?;
    
    info!("Fire 运行时初始化完成");
    Ok(())
}

// 运行时清理
pub fn cleanup() -> Result<()> {
    info!("清理 Fire 运行时");
    
    let mut manager = RUNTIME_MANAGER.lock().unwrap();
    manager.cleanup_all()?;
    
    info!("Fire 运行时清理完成");
    Ok(())
}
