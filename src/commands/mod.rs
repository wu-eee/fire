use crate::errors::Result;

pub mod create;
pub mod delete;
pub mod kill;
pub mod ps;
pub mod run;
pub mod start;
pub mod state;

/// 命令执行的通用trait
pub trait Command {
    /// 执行命令
    fn execute(&self) -> Result<()>;
}
