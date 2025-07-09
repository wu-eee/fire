use log::{Level, Log, Metadata, Record};

use std::io::{stderr, Write};

pub struct SimpleLogger;

pub static SIMPLE_LOGGER: SimpleLogger = SimpleLogger;

impl Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let _ = writeln!(&mut stderr(), "{} - {}", record.level(), record.args());
        }
    }

    fn flush(&self) {
        stderr().flush().expect("Failed to flush");
    }
}

/// 初始化日志系统
pub fn init() -> Result<(), log::SetLoggerError> {
    log::set_logger(&SIMPLE_LOGGER)?;
    log::set_max_level(log::LevelFilter::Info);
    Ok(())
}
