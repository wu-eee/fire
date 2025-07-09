use crate::errors::*;
use lazy_static::lazy_static;
use log::{warn, info};
use oci::{LinuxDevice, LinuxDeviceType, Mount, Spec};
use std::collections::HashMap;
use std::fs::{create_dir_all, File};
use std::os::unix::fs::symlink;
use std::path::Path;

pub fn mount_to(spec: &Spec, rootfs: &str, bind_device: bool) -> Result<()> {
    let olddir = std::env::current_dir()?;
    std::env::set_current_dir(rootfs)?;
    let _guard = scopeguard::guard(olddir, |olddir| {
        let _ = std::env::set_current_dir(&olddir);
    });

    info!("开始挂载文件系统到 rootfs: {}", rootfs);

    // 验证rootfs路径
    if !Path::new(rootfs).exists() {
        return Err(crate::errors::FireError::Generic(format!(
            "rootfs 路径不存在: {}",
            rootfs
        )));
    }

    // 处理根文件系统传播模式
    if let Some(ref linux) = spec.linux {
        setup_rootfs_propagation(&linux.rootfs_propagation)?;
    }

    // 挂载根文件系统
    mount_rootfs(rootfs)?;

    // 挂载所有指定的挂载点
    for m in &spec.mounts {
        if let Err(e) = mount_entry(m, bind_device) {
            warn!("挂载失败，但继续执行: {} -> {}: {}", m.source, m.destination, e);
        }
    }

    // 创建默认符号链接
    default_symlinks()?;
    
    // 创建设备文件
    if let Some(ref linux) = spec.linux {
        create_devices(&linux.devices, bind_device)?;
    }
    
    // 确保ptmx存在
    ensure_ptmx()?;

    info!("文件系统挂载完成");
    Ok(())
}

fn setup_rootfs_propagation(propagation: &str) -> Result<()> {
    let flags = match propagation {
        "shared" => libc::MS_SHARED | libc::MS_REC,
        "private" => libc::MS_PRIVATE | libc::MS_REC,
        "slave" | "" => libc::MS_SLAVE | libc::MS_REC,
        _ => {
            return Err(crate::errors::FireError::InvalidSpec(format!(
                "无效的传播模式: {}",
                propagation
            )));
        }
    };

    unsafe {
        if libc::mount(
            std::ptr::null(),
            std::ffi::CString::new("/")?.as_ptr(),
            std::ptr::null(),
            flags,
            std::ptr::null(),
        ) == -1 {
            return Err(crate::errors::FireError::Generic(format!(
                "设置rootfs传播模式失败: {}",
                std::io::Error::last_os_error()
            )));
        }
    }

    info!("设置rootfs传播模式: {}", propagation);
    Ok(())
}

fn mount_rootfs(rootfs: &str) -> Result<()> {
    let rootfs_cstr = std::ffi::CString::new(rootfs)?;
    
    // 绑定挂载rootfs到自身
    unsafe {
        if libc::mount(
            rootfs_cstr.as_ptr(),
            rootfs_cstr.as_ptr(),
            std::ptr::null(),
            libc::MS_BIND | libc::MS_REC,
            std::ptr::null(),
        ) == -1 {
            return Err(crate::errors::FireError::Generic(format!(
                "绑定挂载rootfs失败: {}",
                std::io::Error::last_os_error()
            )));
        }
    }

    info!("成功绑定挂载rootfs: {}", rootfs);
    Ok(())
}

