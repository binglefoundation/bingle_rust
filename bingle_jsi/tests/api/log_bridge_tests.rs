use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use bingle_jsi::api::callback::LogCallback;
use bingle_jsi::api::log_bridge::{install_log_bridge, set_global_log_callback};

struct CountingLogCallback {
    count: Arc<AtomicU32>,
}

impl LogCallback for CountingLogCallback {
    fn on_log(&self, timestamp: i64, level: String, message: String) {
        assert!(timestamp > 0, "timestamp should be positive");
        assert!(!level.is_empty(), "level should not be empty");
        assert!(!message.is_empty(), "message should not be empty");
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn log_bridge_forwards_to_callback() {
    // install_log_bridge may fail if another test already installed a logger;
    // that's fine — what matters is set_global_log_callback + tracing::info! works.
    let _ = install_log_bridge(tracing_subscriber::filter::LevelFilter::TRACE);

    let count = Arc::new(AtomicU32::new(0));
    let cb = CountingLogCallback {
        count: count.clone(),
    };
    set_global_log_callback(Box::new(cb));

    tracing::info!("test log message");

    // The callback should have been invoked at least once
    assert!(
        count.load(Ordering::SeqCst) >= 1,
        "log callback should have been called"
    );
}

#[test]
fn log_bridge_no_callback_does_not_panic() {
    // With no callback set, logging should not panic
    let _ = install_log_bridge(tracing_subscriber::filter::LevelFilter::TRACE);
    tracing::info!("this should not panic even without a callback");
}

#[test]
fn set_log_callback_global_installs_and_forwards() {
    let count = Arc::new(AtomicU32::new(0));
    let cb = CountingLogCallback {
        count: count.clone(),
    };
    // The free function should install the bridge and set the callback
    bingle_jsi::set_log_callback_global(Box::new(cb), Some("trace".to_string()));

    tracing::info!("message via global free function");

    assert!(
        count.load(Ordering::SeqCst) >= 1,
        "global log callback should have been called"
    );
}

#[test]
fn get_version_returns_valid_info() {
    let info = bingle_jsi::get_version();
    assert!(!info.version.is_empty(), "version should not be empty");
    assert!(
        !info.build_timestamp.is_empty(),
        "build_timestamp should not be empty"
    );
    assert!(
        !info.build_number.is_empty(),
        "build_number should not be empty"
    );
}
