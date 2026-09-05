//! Shared utilities: logging configuration, version metadata, and command-line/configuration
//! helpers used across the Bingle crates.

// Internal helpers reachable for the test tree and workspace crates, not a supported API.
#[doc(hidden)]
pub mod arc_retry;
#[doc(hidden)]
pub mod printing;
/// Logging configuration and formatting for Bingle.
#[macro_use]
pub mod logging;
#[doc(hidden)]
pub mod cli;
/// Helpers for parsing Bingle command-line arguments.
pub mod cli_utils;
/// Helpers for loading and resolving Bingle configuration (node files, app/asset ids).
pub mod config_utils;
#[doc(hidden)]
pub mod net_det;
/// Wall-clock helpers ([`time::now_millis`]).
pub mod time;
/// Build and version metadata, [`version::VersionInfo`].
pub mod version;
