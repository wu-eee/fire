#![allow(unknown_lints)]
#![recursion_limit = "1024"]

use clap::{Parser, Subcommand};
use std::process;

mod capabilities;
mod cgroups;
mod commands;
mod container;
mod errors;
mod logger;
mod mounts;
mod nix_ext;
mod runtime;
mod seccomp;
mod selinux;
mod signals;
mod sync;

use commands::Command;

#[derive(Parser)]
#[command(name = "fire")]
#[command(about = "Fire 容器运行时")]
#[command(version = "1.0.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new container
    Create {
        /// Container ID
        id: String,
        /// Bundle path
        bundle: Option<String>,
    },
    /// Start a container
    Start {
        /// Container ID
        id: String,
    },
    /// Kill a container
    Kill {
        /// Container ID
        id: String,
        /// Signal to send
        #[arg(short, long, default_value = "15")]
        signal: i32,
    },
    /// Delete a container
    Delete {
        /// Container ID
        id: String,
        /// Force delete
        #[arg(short, long)]
        force: bool,
    },
    /// Get container state
    State {
        /// Container ID
        id: String,
    },
    /// Run a container
    Run {
        /// Container ID
        id: String,
        /// Bundle path
        bundle: Option<String>,
    },
    /// Pause a container
    Pause {
        /// Container ID
        id: String,
    },
    /// Resume a paused container
    Resume {
        /// Container ID
        id: String,
    },
    /// List containers
    Ps,
}

fn main() {
    // 初始化日志
    logger::init().unwrap_or_else(|e| {
        eprintln!("初始化日志失败: {}", e);
        process::exit(1);
    });

    // 初始化运行时
    if let Err(e) = runtime::init() {
        eprintln!("初始化运行时失败: {}", e);
        process::exit(1);
    }

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Create { id, bundle } => {
            let cmd = commands::create::CreateCommand::new(id, bundle);
            cmd.execute()
        }
        Commands::Start { id } => {
            let cmd = commands::start::StartCommand::new(id);
            cmd.execute()
        }
        Commands::Kill { id, signal } => {
            let cmd = commands::kill::KillCommand::new(id, signal);
            cmd.execute()
        }
        Commands::Delete { id, force } => {
            let cmd = commands::delete::DeleteCommand::new(id, force);
            cmd.execute()
        }
        Commands::State { id } => {
            let cmd = commands::state::StateCommand::new(id);
            cmd.execute()
        }
        Commands::Run { id, bundle } => {
            let cmd = commands::run::RunCommand::new(id, bundle);
            cmd.execute()
        }
        Commands::Pause { id } => {
            let mut runtime = runtime::Runtime::new();
            runtime.pause_container(&id)
        }
        Commands::Resume { id } => {
            let mut runtime = runtime::Runtime::new();
            runtime.resume_container(&id)
        }
        Commands::Ps => {
            let cmd = commands::ps::PsCommand::new();
            cmd.execute()
        }
    };

    if let Err(e) = result {
        eprintln!("错误: {}", e);
        process::exit(1);
    }

    // 清理运行时
    if let Err(e) = runtime::cleanup() {
        eprintln!("清理运行时失败: {}", e);
        process::exit(1);
    }
}
