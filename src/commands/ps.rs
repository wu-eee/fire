use crate::errors::Result;
use crate::runtime::manager::RUNTIME_MANAGER;
use crate::cgroups;
use log::info;

pub struct PsCommand {}

impl PsCommand {
    pub fn new() -> Self {
        Self {}
    }
}

impl super::Command for PsCommand {
    fn execute(&self) -> Result<()> {
        info!("列出所有容器");

        let manager = RUNTIME_MANAGER.lock().unwrap();
        let containers = manager.list_containers();

        if containers.is_empty() {
            println!("没有找到任何容器");
            return Ok(());
        }

        // 打印表头
        println!("{:<20} {:<15} {:<10} {:<15} {:<30}", 
            "CONTAINER ID", "STATE", "PID", "CGROUP", "COMMAND");
        println!("{}", "-".repeat(90));

        for container in containers {
            let state = format!("{:?}", container.get_state()).to_lowercase();
            let pid = container.get_main_process_pid()
                .map(|p| p.to_string())
                .unwrap_or_else(|| "-".to_string());
            
            let cgroup_path = container.get_cgroup_path();
            let cgroup_display = if cgroup_path.len() > 25 {
                format!("...{}", &cgroup_path[cgroup_path.len()-22..])
            } else {
                cgroup_path.to_string()
            };
            
            let command = if !container.spec.process.args.is_empty() {
                container.spec.process.args.join(" ")
            } else {
                "N/A".to_string()
            };
            
            let command_display = if command.len() > 25 {
                format!("{}...", &command[..22])
            } else {
                command
            };

            println!("{:<20} {:<15} {:<10} {:<15} {:<30}", 
                container.id, state, pid, cgroup_display, command_display);
            
            // 显示详细的 cgroup 信息
            if container.get_main_process_pid().is_some() {
                let cgroup_procs = cgroups::get_procs("cpuset", cgroup_path);
                if !cgroup_procs.is_empty() {
                    println!("  └─ Cgroup 进程: {:?}", cgroup_procs);
                }
            }
        }

        Ok(())
    }
}

impl Default for PsCommand {
    fn default() -> Self {
        Self::new()
    }
}
