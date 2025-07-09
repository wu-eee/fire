// Functions in libc that haven't made it into nix yet
use crate::errors::Result;
use libc;
use nix::errno::Errno;
use std::ffi::CString;
use std::os::unix::io::RawFd;

#[inline]
pub fn lsetxattr(
    path: &CString,
    name: &CString,
    value: &CString,
    len: usize,
    flags: i32,
) -> Result<()> {
    let res = unsafe {
        libc::lsetxattr(
            path.as_ptr(),
            name.as_ptr(),
            value.as_ptr() as *const libc::c_void,
            len,
            flags,
        )
    };
    Errno::result(res).map(drop).map_err(|e| e.into())
}

#[inline]
pub fn fchdir(fd: RawFd) -> Result<()> {
    let res = unsafe { libc::fchdir(fd) };
    Errno::result(res).map(drop).map_err(|e| e.into())
}

#[inline]
pub fn setgroups(gids: &[libc::gid_t]) -> Result<()> {
    let res = unsafe { libc::setgroups(gids.len(), gids.as_ptr()) };
    Errno::result(res).map(drop).map_err(|e| e.into())
}

#[inline]
pub fn setrlimit(
    resource: libc::c_int,
    soft: libc::c_ulonglong,
    hard: libc::c_ulonglong,
) -> Result<()> {
    let rlim = &libc::rlimit {
        rlim_cur: soft,
        rlim_max: hard,
    };
    let res = unsafe { libc::setrlimit(resource as u32, rlim) };
    Errno::result(res).map(drop).map_err(|e| e.into())
}

#[inline]
pub fn clearenv() -> Result<()> {
    let res = unsafe { libc::clearenv() };
    Errno::result(res).map(drop).map_err(|e| e.into())
}

#[cfg(target_env = "gnu")]
#[inline]
pub fn putenv(string: &CString) -> Result<()> {
    // NOTE: gnue takes ownership of the string so we pass it
    //       with into_raw.
    //       This prevents the string to be de-allocated.
    //       According to
    //       https://www.gnu.org/software/libc/manual/html_node/Environment-Access.html
    //       the variable will be accessable from the exec'd program
    //       throughout its lifetime, as such this is not going to be re-claimed
    //       and will show up as leak in valgrind and friends.
    let ptr = string.clone().into_raw();
    let res = unsafe { libc::putenv(ptr as *mut libc::c_char) };
    Errno::result(res).map(drop).map_err(|e| e.into())
}

#[cfg(not(target_env = "gnu"))]
pub fn putenv(string: &CString) -> Result<()> {
    let res = unsafe { libc::putenv(string.as_ptr() as *mut libc::c_char) };
    Errno::result(res).map(drop).map_err(|e| e.into())
}

// 便利函数，用于简化字符串处理
pub fn lsetxattr_str(path: &str, name: &str, value: &[u8]) -> Result<()> {
    let path_cstr = std::ffi::CString::new(path)
        .map_err(|e| crate::errors::FireError::Generic(format!("Invalid path: {}", e)))?;
    let name_cstr = std::ffi::CString::new(name)
        .map_err(|e| crate::errors::FireError::Generic(format!("Invalid name: {}", e)))?;

    let res = unsafe {
        libc::lsetxattr(
            path_cstr.as_ptr(),
            name_cstr.as_ptr(),
            value.as_ptr() as *const libc::c_void,
            value.len(),
            0,
        )
    };
    Errno::result(res).map(drop).map_err(|e| e.into())
}
