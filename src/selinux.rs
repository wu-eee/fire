use crate::errors::*;
use crate::nix_ext::lsetxattr_str;

const SELINUX_XATTR: &str = "security.selinux";

pub fn setexeccon(label: &str) -> Result<()> {
    if label.is_empty() {
        return Ok(());
    }

    let path = "/proc/self/attr/exec";
    std::fs::write(path, label)?;
    Ok(())
}

pub fn setfilecon(file: &str, label: &str) -> Result<()> {
    if label.is_empty() {
        return Ok(());
    }

    lsetxattr_str(file, SELINUX_XATTR, label.as_bytes())?;
    Ok(())
}