fn mount_entry(m: &Mount, _bind_device: bool) -> Result<()> {
    let dest = Path::new(&m.destination);
    let parent = dest.parent().unwrap();
    create_dir_all(parent)?;

    // 解析挂载选项
    let (flags, data) = parse_mount_options(m);
    
    // 准备源路径
    let src = if m.typ == "bind" {
        // 对于bind挂载，需要处理源路径
        let source = std::fs::canonicalize(&m.source).map_err(|e| {
            crate::errors::FireError::Generic(format!("无法解析源路径 {}: {}", m.source, e))
        })?;
        
        // 确保目标目录存在
        let dir = if source.is_file() {
            dest.parent().unwrap()
        } else {
            dest
        };
        create_dir_all(dir)?;
        
        // 如果源是文件，确保目标文件存在
        if source.is_file() {
            let _ = File::create(dest);
        }
        
        source
    } else {
        create_dir_all(dest)?;
        std::path::PathBuf::from(&m.source)
    };

    // 执行挂载
    let dest_cstr = std::ffi::CString::new(dest.to_str().unwrap())
        .map_err(|e| crate::errors::FireError::Generic(format!("路径转换失败: {}", e)))?;
    let src_cstr = std::ffi::CString::new(src.to_str().unwrap())
        .map_err(|e| crate::errors::FireError::Generic(format!("路径转换失败: {}", e)))?;
    let typ_cstr = std::ffi::CString::new(m.typ.as_str())
        .map_err(|e| crate::errors::FireError::Generic(format!("类型转换失败: {}", e)))?;
    let data_cstr = std::ffi::CString::new(data.as_str())
        .map_err(|e| crate::errors::FireError::Generic(format!("数据转换失败: {}", e)))?;

    unsafe {
        if libc::mount(
            src_cstr.as_ptr(),
            dest_cstr.as_ptr(),
            typ_cstr.as_ptr(),
            flags,
            data_cstr.as_ptr() as *const libc::c_void,
        ) == -1 {
            let errno = std::io::Error::last_os_error();
            // 如果是EINVAL错误，尝试不使用data再次挂载
            if errno.raw_os_error() == Some(libc::EINVAL) && !data.is_empty() {
                let empty_data = std::ffi::CString::new("")?;
                if libc::mount(
                    src_cstr.as_ptr(),
                    dest_cstr.as_ptr(),
                    typ_cstr.as_ptr(),
                    flags,
                    empty_data.as_ptr() as *const libc::c_void,
                ) == -1 {
                    return Err(crate::errors::FireError::Generic(format!(
                        "挂载失败 {} -> {}: {}",
                        m.source, m.destination, std::io::Error::last_os_error()
                    )));
                }
            } else {
                return Err(crate::errors::FireError::Generic(format!(
                    "挂载失败 {} -> {}: {}",
                    m.source, m.destination, errno
                )));
            }
        }
    }

    // 对于bind挂载，如果有其他标志需要重新挂载
    if flags & libc::MS_BIND != 0 {
        let remount_flags = flags & !(libc::MS_BIND | libc::MS_REC);
        if remount_flags != 0 {
            unsafe {
                if libc::mount(
                    dest_cstr.as_ptr(),
                    dest_cstr.as_ptr(),
                    std::ptr::null(),
                    remount_flags | libc::MS_REMOUNT,
                    std::ptr::null(),
                ) == -1 {
                    warn!("重新挂载失败 {}: {}", m.destination, std::io::Error::last_os_error());
                }
            }
        }
    }

    info!("成功挂载 {} -> {} (类型: {}, 标志: {})", m.source, m.destination, m.typ, flags);
    Ok(())
}

pub fn pivot_rootfs(path: &str) -> Result<()> {
    let oldroot = Path::new("/.pivot_root");
    create_dir_all(&oldroot)?;

    // 打开旧的根目录文件描述符
    let olddir_fd = unsafe {
        libc::open(
            std::ffi::CString::new("/")?.as_ptr(),
            libc::O_DIRECTORY | libc::O_RDONLY,
        )
    };
    if olddir_fd < 0 {
        return Err(crate::errors::FireError::Generic(format!(
            "打开旧根目录失败: {}",
            std::io::Error::last_os_error()
        )));
    }

    // 打开新的根目录文件描述符
    let newdir_fd = unsafe {
        libc::open(
            std::ffi::CString::new(path)?.as_ptr(),
            libc::O_DIRECTORY | libc::O_RDONLY,
        )
    };
    if newdir_fd < 0 {
        unsafe { libc::close(olddir_fd) };
        return Err(crate::errors::FireError::Generic(format!(
            "打开新根目录失败: {}",
            std::io::Error::last_os_error()
        )));
    }

    // 执行pivot_root系统调用
    let path_cstr = std::ffi::CString::new(path)?;
    let oldroot_cstr = std::ffi::CString::new("/.pivot_root")?;
    
    unsafe {
        if libc::syscall(
            libc::SYS_pivot_root,
            path_cstr.as_ptr(),
            oldroot_cstr.as_ptr(),
        ) == -1 {
            let errno = std::io::Error::last_os_error();
            libc::close(olddir_fd);
            libc::close(newdir_fd);
            return Err(crate::errors::FireError::Generic(format!(
                "pivot_root 系统调用失败: {}",
                errno
            )));
        }
    }

    // 卸载旧根目录
    unsafe {
        let flags = libc::MNT_DETACH;
        if libc::umount2(oldroot_cstr.as_ptr(), flags) == -1 {
            warn!("卸载旧根目录失败: {}", std::io::Error::last_os_error());
        }
    }

    // 切换到新根目录
    unsafe {
        if libc::fchdir(newdir_fd) == -1 {
            let errno = std::io::Error::last_os_error();
            libc::close(olddir_fd);
            libc::close(newdir_fd);
            return Err(crate::errors::FireError::Generic(format!(
                "切换到新根目录失败: {}",
                errno
            )));
        }
    }

    // 清理文件描述符
    unsafe {
        libc::close(olddir_fd);
        libc::close(newdir_fd);
    }

    info!("成功执行 pivot_root 到: {}", path);
    Ok(())
}

