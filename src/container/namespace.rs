use crate::errors::Result;
use nix::fcntl::{open, OFlag};
use nix::sched::{clone, unshare, CloneFlags};
use nix::sys::stat::Mode;
use nix::unistd::{close, getpid};
use std::os::unix::io::RawFd;
use std::collections::HashMap;
use log::{debug, error, info, warn};
use std::fs;
use std::os::unix::io::{AsRawFd, BorrowedFd};
use std::path::Path;

/// Linux namespace类型，对应OCI规范
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamespaceType {
    /// 进程namespace
    Pid,
    /// 网络namespace
    Network,
    /// 挂载namespace
    Mount,
    /// IPC namespace
    Ipc,
    /// UTS namespace (hostname)
    Uts,
    /// 用户namespace
    User,
    /// Cgroup namespace
    Cgroup,
}

impl NamespaceType {
    /// 获取namespace类型对应的clone flags
    pub fn clone_flag(&self) -> CloneFlags {
        match self {
            NamespaceType::Pid => CloneFlags::CLONE_NEWPID,
            NamespaceType::Network => CloneFlags::CLONE_NEWNET,
            NamespaceType::Mount => CloneFlags::CLONE_NEWNS,
            NamespaceType::Ipc => CloneFlags::CLONE_NEWIPC,
            NamespaceType::Uts => CloneFlags::CLONE_NEWUTS,
            NamespaceType::User => CloneFlags::CLONE_NEWUSER,
            NamespaceType::Cgroup => CloneFlags::CLONE_NEWCGROUP,
        }
    }

    /// 获取namespace类型对应的proc路径
    pub fn proc_path(&self) -> &'static str {
        match self {
            NamespaceType::Pid => "pid",
            NamespaceType::Network => "net",
            NamespaceType::Mount => "mnt",
            NamespaceType::Ipc => "ipc",
            NamespaceType::Uts => "uts",
            NamespaceType::User => "user",
            NamespaceType::Cgroup => "cgroup",
        }
    }

    /// 从OCI规范的LinuxNamespaceType转换为namespace类型
    pub fn from_oci_type(oci_type: &oci::LinuxNamespaceType) -> Result<Self> {
        match oci_type {
            oci::LinuxNamespaceType::pid => Ok(NamespaceType::Pid),
            oci::LinuxNamespaceType::network => Ok(NamespaceType::Network),
            oci::LinuxNamespaceType::mount => Ok(NamespaceType::Mount),
            oci::LinuxNamespaceType::ipc => Ok(NamespaceType::Ipc),
            oci::LinuxNamespaceType::uts => Ok(NamespaceType::Uts),
            oci::LinuxNamespaceType::user => Ok(NamespaceType::User),
            oci::LinuxNamespaceType::cgroup => Ok(NamespaceType::Cgroup),
        }
    }

    /// 从OCI规范的字符串转换为namespace类型
    pub fn from_oci_string(s: &str) -> Result<Self> {
        match s {
            "pid" => Ok(NamespaceType::Pid),
            "network" => Ok(NamespaceType::Network),
            "mount" => Ok(NamespaceType::Mount),
            "ipc" => Ok(NamespaceType::Ipc),
            "uts" => Ok(NamespaceType::Uts),
            "user" => Ok(NamespaceType::User),
            "cgroup" => Ok(NamespaceType::Cgroup),
            _ => Err(crate::errors::FireError::InvalidSpec(format!(
                "不支持的namespace类型: {}",
                s
            ))),
        }
    }
}

/// 单个namespace的配置
#[derive(Debug, Clone)]
pub struct Namespace {
    /// Namespace类型
    pub ns_type: NamespaceType,
    /// Namespace路径（可选，用于加入已存在的namespace）
    pub path: Option<String>,
    /// 文件描述符（用于保持namespace引用）
    pub fd: Option<RawFd>,
}

