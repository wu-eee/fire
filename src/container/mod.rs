pub mod namespace;
pub mod process;
pub mod state;

use crate::errors::Result;
use crate::cgroups;
use namespace::{NamespaceManager, NamespaceType};
use oci::Spec;
use process::Process;
use std::collections::HashMap;
use log::{info, warn, error};

#[derive(Debug, Clone)]
pub struct Container {
    pub id: String,
    pub spec: Spec,
    pub bundle: String,
    pub state: ContainerState,
    pub processes: HashMap<i32, process::Process>,
    pub created_at: std::time::SystemTime,
    pub namespace_manager: Option<NamespaceManager>,
    pub cgroup_path: String,
    pub main_process: Option<Process>,
}

#[derive(Debug, Clone)]
pub enum ContainerState {
    Created,
    Running,
    Stopped,
    Paused,
}

impl Container {
    pub fn new(id: String, spec: Spec, bundle: String) -> Result<Self> {
        // 生成 cgroup 路径
        let cgroup_path = if let Some(ref linux) = spec.linux {
            if !linux.cgroups_path.is_empty() {
                linux.cgroups_path.clone()
            } else {
                cgroups::generate_cgroup_path(&id, None)
            }
        } else {
            cgroups::generate_cgroup_path(&id, None)
        };

        // 验证 cgroup 路径
        cgroups::validate_cgroup_path(&cgroup_path)?;
        
        // 检查 cgroup 是否可用
        cgroups::check_cgroup_mounted()?;

        // 创建namespace管理器
        let namespace_manager = if let Some(ref linux) = spec.linux {
            if !linux.namespaces.is_empty() {
                info!("为容器 {} 创建namespace管理器", id);
                let manager = NamespaceManager::from_oci_linux_config(linux)?;
                
                // 验证namespace配置
                manager.validate()?;
                
                // 记录namespace统计信息
                let stats = manager.get_statistics();
                info!("容器 {} 的namespace统计: {:?}", id, stats);
                
                Some(manager)
            } else {
                None
            }
        } else {
            None
        };

        // 创建主进程
        let main_process = {
            let mut process = Process::new(spec.process.args.clone());
            process.set_env(spec.process.env.clone());
            process.set_cwd(spec.process.cwd.clone());
            
            // 设置用户和组
            process.set_uid_gid(Some(spec.process.user.uid), Some(spec.process.user.gid));
            
            Some(process)
        };

        Ok(Container {
            id,
            spec,
            bundle,
            state: ContainerState::Created,
            processes: HashMap::new(),
            created_at: std::time::SystemTime::now(),
            namespace_manager,
            cgroup_path,
            main_process,
        })
    }

    pub fn start(&mut self) -> Result<()> {
        if !matches!(self.state, ContainerState::Created) {
            return Err(crate::errors::FireError::Generic(format!(
                "容器 {} 不在创建状态，无法启动",
                self.id
            )));
        }

        info!("启动容器 {}", self.id);

        // 创建所有namespace
        if let Some(ref mut namespace_manager) = self.namespace_manager {
            info!("为容器 {} 创建namespace", self.id);
            namespace_manager.create_all()?;
            
            // 记录创建的namespace类型
            let ns_types = namespace_manager.get_namespace_types();
            info!("容器 {} 创建的namespace类型: {:?}", self.id, ns_types);
        }

        // 启动主进程
        let pid = if let Some(ref mut main_process) = self.main_process {
            info!("启动容器 {} 的主进程", self.id);
            main_process.start()?
        } else {
            return Err(crate::errors::FireError::Generic(
                "容器没有主进程".to_string()
            ));
        };

        // 应用 cgroup 限制
        if let Some(ref linux) = self.spec.linux {
            info!("为容器 {} 应用 cgroup 限制，路径: {}", self.id, self.cgroup_path);
            cgroups::apply_pid(&linux.resources, pid, &self.cgroup_path)?;
            info!("cgroup 限制应用成功");
        }

        // 将主进程添加到进程列表
        if let Some(ref main_process) = self.main_process {
            self.processes.insert(pid, main_process.clone());
        }

        // 设置容器状态为运行中
        self.state = ContainerState::Running;
        info!("容器 {} 启动成功，主进程 PID: {}", self.id, pid);
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        if !matches!(self.state, ContainerState::Running) {
            return Err(crate::errors::FireError::Generic(format!(
                "容器 {} 不在运行状态，无法停止",
                self.id
            )));
        }

        info!("停止容器 {}", self.id);

        // 杀死主进程
        if let Some(ref main_process) = self.main_process {
            if main_process.is_alive() {
                info!("终止容器 {} 的主进程", self.id);
                main_process.kill(15)?; // SIGTERM
                
                // 等待进程结束
                match main_process.wait() {
                    Ok(exit_code) => {
                        info!("容器 {} 主进程已结束，退出码: {}", self.id, exit_code);
                    }
                    Err(e) => {
                        error!("等待容器 {} 主进程结束失败: {}", self.id, e);
                    }
                }
            }
        }

        // 设置容器状态为停止
        self.state = ContainerState::Stopped;
        info!("容器 {} 停止成功", self.id);
        Ok(())
    }

