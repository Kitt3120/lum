use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
    time::SystemTime,
};

use fern::colors::{Color, ColoredLevelConfig};
use log::{LevelFilter, SetLoggerError};

use crate::is_debug;

static IS_LOGGER_SET_UP: AtomicBool = AtomicBool::new(false);

pub fn is_set_up() -> bool {
    IS_LOGGER_SET_UP.load(Ordering::Relaxed)
}

pub fn setup() -> Result<(), SetLoggerError> {
    let colors = ColoredLevelConfig::new()
        .info(Color::Green)
        .debug(Color::Magenta)
        .warn(Color::Yellow)
        .error(Color::Red)
        .trace(Color::Cyan);

    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{} {: >25} {: <5}] {}",
                humantime::format_rfc3339_seconds(SystemTime::now()),
                record.target(),
                colors.color(record.level()),
                message
            ))
        })
        .level(get_min_log_level())
        .chain(io::stdout())
        .apply()?;

    IS_LOGGER_SET_UP.store(true, Ordering::Relaxed);

    Ok(())
}

fn get_min_log_level() -> LevelFilter {
    if is_debug() {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    }
}