pub fn finish_rootfs(spec: &Spec) -> Result<()> {
    if let Some(ref linux) = spec.linux {
        for path in &linux.masked_paths {
            mask_path(path)?;
        }
        for path in &linux.readonly_paths {
            readonly_path(path)?;
        }
    }
    Ok(())
}

#[rustfmt::skip]
lazy_static! {
    static ref OPTIONS: HashMap<&'static str, (bool, u64)> = {
        let mut m = HashMap::new();
        m.insert("defaults",      (false, 0));
        m.insert("ro",            (false, libc::MS_RDONLY));
        m.insert("rw",            (true,  libc::MS_RDONLY));
        m.insert("suid",          (true,  libc::MS_NOSUID));
        m.insert("nosuid",        (false, libc::MS_NOSUID));
        m.insert("dev",           (true,  libc::MS_NODEV));
        m.insert("nodev",         (false, libc::MS_NODEV));
        m.insert("exec",          (true,  libc::MS_NOEXEC));
        m.insert("noexec",        (false, libc::MS_NOEXEC));
        m.insert("sync",          (false, libc::MS_SYNCHRONOUS));
        m.insert("async",         (true,  libc::MS_SYNCHRONOUS));
        m.insert("dirsync",       (false, libc::MS_DIRSYNC));
        m.insert("remount",       (false, libc::MS_REMOUNT));
        m.insert("mand",          (false, libc::MS_MANDLOCK));
        m.insert("nomand",        (true,  libc::MS_MANDLOCK));
        m.insert("atime",         (true,  libc::MS_NOATIME));
        m.insert("noatime",       (false, libc::MS_NOATIME));
        m.insert("diratime",      (true,  libc::MS_NODIRATIME));
        m.insert("nodiratime",    (false, libc::MS_NODIRATIME));
        m.insert("bind",          (false, libc::MS_BIND));
        m.insert("rbind",         (false, libc::MS_BIND | libc::MS_REC));
        m.insert("unbindable",    (false, libc::MS_UNBINDABLE));
        m.insert("runbindable",   (false, libc::MS_UNBINDABLE | libc::MS_REC));
        m.insert("private",       (false, libc::MS_PRIVATE));
        m.insert("rprivate",      (false, libc::MS_PRIVATE | libc::MS_REC));
        m.insert("shared",        (false, libc::MS_SHARED));
        m.insert("rshared",       (false, libc::MS_SHARED | libc::MS_REC));
        m.insert("slave",         (false, libc::MS_SLAVE));
        m.insert("rslave",        (false, libc::MS_SLAVE | libc::MS_REC));
        m.insert("relatime",      (false, libc::MS_RELATIME));
        m.insert("norelatime",    (true,  libc::MS_RELATIME));
        m.insert("strictatime",   (false, libc::MS_STRICTATIME));
        m.insert("nostrictatime", (true,  libc::MS_STRICTATIME));
        m
    };
}

fn parse_mount_options(m: &Mount) -> (u64, String) {
    let mut flags = 0u64;
    let mut data = Vec::new();
    
    for option in &m.options {
        match OPTIONS.get(option.as_str()) {
            Some((clear, flag)) => {
                if *clear {
                    flags &= !flag;
                } else {
                    flags |= flag;
                }
            }
            None => {
                // 未知选项加入数据字符串
                data.push(option.clone());
            }
        }
    }
    
    (flags, data.join(","))
}

fn default_symlinks() -> Result<()> {
    let links = [
        ("/proc/self/fd", "/dev/fd"),
        ("/proc/self/fd/0", "/dev/stdin"),
        ("/proc/self/fd/1", "/dev/stdout"),
        ("/proc/self/fd/2", "/dev/stderr"),
    ];

    for (target, link) in &links {
        if let Err(e) = symlink(target, link) {
            if e.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(e.into());
            }
        }
    }
    Ok(())
}

