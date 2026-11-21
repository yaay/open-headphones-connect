//! A small append-only log file. `crate::log!("…")` works like `println!`.

use std::fmt::Arguments;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 512 * 1024;

struct Log {
    path: PathBuf,
    file: Option<File>,
}

static LOG: OnceLock<Mutex<Log>> = OnceLock::new();

pub fn init(dir: &Path) {
    let _ = fs::create_dir_all(dir);
    let path = dir.join("app.log");
    if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = fs::rename(&path, dir.join("app.previous.log"));
    }
    let _ = LOG.set(Mutex::new(Log { path, file: None }));
    // The shared protocol and session code logs to the same file.
    ohc_core::logger::set_sink(write);
}

pub fn write(args: Arguments) {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let line = format!(
        "{:02}:{:02}:{:02}Z {}\n",
        secs / 3600 % 24,
        secs / 60 % 60,
        secs % 60,
        args
    );
    #[cfg(debug_assertions)]
    eprint!("{line}");
    let Some(log) = LOG.get() else { return };
    let Ok(mut log) = log.lock() else { return };
    // Opened on first use, and reopened after a failed write, so that one
    // unlucky moment (another program holding the file) does not silence
    // the rest of the run.
    if log.file.is_none() {
        log.file = OpenOptions::new().create(true).append(true).open(&log.path).ok();
    }
    if let Some(file) = log.file.as_mut() {
        if file.write_all(line.as_bytes()).is_err() {
            log.file = None;
        }
    }
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::logger::write(format_args!($($arg)*))
    };
}