impl Namespace {
    /// 创建新的namespace实例
    pub fn new(ns_type: NamespaceType, path: Option<String>) -> Self {
        Self {
            ns_type,
            path,
            fd: None,
        }
    }

    /// 从OCI规范创建namespace
    pub fn from_oci_namespace(oci_ns: &oci::LinuxNamespace) -> Result<Self> {
        let ns_type = NamespaceType::from_oci_type(&oci_ns.typ)?;
        let path = if oci_ns.path.is_empty() {
            None
        } else {
            Some(oci_ns.path.clone())
        };
        Ok(Self::new(ns_type, path))
    }

    /// 创建新的namespace
    pub fn create(&mut self) -> Result<()> {
        debug!("创建namespace: {:?}", self.ns_type);
        
        // 如果有指定路径，则加入现有namespace
        if let Some(path) = self.path.clone() {
            return self.join_existing(&path);
        }

        // 创建新的namespace
        let flag = self.ns_type.clone_flag();
        match unshare(flag) {
            Ok(_) => {
                info!("成功创建namespace: {:?}", self.ns_type);
                Ok(())
            }
            Err(e) => {
                error!("创建namespace失败: {:?}, 错误: {}", self.ns_type, e);
                Err(crate::errors::FireError::Nix(e))
            }
        }
    }

    /// 加入现有namespace
    pub fn join_existing(&mut self, path: &str) -> Result<()> {
        debug!("加入现有namespace: {:?}, 路径: {}", self.ns_type, path);

        // 检查路径是否存在
        if !Path::new(path).exists() {
            return Err(crate::errors::FireError::InvalidSpec(format!(
                "Namespace路径不存在: {}",
                path
            )));
        }

        // 打开namespace文件
        let fd = match open(path, OFlag::O_RDONLY, Mode::empty()) {
            Ok(fd) => fd,
            Err(e) => {
                error!("打开namespace文件失败: {}, 错误: {}", path, e);
                return Err(crate::errors::FireError::Nix(e));
            }
        };

        // 加入namespace
        let borrowed_fd = unsafe { BorrowedFd::borrow_raw(fd) };
        match nix::sched::setns(borrowed_fd, self.ns_type.clone_flag()) {
            Ok(_) => {
                info!("成功加入namespace: {:?}, 路径: {}", self.ns_type, path);
                self.fd = Some(fd);
                Ok(())
            }
            Err(e) => {
                error!("加入namespace失败: {:?}, 错误: {}", self.ns_type, e);
                // 关闭文件描述符
                let _ = close(fd);
                Err(crate::errors::FireError::Nix(e))
            }
        }
    }

    /// 获取当前namespace的路径
    pub fn current_path(&self) -> String {
        format!("/proc/self/ns/{}", self.ns_type.proc_path())
    }

    /// 获取进程的namespace路径
    pub fn process_path(&self, pid: i32) -> String {
        format!("/proc/{}/ns/{}", pid, self.ns_type.proc_path())
    }
}

impl Drop for Namespace {
    fn drop(&mut self) {
        if let Some(fd) = self.fd {
            let _ = close(fd);
        }
    }
}

/// Namespace管理器
#[derive(Debug, Clone)]
pub struct NamespaceManager {
    /// 管理的namespaces
    namespaces: HashMap<NamespaceType, Namespace>,
    /// 用户namespace映射
    user_mapping: Option<UserNamespaceMapping>,
}

impl NamespaceManager {
    /// 创建新的namespace管理器
    pub fn new() -> Self {
        Self {
            namespaces: HashMap::new(),
            user_mapping: None,
        }
    }

    /// 从OCI规范创建namespace管理器
    pub fn from_oci_namespaces(oci_namespaces: &[oci::LinuxNamespace]) -> Result<Self> {
        let mut manager = Self::new();
        
        for oci_ns in oci_namespaces {
            let namespace = Namespace::from_oci_namespace(oci_ns)?;
            manager.add_namespace(namespace);
        }
        
        Ok(manager)
    }

