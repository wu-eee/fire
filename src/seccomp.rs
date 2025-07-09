use crate::errors::*;
use log::warn;
use oci::{LinuxSeccomp, LinuxSeccompAction, LinuxSyscall};
use seccomp_sys::*;

fn init(act: u32) -> Result<*mut scmp_filter_ctx> {
    let ctx = unsafe { seccomp_init(act) };
    if ctx.is_null() {
        return Err(crate::errors::FireError::Generic(
            "failed to initialize seccomp".to_string(),
        ));
    }
    Ok(ctx)
}

pub fn initialize_seccomp(seccomp: &LinuxSeccomp) -> Result<()> {
    if seccomp.syscalls.is_empty() {
        return Ok(());
    }

    let default_action = match seccomp.default_action {
        LinuxSeccompAction::SCMP_ACT_KILL => SCMP_ACT_KILL,
        LinuxSeccompAction::SCMP_ACT_TRAP => SCMP_ACT_TRAP,
        LinuxSeccompAction::SCMP_ACT_ERRNO => SCMP_ACT_ERRNO(1),
        LinuxSeccompAction::SCMP_ACT_TRACE => SCMP_ACT_TRACE(1),
        LinuxSeccompAction::SCMP_ACT_ALLOW => SCMP_ACT_ALLOW,
    };

    let ctx = init(default_action)?;

    for syscall in &seccomp.syscalls {
        add_syscall_rule(ctx, syscall)?;
    }

    load(ctx)?;

    unsafe {
        seccomp_release(ctx);
    }

    Ok(())
}

fn add_syscall_rule(ctx: *mut scmp_filter_ctx, syscall: &LinuxSyscall) -> Result<()> {
    let action = match syscall.action {
        LinuxSeccompAction::SCMP_ACT_KILL => SCMP_ACT_KILL,
        LinuxSeccompAction::SCMP_ACT_TRAP => SCMP_ACT_TRAP,
        LinuxSeccompAction::SCMP_ACT_ERRNO => SCMP_ACT_ERRNO(1),
        LinuxSeccompAction::SCMP_ACT_TRACE => SCMP_ACT_TRACE(1),
        LinuxSeccompAction::SCMP_ACT_ALLOW => SCMP_ACT_ALLOW,
    };

    for name in &syscall.names {
        let name_cstr = std::ffi::CString::new(name.as_str()).map_err(|e| {
            crate::errors::FireError::Generic(format!("Invalid syscall name: {}", e))
        })?;
        let syscall_nr = unsafe { seccomp_syscall_resolve_name(name_cstr.as_ptr()) };
        if syscall_nr == __NR_SCMP_ERROR {
            warn!("unknown syscall: {}", name);
            continue;
        }

        let ret = unsafe { seccomp_rule_add(ctx, action, syscall_nr, 0) };
        if ret != 0 {
            return Err(crate::errors::FireError::Generic(format!(
                "failed to add syscall rule for {}",
                name
            )));
        }
    }

    Ok(())
}

fn load(ctx: *mut scmp_filter_ctx) -> Result<()> {
    let ret = unsafe { seccomp_load(ctx) };
    if ret != 0 {
        return Err(crate::errors::FireError::Generic(
            "failed to load seccomp filter".to_string(),
        ));
    }
    Ok(())
}
