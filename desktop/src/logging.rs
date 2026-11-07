//! Diagnostics. Every process (app, native host, MCP, CLI) logs to stderr;
//! the app also appends to a file in its configuration folder, since a GUI
//! launched from a menu has no terminal. `HERBARIUM_LOG` takes the usual
//! `tracing` filter syntax (`debug`, `herbarium_core=debug,warn`, ...) and
//! defaults to `warn`. At `info` the vault index reports its counts and
//! duration; at `debug` every operation and protocol request is timed.

use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

/// A log file is moved aside once it grows past this many bytes.
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

/// Install the global subscriber. `with_file` adds the app's log file.
/// Calling it twice is harmless.
pub fn init(with_file: bool) {
    let filter =
        EnvFilter::try_from_env("HERBARIUM_LOG").unwrap_or_else(|_| EnvFilter::new("warn"));
    let stderr = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_target(false);
    let file = with_file
        .then(open_log_file)
        .flatten()
        .map(|f| fmt::layer().with_writer(Mutex::new(f)).with_ansi(false));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(stderr)
        .with(file)
        .try_init();
}

/// `<config dir>/logs/herbarium.log`; the previous file is kept as `.1`.
fn open_log_file() -> Option<File> {
    let dir: PathBuf = crate::config::dir().ok()?.join("logs");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("herbarium.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = std::fs::rename(&path, dir.join("herbarium.log.1"));
    }
    OpenOptions::new().create(true).append(true).open(path).ok()
}

/// Logs how long a scope took, at debug level, when it ends (whatever the
/// path out of it).
pub struct Timed {
    what: &'static str,
    detail: String,
    started: Instant,
}

impl Timed {
    pub fn new(what: &'static str, detail: String) -> Self {
        Timed {
            what,
            detail,
            started: Instant::now(),
        }
    }
}

impl Drop for Timed {
    fn drop(&mut self) {
        tracing::debug!(
            elapsed_ms = self.started.elapsed().as_secs_f64() * 1e3,
            "{} {}",
            self.what,
            self.detail
        );
    }
}
