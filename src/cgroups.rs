use lazy_static::lazy_static;
use oci::{LinuxDeviceCgroup, LinuxDeviceType, LinuxResources};
use std::collections::HashMap;
use std::fs::{create_dir_all, read_to_string, remove_dir, write};
use crate::errors::Result;
use log::{info, warn};

/// 生成容器的 cgroup 路径
pub fn generate_cgroup_path(container_id: &str, cgroup_parent: Option<&str>) -> String {
    let parent = cgroup_parent.unwrap_or("/fire");
    format!("{}/{}", parent, container_id)
}

/// 检查 cgroup 是否已挂载
pub fn check_cgroup_mounted() -> Result<()> {
    let cgroup_root = "/sys/fs/cgroup";
    if !std::path::Path::new(cgroup_root).exists() {
        return Err(crate::errors::FireError::Generic(
            "cgroup 文件系统未挂载到 /sys/fs/cgroup".to_string()
        ));
    }
    
    // 检查是否为 cgroup v2
    if std::path::Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
        info!("检测到 cgroup v2");
        return check_cgroup_v2();
    }
    
    // 检查 cgroup v1 控制器
    info!("检测到 cgroup v1");
    return check_cgroup_v1();
}

/// 检查 cgroup v1 控制器
fn check_cgroup_v1() -> Result<()> {
    let required_controllers = ["cpu", "memory", "cpuset", "devices"];
    for controller in &required_controllers {
        let controller_path = format!("/sys/fs/cgroup/{}", controller);
        if !std::path::Path::new(&controller_path).exists() {
            return Err(crate::errors::FireError::Generic(
                format!("cgroup v1 控制器 {} 不存在", controller)
            ));
        }
    }
    Ok(())
}

/// 检查 cgroup v2 控制器
fn check_cgroup_v2() -> Result<()> {
    let controllers_file = "/sys/fs/cgroup/cgroup.controllers";
    if !std::path::Path::new(controllers_file).exists() {
        return Err(crate::errors::FireError::Generic(
            "cgroup v2 controllers 文件不存在".to_string()
        ));
    }
    
    let controllers_content = std::fs::read_to_string(controllers_file)
        .map_err(|e| crate::errors::FireError::Generic(
            format!("读取 cgroup v2 controllers 失败: {}", e)
        ))?;
    
    let available_controllers: Vec<&str> = controllers_content.trim().split_whitespace().collect();
    info!("可用的 cgroup v2 控制器: {:?}", available_controllers);
    
    // 检查必需的控制器
    let required_controllers = ["cpu", "memory", "pids"];
    for controller in &required_controllers {
        if !available_controllers.contains(controller) {
            return Err(crate::errors::FireError::Generic(
                format!("cgroup v2 控制器 {} 不可用", controller)
            ));
        }
    }
    
    Ok(())
}

/// 检测 cgroup 版本
pub fn detect_cgroup_version() -> Result<u8> {
    if std::path::Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
        Ok(2)
    } else if std::path::Path::new("/sys/fs/cgroup/cpu").exists() {
        Ok(1)
    } else {
        Err(crate::errors::FireError::Generic(
            "无法检测 cgroup 版本".to_string()
        ))
    }
}

/// 验证 cgroup 路径是否有效
pub fn validate_cgroup_path(cgroups_path: &str) -> Result<()> {
    if cgroups_path.is_empty() {
        return Err(crate::errors::FireError::InvalidSpec(
            "cgroup 路径不能为空".to_string()
        ));
    }
    
    if !cgroups_path.starts_with('/') {
        return Err(crate::errors::FireError::InvalidSpec(
            "cgroup 路径必须以 / 开头".to_string()
        ));
    }
    
    Ok(())
}