    /// 从OCI规范创建包含用户映射的namespace管理器
    pub fn from_oci_linux_config(linux_config: &oci::Linux) -> Result<Self> {
        let mut manager = Self::from_oci_namespaces(&linux_config.namespaces)?;
        
        // 如果有用户namespace，添加用户映射
        if manager.contains_namespace(NamespaceType::User) {
            if !linux_config.uid_mappings.is_empty() || !linux_config.gid_mappings.is_empty() {
                let user_mapping = UserNamespaceMapping::from_oci_mappings(
                    &linux_config.uid_mappings,
                    &linux_config.gid_mappings,
                );
                manager.set_user_mapping(user_mapping);
                info!("设置用户namespace映射: UID映射={}, GID映射={}",
                    linux_config.uid_mappings.len(),
                    linux_config.gid_mappings.len()
                );
            }
        }
        
        Ok(manager)
    }

    /// 设置用户namespace映射
    pub fn set_user_mapping(&mut self, mapping: UserNamespaceMapping) {
        self.user_mapping = Some(mapping);
    }

    /// 添加namespace
    pub fn add_namespace(&mut self, namespace: Namespace) {
        debug!("添加namespace: {:?}", namespace.ns_type);
        self.namespaces.insert(namespace.ns_type, namespace);
    }

    /// 获取namespace
    pub fn get_namespace(&self, ns_type: NamespaceType) -> Option<&Namespace> {
        self.namespaces.get(&ns_type)
    }

    /// 获取可变namespace
    pub fn get_namespace_mut(&mut self, ns_type: NamespaceType) -> Option<&mut Namespace> {
        self.namespaces.get_mut(&ns_type)
    }

