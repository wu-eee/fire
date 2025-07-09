use crate::errors::Result;
use crate::runtime::Runtime;
use log::info;

pub struct KillCommand {
    pub id: String,
    pub signal: i32,
}

impl KillCommand {
    pub fn new(id: String, signal: i32) -> Self {
        Self { id, signal }
    }
}

impl super::Command for KillCommand {
    fn execute(&self) -> Result<()> {
        info!("向容器 {} 发送信号 {}", self.id, self.signal);

        let mut runtime = Runtime::new();
        runtime.kill_container(&self.id, self.signal)?;

        info!("信号 {} 已发送到容器 {}", self.signal, self.id);
        Ok(())
    }
}
