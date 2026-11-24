// Exercises the compiled bingle_cli binary to verify that `-V`/`--version` and `-h`/`--help`
// print to stdout and exit successfully, while a missing subcommand is an error on stderr.
use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_bingle_cli"))
        .args(args)
        .output()
        .expect("failed to run bingle_cli binary")
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn version_long_flag_prints_to_stdout() {
    let out = run(&["--version"]);
    assert!(out.status.success(), "--version should exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("bingle_cli"),
        "version output should name the binary; got: {stdout}"
    );
    assert!(
        stdout.contains("bingle_core"),
        "version output should include the core version; got: {stdout}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn version_short_flag_matches_long_flag() {
    let short = run(&["-V"]);
    let long = run(&["--version"]);
    assert!(short.status.success(), "-V should exit 0");
    assert_eq!(
        short.stdout, long.stdout,
        "-V and --version should produce identical output"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn help_long_flag_prints_usage_to_stdout() {
    let out = run(&["--help"]);
    assert!(out.status.success(), "--help should exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Usage: bingle_cli"),
        "help output should contain the usage line on stdout; got: {stdout}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn help_short_flag_prints_usage_to_stdout() {
    let out = run(&["-h"]);
    assert!(out.status.success(), "-h should exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Usage: bingle_cli"),
        "help output should contain the usage line on stdout; got: {stdout}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_args_is_usage_error_on_stderr() {
    let out = run(&[]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "running with no arguments should exit 2"
    );
    // The usage error is written to stderr (the logger's operational output uses stdout).
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Usage: bingle_cli"),
        "usage error should be written to stderr; got: {stderr}"
    );
}

// Issue #484: `chat --help` lists --poll-interval and explains its default and 0, and the
// top-level usage lists it on the chat line.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn chat_help_lists_poll_interval() {
    let out = run(&["chat", "--help"]);
    assert!(out.status.success(), "chat --help should exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("[--poll-interval <secs>]"),
        "chat usage should list --poll-interval; got: {stdout}"
    );
    assert!(
        stdout.contains("--poll-interval <secs>: with --store-forward receive or both")
            && stdout.contains("(default 20)")
            && stdout.contains("0 reads it on start only"),
        "chat help should explain --poll-interval; got: {stdout}"
    );

    let top = run(&["--help"]);
    let top = String::from_utf8_lossy(&top.stdout);
    let chat_line = top
        .lines()
        .find(|l| l.trim_start().starts_with("bingle_cli chat "))
        .expect("top-level usage has a chat line");
    assert!(
        chat_line.contains("[--poll-interval <secs>]"),
        "top-level chat line should list --poll-interval; got: {chat_line}"
    );
}