    /// 创建所有namespace
    pub fn create_all(&mut self) -> Result<()> {
        info!("开始创建所有namespace");
        
        // 按照推荐顺序创建namespace
        // 用户namespace需要首先创建，因为其他namespace的创建可能需要特权
        let creation_order = vec![
            NamespaceType::User,
            NamespaceType::Pid,
            NamespaceType::Network,
            NamespaceType::Mount,
            NamespaceType::Ipc,
            NamespaceType::Uts,
            NamespaceType::Cgroup,
        ];

        for ns_type in creation_order {
            if let Some(namespace) = self.namespaces.get_mut(&ns_type) {
                match namespace.create() {
                    Ok(_) => {
                        info!("成功创建namespace: {:?}", ns_type);
                        
                        // 如果是用户namespace，应用用户映射
                        if ns_type == NamespaceType::User {
                            if let Some(ref mapping) = self.user_mapping {
                                if let Err(e) = mapping.apply_mappings() {
                                    error!("应用用户namespace映射失败: {}", e);
                                    return Err(e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("创建namespace失败: {:?}, 错误: {}", ns_type, e);
                        return Err(e);
                    }
                }
            }
        }

        info!("所有namespace创建完成");
        Ok(())
    }

    /// 获取所有namespace类型
    pub fn get_namespace_types(&self) -> Vec<NamespaceType> {
        self.namespaces.keys().cloned().collect()
    }

    /// 检查是否包含指定的namespace
    pub fn contains_namespace(&self, ns_type: NamespaceType) -> bool {
        self.namespaces.contains_key(&ns_type)
    }

    /// 验证namespace配置
    pub fn validate(&self) -> Result<()> {
        debug!("验证namespace配置");

        // 检查用户namespace映射
        if self.contains_namespace(NamespaceType::User) {
            if let Some(ref mapping) = self.user_mapping {
                // 验证映射是否有效
                for uid_mapping in &mapping.uid_mappings {
                    if uid_mapping.size == 0 {
                        return Err(crate::errors::FireError::InvalidSpec(
                            "UID映射大小不能为0".to_string()
                        ));
                    }
                }
                for gid_mapping in &mapping.gid_mappings {
                    if gid_mapping.size == 0 {
                        return Err(crate::errors::FireError::InvalidSpec(
                            "GID映射大小不能为0".to_string()
                        ));
                    }
                }
            }
        }

        // 检查namespace组合是否有效
        if self.contains_namespace(NamespaceType::Pid) 
            && !self.contains_namespace(NamespaceType::Mount) {
            warn!("建议：使用PID namespace时建议同时使用Mount namespace");
        }

        if self.contains_namespace(NamespaceType::Network) 
            && !self.contains_namespace(NamespaceType::Uts) {
            warn!("建议：使用Network namespace时建议同时使用UTS namespace");
        }

        info!("namespace配置验证通过");
        Ok(())
    }

    /// 获取namespace统计信息
    pub fn get_statistics(&self) -> HashMap<String, usize> {
        let mut stats = HashMap::new();
        stats.insert("total_namespaces".to_string(), self.namespaces.len());
        
        let mut type_counts = HashMap::new();
        for (ns_type, _) in &self.namespaces {
            let type_name = format!("{:?}", ns_type).to_lowercase();
            *type_counts.entry(type_name).or_insert(0) += 1;
        }
        
        stats.extend(type_counts);
        
        if let Some(ref mapping) = self.user_mapping {
            stats.insert("uid_mappings".to_string(), mapping.uid_mappings.len());
            stats.insert("gid_mappings".to_string(), mapping.gid_mappings.len());
        }
        
        stats
    }
}

/// 进入指定的namespace
pub fn enter_namespace(namespace: &Namespace) -> Result<()> {
    debug!("进入namespace: {:?}", namespace.ns_type);
    
    if let Some(ref path) = namespace.path {
        // 使用现有namespace
        let fd = match open(path.as_str(), OFlag::O_RDONLY, Mode::empty()) {
            Ok(fd) => fd,
            Err(e) => {
                error!("打开namespace文件失败: {}, 错误: {}", path, e);
                return Err(crate::errors::FireError::Nix(e));
            }
        };

        let borrowed_fd = unsafe { BorrowedFd::borrow_raw(fd) };
        match nix::sched::setns(borrowed_fd, namespace.ns_type.clone_flag()) {
            Ok(_) => {
                info!("成功进入namespace: {:?}", namespace.ns_type);
                let _ = close(fd);
                Ok(())
            }
            Err(e) => {
                error!("进入namespace失败: {:?}, 错误: {}", namespace.ns_type, e);
                let _ = close(fd);
                Err(crate::errors::FireError::Nix(e))
            }
        }
    } else {
        // 创建新的namespace
        let flag = namespace.ns_type.clone_flag();
        match unshare(flag) {
            Ok(_) => {
                info!("成功创建并进入namespace: {:?}", namespace.ns_type);
                Ok(())
            }
            Err(e) => {
                error!("创建namespace失败: {:?}, 错误: {}", namespace.ns_type, e);
                Err(crate::errors::FireError::Nix(e))
            }
        }
    }
}

/// 进入多个namespace
pub fn enter_namespaces(namespaces: &[Namespace]) -> Result<()> {
    info!("进入多个namespace, 数量: {}", namespaces.len());
    
    for namespace in namespaces {
        enter_namespace(namespace)?;
    }
    
    info!("所有namespace进入完成");
    Ok(())
}

/// 获取进程的namespace信息
pub fn get_process_namespaces(pid: i32) -> Result<HashMap<NamespaceType, String>> {
    let mut namespaces = HashMap::new();
    
    let namespace_types = vec![
        NamespaceType::Pid,
        NamespaceType::Network,
        NamespaceType::Mount,
        NamespaceType::Ipc,
        NamespaceType::Uts,
        NamespaceType::User,
        NamespaceType::Cgroup,
    ];
    
    for ns_type in namespace_types {
        let path = format!("/proc/{}/ns/{}", pid, ns_type.proc_path());
        if Path::new(&path).exists() {
            // 读取namespace的inode信息
            match fs::read_link(&path) {
                Ok(link) => {
                    let inode = link.to_string_lossy().to_string();
                    namespaces.insert(ns_type, inode);
                }
                Err(e) => {
                    warn!("读取namespace信息失败: {}, 错误: {}", path, e);
                }
            }
        }
    }
    
    Ok(namespaces)
}

/// 用户namespace映射
#[derive(Debug, Clone)]
pub struct UserNamespaceMapping {
    pub uid_mappings: Vec<oci::LinuxIDMapping>,
    pub gid_mappings: Vec<oci::LinuxIDMapping>,
}

impl UserNamespaceMapping {
    /// 创建新的用户namespace映射
    pub fn new() -> Self {
        Self {
            uid_mappings: Vec::new(),
            gid_mappings: Vec::new(),
        }
    }

    /// 从OCI规范创建用户namespace映射
    pub fn from_oci_mappings(
        uid_mappings: &[oci::LinuxIDMapping],
        gid_mappings: &[oci::LinuxIDMapping],
    ) -> Self {
        Self {
            uid_mappings: uid_mappings.to_vec(),
            gid_mappings: gid_mappings.to_vec(),
        }
    }

    /// 应用用户namespace映射
    pub fn apply_mappings(&self) -> Result<()> {
        debug!("应用用户namespace映射");

        // 应用UID映射
        if !self.uid_mappings.is_empty() {
            self.write_id_map("/proc/self/uid_map", &self.uid_mappings)?;
            info!("成功应用UID映射，数量: {}", self.uid_mappings.len());
        }

        // 应用GID映射
        if !self.gid_mappings.is_empty() {
            // 在写入GID映射之前，需要写入/proc/self/setgroups
            self.write_setgroups_deny()?;
            self.write_id_map("/proc/self/gid_map", &self.gid_mappings)?;
            info!("成功应用GID映射，数量: {}", self.gid_mappings.len());
        }

        Ok(())
    }

    /// 写入ID映射文件
    fn write_id_map(&self, path: &str, mappings: &[oci::LinuxIDMapping]) -> Result<()> {
        let mut content = String::new();
        for mapping in mappings {
            content.push_str(&format!(
                "{} {} {}\n",
                mapping.container_id, mapping.host_id, mapping.size
            ));
        }

        match fs::write(path, content) {
            Ok(_) => {
                debug!("成功写入ID映射文件: {}", path);
                Ok(())
            }
            Err(e) => {
                error!("写入ID映射文件失败: {}, 错误: {}", path, e);
                Err(crate::errors::FireError::Io(e))
            }
        }
    }

    /// 写入setgroups文件
    fn write_setgroups_deny(&self) -> Result<()> {
        let path = "/proc/self/setgroups";
        match fs::write(path, "deny") {
            Ok(_) => {
                debug!("成功设置setgroups为deny");
                Ok(())
            }
            Err(e) => {
                error!("设置setgroups失败: {}", e);
                Err(crate::errors::FireError::Io(e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_type_conversion() {
        assert_eq!(NamespaceType::from_oci_string("pid").unwrap(), NamespaceType::Pid);
        assert_eq!(NamespaceType::from_oci_string("network").unwrap(), NamespaceType::Network);
        assert!(NamespaceType::from_oci_string("invalid").is_err());
    }

    #[test]
    fn test_namespace_creation() {
        let namespace = Namespace::new(NamespaceType::Pid, None);
        assert_eq!(namespace.ns_type, NamespaceType::Pid);
        assert!(namespace.path.is_none());
    }

    #[test]
    fn test_namespace_manager() {
        let mut manager = NamespaceManager::new();
        let namespace = Namespace::new(NamespaceType::Pid, None);
        manager.add_namespace(namespace);
        
        assert!(manager.contains_namespace(NamespaceType::Pid));
        assert!(!manager.contains_namespace(NamespaceType::Network));
    }
}
