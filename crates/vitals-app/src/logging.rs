//! File logging. GUI builds have no console, so every diagnostic goes to `%LOCALAPPDATA%\Vitals\vitals.log`.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use log::{LevelFilter, Log, Metadata, Record};

struct FileLogger {
    file: Mutex<File>,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let millis = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(
                file,
                "{millis:>13} {:<5} {}: {}",
                record.level(),
                record.target(),
                record.args()
            );
            let _ = file.flush();
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

fn open_log() -> Option<(PathBuf, File)> {
    let path = vitals_sys::paths::log_path().ok()?;
    if fs::metadata(&path).is_ok_and(|meta| meta.len() > 1_500_000) {
        let _ = fs::rename(&path, path.with_extension("log.old"));
    }
    let file = OpenOptions::new().create(true).append(true).open(&path).ok()?;
    Some((path, file))
}

/// Starts file logging and installs a panic hook that records the panic before the default handler runs.
pub fn init() {
    if let Some((path, file)) = open_log() {
        let logger = Box::new(FileLogger { file: Mutex::new(file) });
        if log::set_boxed_logger(logger).is_ok() {
            let level = if std::env::var_os("VITALS_LOG").is_some() { LevelFilter::Debug } else { LevelFilter::Info };
            log::set_max_level(level);
            log::info!("Vitals {} starting (pid {}) log={}", env!("CARGO_PKG_VERSION"), std::process::id(), path.display());
        }
    }
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("panic: {info}");
        default(info);
    }));
}
