//! Integration tests for `bingle_cli chat` first-run registration (issue #59).
//!
//! The offline cases exercise the compiled binary on the paths that need no blockchain (they hinge
//! on a "no keypair" status, which is resolved without a chain read) and assert exit codes/messages.
//! The live-chain registration cases are `#[ignore]`d: they need a running localnet and a funded
//! account, supplied via the environment, and are run explicitly with `cargo test -- --ignored`.

use std::path::PathBuf;
use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_bingle_cli"))
        .args(args)
        .output()
        .expect("failed to run bingle_cli binary")
}

#[test]
#[cfg(not(target_os = "ios"))]
fn chat_with_empty_state_file_and_no_credentials_exits_2() {
    // A state file that does not exist yet defers the handle, so parsing succeeds and we reach the
    // registration decision: no keypair and no credentials -> NeedCredentials. This resolves as a
    // "no keypair" status without touching the chain, so it is deterministic offline.
    let dir = tempfile::tempdir().expect("tempdir");
    let state = dir.path().join("new.json").to_string_lossy().into_owned();
    let out = run(&["chat", "--state_file", &state]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "no credentials should exit 2; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no registered account"),
        "should explain a registered account is required; got: {stderr}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn chat_with_handle_but_no_passphrase_exits_2() {
    // A handle alone cannot create an account; still needs a funded passphrase (or a saved account).
    let out = run(&["chat", "--handle", "alice"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "handle without passphrase should exit 2; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no registered account"),
        "should point at the missing credentials; got: {stderr}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn chat_help_still_exits_0() {
    let out = run(&["chat", "--help"]);
    assert!(out.status.success(), "chat --help should exit 0");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("Usage: bingle_cli chat"),
        "help should print the chat usage line"
    );
}

/// Read a required environment variable for the live tests, or `None` (so the test no-ops rather
/// than failing) when it is unset.
fn live_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

#[test]
#[ignore = "requires a localnet + funded account; set BINGLE_IT_NODE_FILE/PASSPHRASE/HANDLE and run with --ignored"]
#[cfg(not(target_os = "ios"))]
fn live_first_run_registers_then_second_run_needs_no_passphrase() {
    // Live end-to-end: register on first run, then confirm the saved account starts with no creds.
    // Skips (passes) unless the localnet env vars are provided.
    let (Some(node_file), Some(passphrase), Some(handle)) = (
        live_env("BINGLE_IT_NODE_FILE"),
        live_env("BINGLE_IT_PASSPHRASE"),
        live_env("BINGLE_IT_HANDLE"),
    ) else {
        eprintln!("skipping live registration test: BINGLE_IT_* env not set");
        return;
    };

    let dir = tempfile::tempdir().expect("tempdir");
    let state_file: PathBuf = dir.path().join("chat.state.json");
    let state = state_file.to_string_lossy().into_owned();

    // First run: import + register + save.
    let first = run(&[
        "chat",
        "--state_file",
        &state,
        "--node-file",
        &node_file,
        "--passphrase",
        &passphrase,
        "--handle",
        &handle,
    ]);
    assert!(
        first.status.success(),
        "first-run registration should exit 0; stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(
        state_file.exists(),
        "state file should be written on first run"
    );

    // Second run: no --passphrase/--handle needed.
    let second = run(&["chat", "--state_file", &state, "--node-file", &node_file]);
    assert!(
        second.status.success(),
        "second run should start from the saved account with no credentials; stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
}
