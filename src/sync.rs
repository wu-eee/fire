use crate::errors::*;
use nix::unistd::{close, read};
use std::os::unix::io::RawFd;

pub struct Sync {
    pub child_pipe: RawFd,
    pub parent_pipe: RawFd,
}

impl Sync {
    pub fn new() -> Result<Self> {
        let (read_fd, write_fd) = nix::unistd::pipe()?;
        Ok(Sync {
            child_pipe: write_fd,
            parent_pipe: read_fd,
        })
    }

    pub fn wait_for_child(&self) -> Result<()> {
        let mut buf = [0u8; 1];
        read(self.parent_pipe, &mut buf)?;
        Ok(())
    }

    pub fn notify_parent(&self) -> Result<()> {
        nix::unistd::write(self.child_pipe, b"1")?;
        Ok(())
    }

    pub fn close_child_pipe(&self) -> Result<()> {
        close(self.child_pipe)?;
        Ok(())
    }

    pub fn close_parent_pipe(&self) -> Result<()> {
        close(self.parent_pipe)?;
        Ok(())
    }
}

impl Drop for Sync {
    fn drop(&mut self) {
        let _ = close(self.child_pipe);
        let _ = close(self.parent_pipe);
    }
}