fn create_devices(devices: &[LinuxDevice], bind: bool) -> Result<()> {
    let op: fn(&LinuxDevice) -> Result<()> = if bind { bind_dev } else { mknod_dev };

    for dev in devices {
        op(dev)?;
    }
    Ok(())
}

fn ensure_ptmx() -> Result<()> {
    let ptmx = Path::new("/dev/ptmx");
    if !ptmx.exists() {
        if let Err(e) = symlink("pts/ptmx", ptmx) {
            let msg = format!("failed to create /dev/ptmx symlink: {}", e);
            return Err(crate::errors::FireError::Generic(msg));
        }
    }
    Ok(())
}

fn to_sflag(t: LinuxDeviceType) -> Result<u32> {
    match t {
        LinuxDeviceType::b => Ok(libc::S_IFBLK as u32),
        LinuxDeviceType::c => Ok(libc::S_IFCHR as u32),
        LinuxDeviceType::u => Ok(libc::S_IFCHR as u32), // 'u' 也是字符设备
        LinuxDeviceType::p => Ok(libc::S_IFIFO as u32),
        LinuxDeviceType::a => {
            let msg = "cannot create device of type 'a'".to_string();
            Err(crate::errors::FireError::InvalidSpec(msg))
        }
    }
}

fn makedev(major: u64, minor: u64) -> u64 {
    (minor & 0xff) | ((major & 0xfff) << 8) | ((minor & !0xff) << 12) | ((major & !0xfff) << 32)
}

fn mknod_dev(dev: &LinuxDevice) -> Result<()> {
    let path = Path::new(&dev.path);
    let parent = path.parent().unwrap();
    create_dir_all(parent)?;

    let mode = dev.file_mode.unwrap_or(0o644);
    let dev_type = to_sflag(dev.typ)?;
    let device = makedev(dev.major as u64, dev.minor as u64);

    let path_cstr = std::ffi::CString::new(dev.path.as_str())
        .map_err(|e| crate::errors::FireError::Generic(format!("Invalid path: {}", e)))?;

    unsafe {
        if libc::mknod(path_cstr.as_ptr(), dev_type | mode, device) == -1 {
            return Err(crate::errors::FireError::Generic(format!(
                "mknod failed: {}",
                std::io::Error::last_os_error()
            )));
        }
    }

    if let (Some(uid), Some(gid)) = (dev.uid, dev.gid) {
        unsafe {
            if libc::chown(path_cstr.as_ptr(), uid, gid) == -1 {
                warn!(
                    "failed to chown {}: {}",
                    dev.path,
                    std::io::Error::last_os_error()
                );
            }
        }
    }

    Ok(())
}

fn bind_dev(dev: &LinuxDevice) -> Result<()> {
    let path = Path::new(&dev.path);
    let parent = path.parent().unwrap();
    create_dir_all(parent)?;

    // 打开/创建目标文件
    let fd = unsafe {
        libc::open(
            std::ffi::CString::new(dev.path.as_str())?.as_ptr(),
            libc::O_RDWR | libc::O_CREAT,
            0o644,
        )
    };
    if fd < 0 {
        return Err(crate::errors::FireError::Generic(format!(
            "创建设备文件失败 {}: {}",
            dev.path,
            std::io::Error::last_os_error()
        )));
    }
    unsafe { libc::close(fd) };

    // 执行绑定挂载
    let source_cstr = std::ffi::CString::new(dev.path.as_str())?;
    let dest_cstr = std::ffi::CString::new(dev.path.as_str())?;
    
    unsafe {
        if libc::mount(
            source_cstr.as_ptr(),
            dest_cstr.as_ptr(),
            std::ptr::null(),
            libc::MS_BIND,
            std::ptr::null(),
        ) == -1 {
            return Err(crate::errors::FireError::Generic(format!(
                "绑定挂载设备失败 {}: {}",
                dev.path,
                std::io::Error::last_os_error()
            )));
        }
    }

    info!("成功绑定挂载设备: {}", dev.path);
    Ok(())
}

