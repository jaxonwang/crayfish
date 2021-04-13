extern crate fern;
extern crate log;

use crate::meta_data;

use fern::colors::{Color, ColoredLevelConfig};
use std::env;
use std::string::String;
use std::sync::atomic::{AtomicI32, Ordering};
use std::thread;

pub use log::{debug, error, info, trace, warn};

static GLOBAL_ID: AtomicI32 = AtomicI32::new(-1); // -1 as uninit

pub fn set_global_id(id: i32) {
    GLOBAL_ID.store(id, Ordering::Relaxed);
}

pub fn setup_logger() -> Result<(), fern::InitError> {
    let colors = ColoredLevelConfig::new().info(Color::Green).debug(Color::White);

    let mut pkg_name_upper = String::from(meta_data::PKG_NAME);
    pkg_name_upper.make_ascii_uppercase();
    let pkg_name_upper = pkg_name_upper.replace("-", "_");
    let log_level_env_name = format!("{}_LOG_LEVEL", &pkg_name_upper);
    let env_value = match env::var(&log_level_env_name) {
        Ok(v) => v,
        Err(_) => "".to_string(),
    };

    let debug_level = match env_value.as_str() {
        "trace" => log::LevelFilter::Trace,
        "debug" => log::LevelFilter::Debug,
        "info" => log::LevelFilter::Info,
        "warn" => log::LevelFilter::Warn,
        "error" => log::LevelFilter::Error,
        _ if cfg!(debug_assertions) => log::LevelFilter::Debug,
        _ => log::LevelFilter::Warn,
    };
    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{date} {file}:{line} {pid}:{tid:?} {level} {message}",
                date = chrono::Local::now().format("%H:%M:%S.%6f"),
                file = record.file().unwrap_or("unknown"),
                line = record.line().unwrap_or(0),
                pid = GLOBAL_ID.load(Ordering::Relaxed),
                tid = thread::current().id(),
                level = colors.color(record.level()),
                message = message
            ))
        })
        .level(debug_level)
        .chain(std::io::stdout())
        .apply()?;
    Ok(())
}
