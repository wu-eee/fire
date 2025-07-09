use crate::errors::Result;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{fork, ForkResult, Pid};
use log::{debug, error, info};

#[derive(Debug, Clone)]
pub struct Process {
    pub pid: Option<i32>,
    pub command: Vec<String>,
    pub args: Vec<String>,
    pub env: Vec<String>,
    pub cwd: String,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
}

impl Process {
    pub fn new(command: Vec<String>) -> Self {
        let (cmd, args) = if command.is_empty() {
            (vec!["/bin/sh".to_string()], vec![])
        } else {
            let cmd = command[0].clone();
            let args = command[1..].to_vec();
            (vec![cmd], args)
        };

        Self {
            pid: None,
            command: cmd,
            args,
            env: Vec::new(),
            cwd: "/".to_string(),
            uid: None,
            gid: None,
        }
    }

    pub fn set_env(&mut self, env: Vec<String>) {
        self.env = env;
    }

    pub fn set_cwd(&mut self, cwd: String) {
        self.cwd = cwd;
    }

    pub fn set_uid_gid(&mut self, uid: Option<u32>, gid: Option<u32>) {
        self.uid = uid;
        self.gid = gid;
    }

    /// 启动容器进程
    pub fn start(&mut self) -> Result<i32> {
        info!("启动容器进程: {:?}", self.command);
        
        match unsafe { fork() } {
            Ok(ForkResult::Parent { child }) => {
                let pid = child.as_raw();
                self.pid = Some(pid);
                info!("容器进程启动成功, PID: {}", pid);
                Ok(pid)
            }
            Ok(ForkResult::Child) => {
                // 子进程中执行容器命令
                self.exec_in_child()
            }
            Err(e) => {
                error!("fork 失败: {}", e);
                Err(crate::errors::FireError::Nix(e))
            }
        }
    }

    /// 在子进程中执行命令
    fn exec_in_child(&self) -> ! {
        // 设置工作目录
        if let Err(e) = std::env::set_current_dir(&self.cwd) {
            error!("设置工作目录失败: {}", e);
            std::process::exit(1);
        }

        // 设置环境变量
        for env_var in &self.env {
            if let Some(eq_pos) = env_var.find('=') {
                let key = &env_var[..eq_pos];
                let value = &env_var[eq_pos + 1..];
                std::env::set_var(key, value);
            }
        }

        // 设置用户和组
        if let Some(gid) = self.gid {
            if let Err(e) = nix::unistd::setgid(nix::unistd::Gid::from_raw(gid)) {
                error!("设置 GID 失败: {}", e);
                std::process::exit(1);
            }
        }

        if let Some(uid) = self.uid {
            if let Err(e) = nix::unistd::setuid(nix::unistd::Uid::from_raw(uid)) {
                error!("设置 UID 失败: {}", e);
                std::process::exit(1);
            }
        }

        // 执行命令
        let err = exec_command(&self.command[0], &self.args);
        error!("执行命令失败: {}", err);
        std::process::exit(1);
    }

    /// 等待进程结束
    pub fn wait(&self) -> Result<i32> {
        if let Some(pid) = self.pid {
            debug!("等待进程 {} 结束", pid);
            match waitpid(Pid::from_raw(pid), None) {
                Ok(WaitStatus::Exited(_, exit_code)) => {
                    info!("进程 {} 正常退出，退出码: {}", pid, exit_code);
                    Ok(exit_code)
                }
                Ok(WaitStatus::Signaled(_, signal, _)) => {
                    info!("进程 {} 被信号 {} 终止", pid, signal);
                    Ok(128 + signal as i32)
                }
                Ok(status) => {
                    info!("进程 {} 状态: {:?}", pid, status);
                    Ok(0)
                }
                Err(e) => {
                    error!("等待进程失败: {}", e);
                    Err(crate::errors::FireError::Nix(e))
                }
            }
        } else {
            Err(crate::errors::FireError::Generic(
                "进程未启动".to_string()
            ))
        }
    }

    /// 杀死进程
    pub fn kill(&self, signal: i32) -> Result<()> {
        if let Some(pid) = self.pid {
            info!("向进程 {} 发送信号 {}", pid, signal);
            match nix::sys::signal::kill(
                Pid::from_raw(pid),
                nix::sys::signal::Signal::try_from(signal).unwrap_or(nix::sys::signal::SIGTERM),
            ) {
                Ok(_) => {
                    info!("信号发送成功");
                    Ok(())
                }
                Err(e) => {
                    error!("发送信号失败: {}", e);
                    Err(crate::errors::FireError::Nix(e))
                }
            }
        } else {
            Err(crate::errors::FireError::Generic(
                "进程未启动".to_string()
            ))
        }
    }

    /// 检查进程是否存在
    pub fn is_alive(&self) -> bool {
        if let Some(pid) = self.pid {
            match nix::sys::signal::kill(Pid::from_raw(pid), None) {
                Ok(_) => true,
                Err(_) => false,
            }
        } else {
            false
        }
    }
}

fn exec_command(program: &str, args: &[String]) -> std::io::Error {
    use std::ffi::CString;
    use std::ptr;

    let program_c = CString::new(program).unwrap();
    let args_c: Vec<CString> = std::iter::once(program.to_string())
        .chain(args.iter().cloned())
        .map(|arg| CString::new(arg).unwrap())
        .collect();
    let mut args_ptr: Vec<*const libc::c_char> = args_c.iter().map(|arg| arg.as_ptr()).collect();
    args_ptr.push(ptr::null());

    unsafe {
        libc::execvp(program_c.as_ptr(), args_ptr.as_ptr());
    }

    std::io::Error::last_os_error()
}
