use thiserror::Error;

#[derive(Error, Debug)]
pub enum FireError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid specification: {0}")]
    InvalidSpec(String),

    #[error("Generic error: {0}")]
    Generic(String),

    #[error("Nix error: {0}")]
    Nix(#[from] nix::Error),

    #[error("Serde JSON error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("Capabilities error: {0}")]
    Capabilities(#[from] caps::errors::CapsError),

    #[error("NUL error: {0}")]
    NulError(#[from] std::ffi::NulError),
}

pub type Result<T> = std::result::Result<T, FireError>;

// 兼容性宏
#[macro_export]
macro_rules! bail {
    ($msg:expr) => {
        return Err($crate::errors::FireError::Generic($msg.to_string()))
    };
    ($fmt:expr, $($arg:tt)*) => {
        return Err($crate::errors::FireError::Generic(format!($fmt, $($arg)*)))
    };
}

// 提供 ResultExt trait 用于兼容性
pub trait ResultExt<T> {
    fn chain_err<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String;
}

impl<T, E> ResultExt<T> for std::result::Result<T, E>
where
    E: Into<FireError>,
{
    fn chain_err<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| {
            let base_error = e.into();
            let context = f();
            FireError::Generic(format!("{}: {}", context, base_error))
        })
    }
}
