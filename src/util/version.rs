use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub version: String,
    pub git_sha: Option<String>,
    pub build_timestamp: String,
    pub build_number: String,
}

pub type VersionsMap = HashMap<String, VersionInfo>;

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
