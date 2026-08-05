// Unit tests for the `chat` subcommand argument parser (bingle_cli::chat::parse_chat_args).
use bingle_cli::chat::parse_chat_args;
use tracing_subscriber::filter::LevelFilter;

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn parses_handle_passphrase_and_to() {
    let parsed = parse_chat_args(args(&[
        "--handle",
        "alice",
        "--passphrase",
        "secret",
        "--to",
        "bob",
    ]))
    .expect("should parse");
    assert_eq!(parsed.opts.handle, "alice");
    assert_eq!(parsed.opts.algo_passphrase.as_deref(), Some("secret"));
    assert_eq!(parsed.to.as_deref(), Some("bob"));
    // Validate the Option explicitly per guidelines.
    assert!(parsed.to_id.is_none());
    assert!(parsed.state_file.is_none());
    // Default log level is Warn.
    assert_eq!(parsed.log_level, LevelFilter::WARN);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn accepts_positional_handle() {
    let parsed = parse_chat_args(args(&["alice", "--to-id", "PEER123"])).expect("should parse");
    assert_eq!(parsed.opts.handle, "alice");
    assert_eq!(parsed.to_id.as_deref(), Some("PEER123"));
    assert!(parsed.to.is_none());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn accepts_both_state_file_spellings() {
    let underscore =
        parse_chat_args(args(&["alice", "--state_file", "chat.state"])).expect("should parse");
    assert_eq!(underscore.state_file.as_deref(), Some("chat.state"));

    let hyphen =
        parse_chat_args(args(&["alice", "--state-file", "chat.state"])).expect("should parse");
    assert_eq!(hyphen.state_file.as_deref(), Some("chat.state"));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn debug_flag_raises_log_level_to_debug() {
    let parsed = parse_chat_args(args(&["alice", "--debug"])).expect("should parse");
    assert_eq!(parsed.log_level, LevelFilter::DEBUG);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn to_and_to_id_are_mutually_exclusive() {
    let err = parse_chat_args(args(&["alice", "--to", "bob", "--to-id", "PEER123"]))
        .expect_err("conflicting recipient flags should error");
    assert!(
        err.contains("mutually exclusive"),
        "error should explain the conflict; got: {err}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn to_without_value_is_error() {
    let err = parse_chat_args(args(&["alice", "--to"])).expect_err("--to needs a value");
    assert!(
        err.contains("--to"),
        "error should name the flag; got: {err}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn to_id_without_value_is_error() {
    let err = parse_chat_args(args(&["alice", "--to-id"])).expect_err("--to-id needs a value");
    assert!(
        err.contains("--to-id"),
        "error should name the flag; got: {err}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn state_file_without_value_is_error() {
    let err =
        parse_chat_args(args(&["alice", "--state_file"])).expect_err("--state_file needs a value");
    assert!(
        err.contains("--state_file"),
        "error should name the flag; got: {err}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn missing_handle_is_error() {
    let err = parse_chat_args(args(&["--to", "bob"])).expect_err("a handle is required");
    assert!(
        err.to_lowercase().contains("handle"),
        "error should mention the missing handle; got: {err}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn unknown_flag_is_error() {
    let err =
        parse_chat_args(args(&["alice", "--bogus"])).expect_err("unknown flags should be rejected");
    assert!(
        err.contains("--bogus"),
        "error should name the unknown flag; got: {err}"
    );
}
