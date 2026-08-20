use serde::Serialize;
use std::collections::HashMap;

/// Build and version metadata for a Bingle crate.
///
/// Combines the Cargo package version with the build number and, where available, the git commit
/// (SHA) and build timestamp recorded at compile time.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    /// Full version string, `"{cargo_version}.{build_number}"` (for example `"3.0.1.42"`).
    pub version: String,
    /// Git commit hash (SHA) the crate was built from, if it was available at build time.
    pub git_sha: Option<String>,
    /// Build timestamp recorded at compile time.
    pub build_timestamp: String,
    /// Monotonic build number supplied by the build script.
    pub build_number: String,
}

/// A map from crate/module name to its [`VersionInfo`], used to report versions of several
/// components together.
pub type VersionsMap = HashMap<String, VersionInfo>;

/// Return the [`VersionInfo`] for this crate, assembled from compile-time environment values.
pub fn get_version_info() -> VersionInfo {
    // CARGO_PKG_VERSION is provided by cargo itself
    // BINGLE_BUILD_NUMBER is provided by our build.rs
    let cargo_version = env!("CARGO_PKG_VERSION");
    let build_number = env!("BINGLE_BUILD_NUMBER");

    // Combining them to match the request: 0.1.0.x
    let full_version = format!("{}.{}", cargo_version, build_number);

    tracing::info!("get_version_info - Version: {}", full_version);

    VersionInfo {
        version: full_version,
        git_sha: option_env!("VERGEN_GIT_SHA").map(String::from),
        build_timestamp: env!("VERGEN_BUILD_TIMESTAMP").to_string(),
        build_number: build_number.to_string(),
    }
}

/// Build a [`VersionInfo`](crate::util::version::VersionInfo) for the calling crate from its own
/// compile-time environment.
///
/// Unlike [`get_version_info`](crate::util::version::get_version_info), which reports
/// `bingle_core`'s version, this macro expands in the caller's crate so it captures that crate's
/// `CARGO_PKG_VERSION` and build metadata.
#[macro_export]
macro_rules! get_module_version {
    () => {
        $crate::util::version::VersionInfo {
            version: format!(
                "{}.{}",
                env!("CARGO_PKG_VERSION"),
                option_env!("BINGLE_BUILD_NUMBER").unwrap_or("0")
            ),
            git_sha: option_env!("VERGEN_GIT_SHA").map(String::from),
            build_timestamp: option_env!("VERGEN_BUILD_TIMESTAMP")
                .unwrap_or("unknown")
                .to_string(),
            build_number: option_env!("BINGLE_BUILD_NUMBER")
                .unwrap_or("0")
                .to_string(),
        }
    };
}
