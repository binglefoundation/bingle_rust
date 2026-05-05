uniffi::setup_scaffolding!();

pub mod api;

use std::sync::Arc;

use api::bingle_jsi_api::BingleJsiApi;
use api::bingle_jsi_api_impl::BingleJsiApiImpl;
use api::callback::LogCallback;
use api::error::BingleJsiError;
use api::types::{BingleJsiConfig, VersionInfo};

/// Create and initialize the Bingle JSI API from a typed configuration object.
///
/// This is the main entry point for React Native / TypeScript consumers.
/// The function name avoids Swift's reserved `init` keyword so that
/// uniffi-bindgen can generate valid Swift bindings.
#[uniffi::export]
pub fn create_bingle_api(config: BingleJsiConfig) -> Result<Arc<dyn BingleJsiApi>, BingleJsiError> {
    let impl_arc = BingleJsiApiImpl::init(config)?;
    Ok(impl_arc as Arc<dyn BingleJsiApi>)
}

/// Return version information without requiring an initialized API instance.
///
/// This can be called before `create_bingle_api` to display version info
/// during app startup.
#[uniffi::export]
pub fn get_version() -> VersionInfo {
    let info = rust_comms::util::version::get_version_info();
    VersionInfo {
        version: info.version,
        git_sha: info.git_sha,
        build_timestamp: info.build_timestamp,
        build_number: info.build_number,
    }
}

/// Register a global log callback without requiring an initialized API instance.
///
/// This can be called before `create_bingle_api` so that log messages
/// emitted during initialization are captured.
///
/// An optional `log_level` may be provided (trace|debug|info|warn|error);
/// if `None`, defaults to "info".
#[uniffi::export]
pub fn set_log_callback_global(callback: Box<dyn LogCallback>, log_level: Option<String>) {
    let level_str = log_level.as_deref().unwrap_or("info");
    let level_filter = match level_str.to_ascii_lowercase().as_str() {
        "trace" => tracing_subscriber::filter::LevelFilter::TRACE,
        "debug" => tracing_subscriber::filter::LevelFilter::DEBUG,
        "info" => tracing_subscriber::filter::LevelFilter::INFO,
        "warn" | "warning" => tracing_subscriber::filter::LevelFilter::WARN,
        "error" => tracing_subscriber::filter::LevelFilter::ERROR,
        _ => tracing_subscriber::filter::LevelFilter::INFO,
    };
    api::log_bridge::install_log_bridge(level_filter);
    api::log_bridge::set_global_log_callback(callback);
}
