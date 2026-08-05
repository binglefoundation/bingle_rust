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
