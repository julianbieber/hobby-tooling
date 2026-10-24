//! A `log` backend appending warnings and errors to a private file, for programs that own the
//! terminal and so cannot print.

use std::fs::{DirBuilder, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::sync::Mutex;

use log::{Level, LevelFilter, Log, Metadata, Record};

const MAX_BYTES: u64 = 1024 * 1024;

struct FileLog(Mutex<File>);

impl Log for FileLog {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Warn
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {} {}: {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            record.level(),
            record.target(),
            record.args()
        );
        let line: String = line
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        if let Ok(mut file) = self.0.lock() {
            let _ = writeln!(file, "{line}");
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.0.lock() {
            let _ = file.flush();
        }
    }
}

/// Install the process's log backend: warnings and errors, each with a local timestamp, appended
/// to `xdg_cache_path(app, file)`. The dir is created private to the user and the file readable by
/// the user only; a file over 1 MB is started afresh. Does nothing when the file cannot be opened
/// or another backend is already installed.
pub fn log_to_file(app: &str, file: &str) {
    let path = crate::xdg_cache_path(app, file);
    if let Some(dir) = path.parent() {
        let _ = DirBuilder::new().recursive(true).mode(0o700).create(dir);
    }
    let too_big = std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES);
    let opened = OpenOptions::new()
        .create(true)
        .append(!too_big)
        .write(true)
        .truncate(too_big)
        .mode(0o600)
        .open(&path);
    let Ok(handle) = opened else { return };
    if log::set_logger(Box::leak(Box::new(FileLog(Mutex::new(handle))))).is_ok() {
        log::set_max_level(LevelFilter::Warn);
    }
}