lazy_static! {
    static ref CGROUPS: HashMap<&'static str, Apply> = {
        let mut result = HashMap::new();
        result.insert("cpuset", cpuset_apply as Apply);
        result.insert("cpu", cpu_apply as Apply);
        result.insert("memory", memory_apply as Apply);
        result.insert("devices", devices_apply as Apply);
        result.insert("blkio", blkio_apply as Apply);
        result.insert("pids", pids_apply as Apply);
        result.insert("net_cls", net_cls_apply as Apply);
        result.insert("net_prio", net_prio_apply as Apply);
        result.insert("hugetlb", hugetlb_apply as Apply);
        result.insert("systemd", null_apply as Apply);
        result
    };
}

/// 应用资源限制到指定进程 (支持 cgroup v1 和 v2)
pub fn apply_pid(resources: &Option<LinuxResources>, pid: i32, cgroups_path: &str) -> Result<()> {
    let cgroup_version = detect_cgroup_version()?;
    
    match cgroup_version {
        1 => apply_pid_v1(resources, pid, cgroups_path),
        2 => apply_pid_v2(resources, pid, cgroups_path),
        _ => Err(crate::errors::FireError::Generic(
            format!("不支持的 cgroup 版本: {}", cgroup_version)
        ))
    }
}

/// cgroup v1 应用逻辑
fn apply_pid_v1(resources: &Option<LinuxResources>, pid: i32, cgroups_path: &str) -> Result<()> {
    if let Some(ref res) = resources {
        info!("应用 cgroup v1 资源限制到进程 {}, 路径: {}", pid, cgroups_path);
        
        for (subsystem, apply_fn) in CGROUPS.iter() {
            let path = format!("/sys/fs/cgroup/{}{}", subsystem, cgroups_path);
            apply_fn(res, &path)?;
            
            // 将进程添加到 cgroup
            let procs_file = format!("{}/cgroup.procs", path);
            write_file(&path, "cgroup.procs", &pid.to_string())?;
            info!("进程 {} 已添加到 {} cgroup", pid, subsystem);
        }
    }
    Ok(())
}

/// cgroup v2 应用逻辑
fn apply_pid_v2(resources: &Option<LinuxResources>, pid: i32, cgroups_path: &str) -> Result<()> {
    if let Some(ref res) = resources {
        info!("应用 cgroup v2 资源限制到进程 {}, 路径: {}", pid, cgroups_path);
        
        let cgroup_dir = format!("/sys/fs/cgroup{}", cgroups_path);
        
        // 创建 cgroup 目录
        create_dir_all(&cgroup_dir).map_err(|e| {
            crate::errors::FireError::Generic(format!("创建 cgroup v2 目录失败: {}", e))
        })?;
        
        // 启用必要的控制器
        enable_cgroup_v2_controllers(&cgroup_dir)?;
        
        // 应用资源限制
        apply_cgroup_v2_resources(res, &cgroup_dir)?;
        
        // 将进程添加到 cgroup
        let procs_file = format!("{}/cgroup.procs", cgroup_dir);
        std::fs::write(&procs_file, pid.to_string()).map_err(|e| {
            crate::errors::FireError::Generic(format!("添加进程到 cgroup v2 失败: {}", e))
        })?;
        
        info!("进程 {} 已添加到 cgroup v2: {}", pid, cgroup_dir);
    }
    Ok(())
}

/// 启用 cgroup v2 控制器
fn enable_cgroup_v2_controllers(cgroup_dir: &str) -> Result<()> {
    // 读取父目录的可用控制器
    let parent_dir = std::path::Path::new(cgroup_dir).parent()
        .unwrap_or_else(|| std::path::Path::new("/sys/fs/cgroup"));
    
    let controllers_file = parent_dir.join("cgroup.controllers");
    if !controllers_file.exists() {
        return Ok(()); // 根目录，无需启用
    }
    
    let available_controllers = std::fs::read_to_string(&controllers_file)
        .map_err(|e| crate::errors::FireError::Generic(
            format!("读取可用控制器失败: {}", e)
        ))?;
    
    let subtree_control_file = parent_dir.join("cgroup.subtree_control");
    let controllers_to_enable = ["cpu", "memory", "pids"];
    
    for controller in &controllers_to_enable {
        if available_controllers.contains(controller) {
            let enable_cmd = format!("+{}", controller);
            if let Err(e) = std::fs::write(&subtree_control_file, &enable_cmd) {
                warn!("启用控制器 {} 失败: {}", controller, e);
            } else {
                info!("已启用 cgroup v2 控制器: {}", controller);
            }
        }
    }
    
    Ok(())
}

