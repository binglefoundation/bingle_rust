// Unit tests for the CLI default log-level selection (bingle_cli::logging::default_log_level).
use bingle_cli::logging::default_log_level;
use tracing_subscriber::filter::LevelFilter;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn chat_and_checkrelays_default_to_warn() {
    // Both print their own output (REPL prompt / per-relay summary) that would otherwise be buried
    // under INFO engine tracing, so they default to WARN.
    assert_eq!(default_log_level(Some("chat")), LevelFilter::WARN);
    assert_eq!(default_log_level(Some("checkrelays")), LevelFilter::WARN);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn other_subcommands_and_none_default_to_info() {
    for sc in ["run", "register", "migrate", "buybingle", "sellbingle"] {
        assert_eq!(
            default_log_level(Some(sc)),
            LevelFilter::INFO,
            "subcommand {sc} should keep the INFO default"
        );
    }
    assert_eq!(default_log_level(None), LevelFilter::INFO);
}
