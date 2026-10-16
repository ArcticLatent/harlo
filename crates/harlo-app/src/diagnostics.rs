//! Logs for tracking down problems, under `~/.local/share/harlo/logs/`:
//! - `harlo.log`: this run's log (Rust and Qt/QML messages); the previous
//!   run's is kept as `harlo.log.1`.
//! - `crash-<time>.log`: a panic with its backtrace.
//! - `protocol/*.jsonl`: harness traffic, when `--debug-protocol` or
//!   Settings → Providers → "Log harness traffic" is on.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use cxx_qt_lib::QString;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

pub fn logs_dir() -> PathBuf {
    harlo_core::db::default_path().parent().map(|p| p.join("logs")).unwrap_or_else(|| PathBuf::from("logs"))
}

fn forced_on() -> bool {
    std::env::args().any(|a| a == "--debug-protocol")
}

/// Logging to stderr and to `harlo.log`.
pub fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    let stderr = fmt::layer().with_writer(std::io::stderr);
    let file = open_log().map(|f| fmt::layer().with_ansi(false).with_writer(Mutex::new(f)));
    tracing_subscriber::registry().with(filter).with(stderr).with(file).init();
    if forced_on() {
        harlo_harness::protocol_log::set_dir(Some(logs_dir().join("protocol")));
    }
}

fn open_log() -> Option<File> {
    let dir = logs_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let log = dir.join("harlo.log");
    let _ = std::fs::rename(&log, dir.join("harlo.log.1"));
    // Append mode: Qt's message handler writes to the same file through its
    // own handle, and plain writes would overwrite its lines.
    std::fs::OpenOptions::new().create(true).append(true).open(&log).ok()
}

/// Settings → Providers → "Log harness traffic" (`--debug-protocol` keeps it on).
pub fn set_protocol_logging(on: bool) {
    let on = on || forced_on();
    harlo_harness::protocol_log::set_dir(on.then(|| logs_dir().join("protocol")));
}

/// Writes panics to `crash-<time>.log` (then reports them as before).
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let path = logs_dir().join(format!("crash-{secs}.log"));
        let thread = std::thread::current();
        let report = format!(
            "harlo {} crashed (unix time {secs})\nthread: {}\n{info}\n\nbacktrace:\n{}\n",
            env!("CARGO_PKG_VERSION"),
            thread.name().unwrap_or("unnamed"),
            std::backtrace::Backtrace::force_capture()
        );
        if std::fs::create_dir_all(logs_dir()).is_ok()
            && let Ok(mut f) = File::create(&path)
        {
            let _ = f.write_all(report.as_bytes());
            eprintln!("harlo: crash report written to {}", path.display());
        }
        tracing::error!("panic: {info}");
        previous(info);
    }));
}

/// Qt and QML messages go to `harlo.log` too (call once the app exists).
pub fn capture_qt_messages() {
    crate::platform::install_message_log(&QString::from(&logs_dir().join("harlo.log").to_string_lossy().into_owned()));
}