/// 应用 cgroup v2 资源限制
fn apply_cgroup_v2_resources(resources: &LinuxResources, cgroup_dir: &str) -> Result<()> {
    // CPU 限制
    if let Some(ref cpu) = resources.cpu {
        if let Some(shares) = cpu.shares {
            // cgroup v2 使用 cpu.weight 替代 cpu.shares
            // 转换公式: weight = 1 + ((shares - 2) * 9999) / 262142
            let weight = 1 + ((shares.saturating_sub(2)) * 9999) / 262142;
            let weight = weight.min(10000).max(1);
            write_file(cgroup_dir, "cpu.weight", &weight.to_string())?;
        }
        
        if let Some(quota) = cpu.quota {
            if let Some(period) = cpu.period {
                if quota > 0 {
                    let cpu_max = format!("{} {}", quota, period);
                    write_file(cgroup_dir, "cpu.max", &cpu_max)?;
                }
            }
        }
    }
    
    // 内存限制
    if let Some(ref memory) = resources.memory {
        if let Some(limit) = memory.limit {
            if limit > 0 {
                write_file(cgroup_dir, "memory.max", &limit.to_string())?;
            }
        }
        
        if let Some(reservation) = memory.reservation {
            if reservation > 0 {
                write_file(cgroup_dir, "memory.low", &reservation.to_string())?;
            }
        }
    }
    
    // 进程数限制
    if let Some(ref pids) = resources.pids {
        if pids.limit > 0 {
            write_file(cgroup_dir, "pids.max", &pids.limit.to_string())?;
        }
    }
    
    Ok(())
}

pub fn init() {
    lazy_static::initialize(&CGROUPS);
}

pub fn freeze(cgroups_path: &str) -> Result<()> {
    let cgroup_version = detect_cgroup_version()?;
    
    match cgroup_version {
        1 => freeze_v1(cgroups_path),
        2 => freeze_v2(cgroups_path),
        _ => Err(crate::errors::FireError::Generic(
            format!("不支持的 cgroup 版本: {}", cgroup_version)
        ))
    }
}

fn freeze_v1(cgroups_path: &str) -> Result<()> {
    let freezer_path = format!("/sys/fs/cgroup/freezer{}", cgroups_path);
    create_dir_all(&freezer_path).map_err(|e| {
        crate::errors::FireError::Generic(format!("创建 freezer cgroup 失败: {}", e))
    })?;
    write_file(&freezer_path, "freezer.state", "FROZEN")
}

fn freeze_v2(cgroups_path: &str) -> Result<()> {
    let cgroup_dir = format!("/sys/fs/cgroup{}", cgroups_path);
    
    // cgroup v2 使用 cgroup.freeze 文件
    write_file(&cgroup_dir, "cgroup.freeze", "1")
}

pub fn remove(cgroups_path: &str) -> Result<()> {
    let cgroup_version = detect_cgroup_version()?;
    
    match cgroup_version {
        1 => remove_v1(cgroups_path),
        2 => remove_v2(cgroups_path),
        _ => Err(crate::errors::FireError::Generic(
            format!("不支持的 cgroup 版本: {}", cgroup_version)
        ))
    }
}

fn remove_v1(cgroups_path: &str) -> Result<()> {
    for (subsystem, _) in CGROUPS.iter() {
        let path = format!("/sys/fs/cgroup/{}{}", subsystem, cgroups_path);
        if std::path::Path::new(&path).exists() {
            match remove_dir(&path) {
                Ok(_) => info!("已删除 {} cgroup: {}", subsystem, path),
                Err(e) => warn!("删除 {} cgroup 失败: {}", subsystem, e),
            }
        }
    }
    Ok(())
}

