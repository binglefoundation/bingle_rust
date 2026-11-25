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
/// The host's own IPv6 addresses, for telling a translated STUN-mapped address from a native one
/// (issue #498).
pub mod local_addrs;
/// NAT64 prefix discovery and RFC 6052 address translation (issue #496).
pub mod nat64;
#[doc(hidden)]
pub mod net_det;
/// Background loop that runs a pass once per interval ([`periodic_task::PeriodicTask`]).
pub mod periodic_task;
/// The STUN server list: built-in default, configured override and resolution (issue #496).
pub mod stun_servers;
/// Wall-clock helpers ([`time::now_millis`]).
pub mod time;
/// Build and version metadata, [`version::VersionInfo`].
pub mod version;
