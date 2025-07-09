use crate::commands::create::CreateCommand;
use crate::commands::start::StartCommand;
use crate::errors::Result;
use log::info;

pub struct RunCommand {
    pub id: String,
    pub bundle: Option<String>,
}

impl RunCommand {
    pub fn new(id: String, bundle: Option<String>) -> Self {
        Self { id, bundle }
    }
}

impl super::Command for RunCommand {
    fn execute(&self) -> Result<()> {
        info!("运行容器: {}", self.id);

        // 先创建容器
        let create_cmd = CreateCommand::new(self.id.clone(), self.bundle.clone());
        create_cmd.execute()?;

        // 然后启动容器
        let start_cmd = StartCommand::new(self.id.clone());
        start_cmd.execute()?;

        info!("容器 {} 创建并启动成功", self.id);
        Ok(())
    }
}