fn remove_v2(cgroups_path: &str) -> Result<()> {
    let cgroup_dir = format!("/sys/fs/cgroup{}", cgroups_path);
    
    if std::path::Path::new(&cgroup_dir).exists() {
        match remove_dir(&cgroup_dir) {
            Ok(_) => info!("已删除 cgroup v2: {}", cgroup_dir),
            Err(e) => warn!("删除 cgroup v2 失败: {}", e),
        }
    }
    Ok(())
}

pub fn get_procs(subsystem: &str, cgroups_path: &str) -> Vec<i32> {
    let cgroup_version = detect_cgroup_version().unwrap_or(1);
    
    let procs_file = match cgroup_version {
        1 => format!("/sys/fs/cgroup/{}{}/cgroup.procs", subsystem, cgroups_path),
        2 => format!("/sys/fs/cgroup{}/cgroup.procs", cgroups_path),
        _ => return Vec::new(),
    };
    
    match read_to_string(&procs_file) {
        Ok(content) => content
            .lines()
            .filter_map(|line| line.trim().parse::<i32>().ok())
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub fn write_file(dir: &str, file: &str, data: &str) -> Result<()> {
    let path = format!("{}/{}", dir, file);
    write(&path, data)?;
    Ok(())
}

pub fn read_file(dir: &str, file: &str) -> Result<String> {
    let path = format!("{}/{}", dir, file);
    Ok(read_to_string(&path)?)
}

type Apply = fn(&LinuxResources, &str) -> Result<()>;

fn copy_parent(dir: &str, file: &str) -> Result<()> {
    let parent = if let Some(o) = dir.rfind('/') {
        &dir[..o]
    } else {
        return Err(crate::errors::FireError::Generic(format!(
            "failed to find {} in parent cgroups",
            file
        )));
    };

    let parent_data = read_file(parent, file)?;
    write_file(dir, file, &parent_data)?;
    Ok(())
}

fn null_apply(_: &LinuxResources, _: &str) -> Result<()> {
    Ok(())
}

fn cpuset_apply(r: &LinuxResources, dir: &str) -> Result<()> {
    copy_parent(dir, "cpuset.cpus")?;
    copy_parent(dir, "cpuset.mems")?;
    if let Some(ref cpu) = r.cpu {
        if !cpu.cpus.is_empty() {
            write_file(dir, "cpuset.cpus", &cpu.cpus)?;
        }
        if !cpu.mems.is_empty() {
            write_file(dir, "cpuset.mems", &cpu.mems)?;
        }
    }
    Ok(())
}

fn cpu_apply(r: &LinuxResources, dir: &str) -> Result<()> {
    if let Some(ref cpu) = r.cpu {
        if let Some(shares) = cpu.shares {
            write_file(dir, "cpu.shares", &shares.to_string())?;
        }
        if let Some(quota) = cpu.quota {
            write_file(dir, "cpu.cfs_quota_us", &quota.to_string())?;
        }
        if let Some(period) = cpu.period {
            write_file(dir, "cpu.cfs_period_us", &period.to_string())?;
        }
    }
    Ok(())
}

fn memory_apply(r: &LinuxResources, dir: &str) -> Result<()> {
    if let Some(ref memory) = r.memory {
        if let Some(limit) = memory.limit {
            write_file(dir, "memory.limit_in_bytes", &limit.to_string())?;
        }
        if let Some(reservation) = memory.reservation {
            write_file(dir, "memory.soft_limit_in_bytes", &reservation.to_string())?;
        }
        if let Some(swap) = memory.swap {
            write_file(dir, "memory.memsw.limit_in_bytes", &swap.to_string())?;
        }
        if let Some(kernel) = memory.kernel {
            write_file(dir, "memory.kmem.limit_in_bytes", &kernel.to_string())?;
        }
        if let Some(kernel_tcp) = memory.kernel_tcp {
            write_file(
                dir,
                "memory.kmem.tcp.limit_in_bytes",
                &kernel_tcp.to_string(),
            )?;
        }
        if let Some(swappiness) = memory.swappiness {
            write_file(dir, "memory.swappiness", &swappiness.to_string())?;
        }
    }
    Ok(())
}

fn blkio_apply(r: &LinuxResources, dir: &str) -> Result<()> {
    if let Some(ref blkio) = r.block_io {
        if let Some(weight) = blkio.weight {
            write_file(dir, "blkio.weight", &weight.to_string())?;
        }
        if let Some(leaf_weight) = blkio.leaf_weight {
            write_file(dir, "blkio.leaf_weight", &leaf_weight.to_string())?;
        }
        for device in &blkio.weight_device {
            let data = format!(
                "{}:{} {}",
                device.major,
                device.minor,
                device.weight.unwrap_or(0)
            );
            write_file(dir, "blkio.weight_device", &data)?;
        }
        for device in &blkio.throttle_read_bps_device {
            let data = format!("{}:{} {}", device.major, device.minor, device.rate);
            write_file(dir, "blkio.throttle.read_bps_device", &data)?;
        }
        for device in &blkio.throttle_write_bps_device {
            let data = format!("{}:{} {}", device.major, device.minor, device.rate);
            write_file(dir, "blkio.throttle.write_bps_device", &data)?;
        }
        for device in &blkio.throttle_read_iops_device {
            let data = format!("{}:{} {}", device.major, device.minor, device.rate);
            write_file(dir, "blkio.throttle.read_iops_device", &data)?;
        }
        for device in &blkio.throttle_write_iops_device {
            let data = format!("{}:{} {}", device.major, device.minor, device.rate);
            write_file(dir, "blkio.throttle.write_iops_device", &data)?;
        }
    }
    Ok(())
}

fn pids_apply(r: &LinuxResources, dir: &str) -> Result<()> {
    if let Some(ref pids) = r.pids {
        if pids.limit > 0 {
            write_file(dir, "pids.max", &pids.limit.to_string())?;
        }
    }
    Ok(())
}

fn net_cls_apply(r: &LinuxResources, dir: &str) -> Result<()> {
    if let Some(ref network) = r.network {
        if let Some(class_id) = network.class_id {
            write_file(dir, "net_cls.classid", &class_id.to_string())?;
        }
    }
    Ok(())
}

fn net_prio_apply(r: &LinuxResources, dir: &str) -> Result<()> {
    if let Some(ref network) = r.network {
        for priority in &network.priorities {
            let data = format!("{} {}", priority.name, priority.priority);
            write_file(dir, "net_prio.ifpriomap", &data)?;
        }
    }
    Ok(())
}

fn hugetlb_apply(r: &LinuxResources, dir: &str) -> Result<()> {
    for limit in &r.hugepage_limits {
        let file = format!("hugetlb.{}.limit_in_bytes", limit.page_size);
        write_file(dir, &file, &limit.limit.to_string())?;
    }
    Ok(())
}

fn write_device(d: &LinuxDeviceCgroup, dir: &str) -> Result<()> {
    let typ = match d.typ {
        LinuxDeviceType::b => "b",
        LinuxDeviceType::c => "c",
        LinuxDeviceType::a => "a",
        LinuxDeviceType::u => "c", // 'u' 也是字符设备
        LinuxDeviceType::p => {
            let msg = format!("invalid device type: {:?}", d.typ);
            return Err(crate::errors::FireError::InvalidSpec(msg));
        }
    };

    let major = d
        .major
        .map(|m| m.to_string())
        .unwrap_or_else(|| "*".to_string());
    let minor = d
        .minor
        .map(|m| m.to_string())
        .unwrap_or_else(|| "*".to_string());
    let access = &d.access;

    let data = format!("{} {}:{} {}", typ, major, minor, access);
    write_file(dir, "devices.allow", &data)?;
    Ok(())
}

fn devices_apply(r: &LinuxResources, dir: &str) -> Result<()> {
    write_file(dir, "devices.deny", "a")?;

    for device in &r.devices {
        if device.allow {
            write_device(device, dir)?;
        }
    }
    Ok(())
}
