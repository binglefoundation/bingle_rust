//! Logging helpers for the CLI. Only the pure default-level and filter decisions live here (in the
//! lib target) so they can be unit-tested from the test tree; the actual subscriber wiring stays in
//! `main.rs`.

use tracing_subscriber::EnvFilter;
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

/// HTTP client stack targets (the hyper/h2 client under reqwest, used for the indexer and Sidewinder
/// calls). Their per-request detail (connect, frames, connection pooling) is capped at WARN at every
/// level, `--log-level trace` included: the HTTP path is stable and its detail drowned the engine
/// output (issue #402). Only `RUST_LOG` brings it back.
pub const HTTP_STACK_TARGETS: [&str; 4] = ["h2", "hyper", "hyper_util", "reqwest"];

/// The tracing filter directives for the chosen `level`.
///
/// A non-empty `rust_log` (the `RUST_LOG` environment variable) is used as-is, overriding both the
/// level and the HTTP stack cap. Otherwise the directives are `level`, with each of
/// [`HTTP_STACK_TARGETS`] capped at WARN (or at `level`, when that is quieter).
pub fn filter_directives(level: LevelFilter, rust_log: Option<&str>) -> String {
    if let Some(directives) = rust_log.map(str::trim).filter(|d| !d.is_empty()) {
        return directives.to_string();
    }
    let http_level = std::cmp::min(level, LevelFilter::WARN);
    let mut directives = vec![level.to_string().to_ascii_lowercase()];
    directives.extend(
        HTTP_STACK_TARGETS
            .iter()
            .map(|target| format!("{target}={}", http_level.to_string().to_ascii_lowercase())),
    );
    directives.join(",")
}

/// The tracing filter for the chosen `level`, from [`filter_directives`]. Falls back to the plain
/// `level` if `rust_log` does not parse, so a bad `RUST_LOG` never silences the CLI.
pub fn log_filter(level: LevelFilter, rust_log: Option<&str>) -> EnvFilter {
    EnvFilter::try_new(filter_directives(level, rust_log))
        .unwrap_or_else(|_| EnvFilter::new(filter_directives(level, None)))
}
