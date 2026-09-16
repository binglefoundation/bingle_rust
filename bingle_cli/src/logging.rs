//! Logging helpers for the CLI. Only the pure default-level decision lives here (in the lib target)
//! so it can be unit-tested from the test tree; the actual subscriber wiring stays in `main.rs`.

use tracing_subscriber::filter::LevelFilter;

/// The default tracing level for a subcommand when the user passes no explicit `--log-*` flag.
///
/// `chat` (an interactive REPL) and `checkrelays` (a diagnostic whose result is a printed per-relay
/// summary) default to [`LevelFilter::WARN`] so their real output is not buried under the INFO-level
/// engine tracing; every other subcommand defaults to [`LevelFilter::INFO`]. Override with
/// `--info`/`--debug` (or `--log-level error` for just the result).
pub fn default_log_level(subcommand: Option<&str>) -> LevelFilter {
    match subcommand {
        Some("chat") | Some("checkrelays") => LevelFilter::WARN,
        _ => LevelFilter::INFO,
    }
}