    pub fn pause(&mut self) -> Result<()> {
        if !matches!(self.state, ContainerState::Running) {
            return Err(crate::errors::FireError::Generic(format!(
                "容器 {} 不在运行状态，无法暂停",
                self.id
            )));
        }

        info!("暂停容器 {}", self.id);
        
        // 使用 cgroup freezer 暂停容器
        cgroups::freeze(&self.cgroup_path)?;
        
        self.state = ContainerState::Paused;
        info!("容器 {} 暂停成功", self.id);
        Ok(())
    }

    pub fn resume(&mut self) -> Result<()> {
        if !matches!(self.state, ContainerState::Paused) {
            return Err(crate::errors::FireError::Generic(format!(
                "容器 {} 不在暂停状态，无法恢复",
                self.id
            )));
        }

        info!("恢复容器 {}", self.id);
        
        // 检测 cgroup 版本并使用相应的恢复方法
        let cgroup_version = cgroups::detect_cgroup_version()?;
        match cgroup_version {
            1 => {
                // cgroup v1 使用 freezer.state
                cgroups::write_file(
                    &format!("/sys/fs/cgroup/freezer{}", self.cgroup_path),
                    "freezer.state",
                    "THAWED",
                )?;
            }
            2 => {
                // cgroup v2 使用 cgroup.freeze
                cgroups::write_file(
                    &format!("/sys/fs/cgroup{}", self.cgroup_path),
                    "cgroup.freeze",
                    "0",
                )?;
            }
            _ => {
                return Err(crate::errors::FireError::Generic(
                    format!("不支持的 cgroup 版本: {}", cgroup_version)
                ));
            }
        }
        
        self.state = ContainerState::Running;
        info!("容器 {} 恢复成功", self.id);
        Ok(())
    }

    pub fn cleanup(&mut self) -> Result<()> {
        info!("清理容器 {} 资源", self.id);

        // 清理 cgroup
        match cgroups::remove(&self.cgroup_path) {
            Ok(_) => {
                info!("容器 {} 的 cgroup 清理成功", self.id);
            }
            Err(e) => {
                error!("清理容器 {} 的 cgroup 失败: {}", self.id, e);
                // 不返回错误，继续清理其他资源
            }
        }

        // 清理进程列表
        self.processes.clear();
        self.main_process = None;

        info!("容器 {} 资源清理完成", self.id);
        Ok(())
    }

    pub fn get_main_process_pid(&self) -> Option<i32> {
        self.main_process.as_ref().and_then(|p| p.pid)
    }

    pub fn get_cgroup_path(&self) -> &str {
        &self.cgroup_path
    }

    pub fn get_state(&self) -> &ContainerState {
        &self.state
    }

    pub fn get_id(&self) -> &str {
        &self.id
    }

    pub fn get_bundle(&self) -> &str {
        &self.bundle
    }

    /// 检查容器是否有指定的namespace
    pub fn has_namespace(&self, ns_type: NamespaceType) -> bool {
        self.namespace_manager
            .as_ref()
            .map(|manager| manager.contains_namespace(ns_type))
            .unwrap_or(false)
    }

    /// 获取容器的namespace管理器
    pub fn get_namespace_manager(&self) -> Option<&NamespaceManager> {
        self.namespace_manager.as_ref()
    }

    /// 获取容器的可变namespace管理器
    pub fn get_namespace_manager_mut(&mut self) -> Option<&mut NamespaceManager> {
        self.namespace_manager.as_mut()
    }

    /// 获取容器的namespace信息
    pub fn get_namespace_info(&self) -> HashMap<String, String> {
        let mut info = HashMap::new();
        
        if let Some(ref manager) = self.namespace_manager {
            let ns_types = manager.get_namespace_types();
            for ns_type in ns_types {
                let key = format!("{:?}", ns_type).to_lowercase();
                let value = if let Some(ns) = manager.get_namespace(ns_type) {
                    if let Some(ref path) = ns.path {
                        format!("存在 (路径: {})", path)
                    } else {
                        "新建".to_string()
                    }
                } else {
                    "未知".to_string()
                };
                info.insert(key, value);
            }
        }
        
        info
    }

    /// 执行容器内的命令（需要进入namespace）
    pub fn exec_in_container(&self, command: &[String]) -> Result<()> {
        if !matches!(self.state, ContainerState::Running) {
            return Err(crate::errors::FireError::Generic(format!(
                "容器 {} 不在运行状态，无法执行命令",
                self.id
            )));
        }

        info!("在容器 {} 中执行命令: {:?}", self.id, command);

        // 如果有namespace管理器，需要进入相应的namespace
        if let Some(ref manager) = self.namespace_manager {
            // 获取所有namespace并进入
            let namespaces: Vec<_> = manager.get_namespace_types()
                .iter()
                .filter_map(|&ns_type| manager.get_namespace(ns_type).cloned())
                .collect();
            
            if !namespaces.is_empty() {
                namespace::enter_namespaces(&namespaces)?;
                info!("成功进入容器 {} 的namespace环境", self.id);
            }
        }

        // TODO: 实际执行命令的逻辑
        warn!("命令执行功能尚未完全实现: {:?}", command);
        Ok(())
    }
}
