use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use std::path::PathBuf;
use std::sync::OnceLock;

// Simple file-based debug logger used by tests to capture background-thread logs
// even when the test harness captures stdout/stderr.
static LOG_FILE: OnceLock<Mutex<File>> = OnceLock::new();

fn resolve_log_path() -> PathBuf {
    if let Ok(p) = std::env::var("RUST_COMMS_DEBUG_LOG") {
        return PathBuf::from(p);
    }
    // Default to the target directory so it’s easy to find alongside build artifacts.
    PathBuf::from("target/rust_comms_debug.log")
}

fn ensure_log_file() -> &'static Mutex<File> {
    LOG_FILE.get_or_init(|| {
        let path = resolve_log_path();
        // Try to create parent directories if missing
        if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
        let file = OpenOptions::new().create(true).append(true).open(path)
            .unwrap_or_else(|_| OpenOptions::new().create(true).write(true).open("rust_comms_debug.log").expect("open fallback log file"));
        Mutex::new(file)
    })
}

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

/// Append a line to the debug log file. Best-effort; errors are ignored.
pub fn log_line<S: AsRef<str>>(msg: S) {
    let line = format!("[{}] {}\n", now_ms(), msg.as_ref());
    if let Ok(mut f) = ensure_log_file().lock() {
        let _ = f.write_all(line.as_bytes());
        let _ = f.flush();
    }
}

/// Write to stderr and also append the same message to the debug log.
pub fn tee_stderr<S: AsRef<str>>(msg: S) {
    eprintln!("{}", msg.as_ref());
    log_line(msg);
}
