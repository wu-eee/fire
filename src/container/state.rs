#[derive(Debug, Clone)]
pub enum ContainerState {
    Creating,
    Created,
    Running(i32),
    Stopped(i32),
}

impl ContainerState {
    pub fn is_running(&self) -> bool {
        matches!(self, ContainerState::Running(_))
    }

    pub fn is_stopped(&self) -> bool {
        matches!(self, ContainerState::Stopped(_))
    }
}