fn mask_path(path: &str) -> Result<()> {
    // 验证路径安全性
    if !path.starts_with('/') || path.contains("..") {
        return Err(crate::errors::FireError::InvalidSpec(format!(
            "无效的屏蔽路径: {}",
            path
        )));
    }

    let target = Path::new(path);
    if target.exists() {
        // 使用 /dev/null 绑定挂载到目标路径来屏蔽它
        let devnull_cstr = std::ffi::CString::new("/dev/null")?;
        let path_cstr = std::ffi::CString::new(path)?;
        
        unsafe {
            if libc::mount(
                devnull_cstr.as_ptr(),
                path_cstr.as_ptr(),
                std::ptr::null(),
                libc::MS_BIND,
                std::ptr::null(),
            ) == -1 {
                let errno = std::io::Error::last_os_error();
                // 忽略 ENOENT 和 ENOTDIR 错误，因为路径可能不存在
                if errno.raw_os_error() != Some(libc::ENOENT) && 
                   errno.raw_os_error() != Some(libc::ENOTDIR) {
                    return Err(crate::errors::FireError::Generic(format!(
                        "屏蔽路径失败 {}: {}",
                        path, errno
                    )));
                } else {
                    warn!("忽略屏蔽不存在的路径: {}", path);
                }
            } else {
                info!("成功屏蔽路径: {}", path);
            }
        }
    } else {
        warn!("路径不存在，跳过屏蔽: {}", path);
    }
    Ok(())
}

fn readonly_path(path: &str) -> Result<()> {
    // 验证路径安全性
    if !path.starts_with('/') || path.contains("..") {
        return Err(crate::errors::FireError::InvalidSpec(format!(
            "无效的只读路径: {}",
            path
        )));
    }

    let target = Path::new(path);
    if target.exists() {
        let path_cstr = std::ffi::CString::new(path)?;
        
        // 首先进行绑定挂载
        unsafe {
            if libc::mount(
                path_cstr.as_ptr(),
                path_cstr.as_ptr(),
                std::ptr::null(),
                libc::MS_BIND | libc::MS_REC,
                std::ptr::null(),
            ) == -1 {
                let errno = std::io::Error::last_os_error();
                // 忽略 ENOENT 错误，因为路径可能不存在
                if errno.raw_os_error() != Some(libc::ENOENT) {
                    return Err(crate::errors::FireError::Generic(format!(
                        "绑定挂载只读路径失败 {}: {}",
                        path, errno
                    )));
                } else {
                    warn!("忽略不存在的只读路径: {}", path);
                    return Ok(());
                }
            }
        }
        
        // 然后重新挂载为只读
        unsafe {
            if libc::mount(
                path_cstr.as_ptr(),
                path_cstr.as_ptr(),
                std::ptr::null(),
                libc::MS_BIND | libc::MS_REC | libc::MS_RDONLY | libc::MS_REMOUNT,
                std::ptr::null(),
            ) == -1 {
                return Err(crate::errors::FireError::Generic(format!(
                    "重新挂载只读路径失败 {}: {}",
                    path,
                    std::io::Error::last_os_error()
                )));
            }
        }
        
        info!("成功设置只读路径: {}", path);
    } else {
        warn!("路径不存在，跳过只读设置: {}", path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    
    #[test]
    fn test_parse_mount_options() {
        let mount = Mount {
            destination: "/test".to_string(),
            source: "/source".to_string(),
            typ: "bind".to_string(),
            options: vec!["ro".to_string(), "nosuid".to_string()],
        };
        
        let (flags, data) = parse_mount_options(&mount);
        assert!(flags & libc::MS_RDONLY != 0);
        assert!(flags & libc::MS_NOSUID != 0);
        assert!(data.is_empty());
    }
    
    #[test]
    fn test_to_sflag() {
        assert_eq!(to_sflag(LinuxDeviceType::c).unwrap(), libc::S_IFCHR as u32);
        assert_eq!(to_sflag(LinuxDeviceType::b).unwrap(), libc::S_IFBLK as u32);
        assert_eq!(to_sflag(LinuxDeviceType::p).unwrap(), libc::S_IFIFO as u32);
        assert_eq!(to_sflag(LinuxDeviceType::u).unwrap(), libc::S_IFCHR as u32);
        assert!(to_sflag(LinuxDeviceType::a).is_err());
    }
    
    #[test]
    fn test_makedev() {
        let dev = makedev(1, 5);
        assert_eq!(dev, 0x105);
    }
    
    #[test]
    fn test_mount_options_with_data() {
        let mount = Mount {
            destination: "/test".to_string(),
            source: "/source".to_string(),
            typ: "ext4".to_string(),
            options: vec!["ro".to_string(), "user_xattr".to_string()],
        };
        
        let (flags, data) = parse_mount_options(&mount);
        assert!(flags & libc::MS_RDONLY != 0);
        assert_eq!(data, "user_xattr");
    }
}
