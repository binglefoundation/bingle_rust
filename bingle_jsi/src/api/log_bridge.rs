use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::api::callback::LogCallback;

/// Global storage for the user-provided log callback.
static GLOBAL_LOG_CALLBACK: OnceLock<Arc<Mutex<Option<Box<dyn LogCallback>>>>> = OnceLock::new();

fn global_callback() -> &'static Arc<Mutex<Option<Box<dyn LogCallback>>>> {
    GLOBAL_LOG_CALLBACK.get_or_init(|| Arc::new(Mutex::new(None)))
}

/// Set (or replace) the global log callback.
pub fn set_global_log_callback(callback: Box<dyn LogCallback>) {
    if let Ok(mut guard) = global_callback().lock() {
        *guard = Some(callback);
    }
}

/// Custom logger that forwards to the registered LogCallback.
struct CallbackLogger;

impl log::Log for CallbackLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if let Ok(guard) = global_callback().lock() {
            if let Some(ref cb) = *guard {
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                let level = record.level().to_string();
                let message = format!("{}", record.args());
                cb.on_log(timestamp, level, message);
            }
        }
    }

    fn flush(&self) {}
}

/// Install the callback logger as the global `log` logger.
///
/// Must be called at most once (subsequent calls are no-ops because
/// `log::set_logger` fails if a logger is already installed).
/// Returns `true` if successfully installed.
pub fn install_log_bridge(level: log::LevelFilter) -> bool {
    // Ensure the global callback storage is initialized
    let _ = global_callback();
    static LOGGER: CallbackLogger = CallbackLogger;
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(level);
        true
    } else {
        false
    }
}
