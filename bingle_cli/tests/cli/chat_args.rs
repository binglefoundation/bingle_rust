// Unit tests for the `chat` subcommand argument parser (bingle_cli::chat::parse_chat_args).
use bingle_cli::chat::parse_chat_args;

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
pub fn logging_flags_are_tolerated_and_not_treated_as_options() {
    // --warn/--info/--debug are consumed globally by init_logger before dispatch, but parse_chat_args
    // must also tolerate them (as no-ops) so they never reach parse_start_options_from_args, which
    // would reject --info/--warn as unknown.
    for flag in ["--debug", "--info", "--warn"] {
        let parsed = parse_chat_args(args(&["alice", flag]))
            .unwrap_or_else(|e| panic!("{flag} should parse: {e}"));
        assert_eq!(parsed.opts.handle, "alice");
    }
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

#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_retries_flag_defaults_off_and_parses() {
    let default = parse_chat_args(args(&["alice"])).expect("parse");
    assert!(!default.no_retries);

    for flag in ["--no-retries", "--no-retry"] {
        let parsed = parse_chat_args(args(&["alice", flag]))
            .unwrap_or_else(|e| panic!("{flag} should parse: {e}"));
        assert!(parsed.no_retries, "{flag} should set no_retries");
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn state_file_allows_missing_handle() {
    // With a state file the handle can come from the stored keypair, so it need not be on the CLI.
    // parse leaves the handle empty; the bridge fills it in from the file.
    let parsed = parse_chat_args(args(&["--state_file", "chat.state", "--to", "bob"]))
        .expect("state file should defer the handle");
    assert_eq!(parsed.opts.handle, "");
    assert_eq!(parsed.state_file.as_deref(), Some("chat.state"));
    assert_eq!(parsed.to.as_deref(), Some("bob"));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn state_file_still_accepts_explicit_handle() {
    let parsed = parse_chat_args(args(&["alice", "--state_file", "chat.state"]))
        .expect("explicit handle with a state file should parse");
    assert_eq!(parsed.opts.handle, "alice");
    assert_eq!(parsed.state_file.as_deref(), Some("chat.state"));
}
