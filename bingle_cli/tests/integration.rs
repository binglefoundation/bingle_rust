//! Integration tests for `bingle_cli chat`: first-run registration (issue #59), engine start-up
//! (issue #60), and the interactive REPL (issue #61).
//!
//! The offline cases exercise the compiled binary on the paths that need no blockchain (they hinge
//! on a "no keypair" status, which is resolved without a chain read) and assert exit codes/messages.
//! The live-chain cases are `#[ignore]`d: they need a running localnet and a funded/registered
//! account, supplied via the environment, and are run explicitly with `cargo test -- --ignored`.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

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

#[test]
#[ignore = "requires a localnet + a pre-registered account; set BINGLE_IT_NODE_FILE and BINGLE_IT_STATE_FILE and run with --ignored"]
#[cfg(not(target_os = "ios"))]
fn live_chat_starts_engine_and_reaches_started() {
    // Starts `chat` against a real node with a pre-registered account (via bingle_cli register /
    // an earlier first run) and asserts it reaches the "started" state within a timeout, then stops
    // it. Verifies the engine start + Ctrl-C shutdown wiring; the full two-peer receive assertion
    // pairs with the send path from the REPL subtask (#61). Skips (passes) unless env is provided.
    let (Some(node_file), Some(state_file)) = (
        live_env("BINGLE_IT_NODE_FILE"),
        live_env("BINGLE_IT_STATE_FILE"),
    ) else {
        eprintln!("skipping live start test: BINGLE_IT_NODE_FILE/STATE_FILE not set");
        return;
    };

    let mut child = Command::new(env!("CARGO_BIN_EXE_bingle_cli"))
        .args([
            "chat",
            "--state_file",
            &state_file,
            "--node-file",
            &node_file,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn chat");

    // Scan stdout for the "started" marker on a background thread so we can time out.
    let stdout = child.stdout.take().expect("child stdout");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.contains("chat: started as") {
                let _ = tx.send(());
                break;
            }
        }
    });

    let started = rx.recv_timeout(Duration::from_secs(90)).is_ok();

    // Best-effort teardown regardless of outcome.
    let _ = child.kill();
    let _ = child.wait();

    assert!(
        started,
        "chat should reach the started/listening state within the timeout"
    );
}

#[test]
#[ignore = "requires a localnet + a registered account + an echo peer; set BINGLE_IT_NODE_FILE, BINGLE_IT_STATE_FILE, BINGLE_IT_ECHO_HANDLE and run with --ignored"]
#[cfg(not(target_os = "ios"))]
fn live_repl_send_to_echo_peer_prints_reply() {
    // Drives the epic's worked example over piped stdin: with an echo peer (`run --echo`) registered
    // as BINGLE_IT_ECHO_HANDLE, send "Hello, echo" then `!exit`, and assert the echoed reply is
    // printed. Skips (passes) unless the localnet env vars are provided.
    let (Some(node_file), Some(state_file), Some(echo_handle)) = (
        live_env("BINGLE_IT_NODE_FILE"),
        live_env("BINGLE_IT_STATE_FILE"),
        live_env("BINGLE_IT_ECHO_HANDLE"),
    ) else {
        eprintln!("skipping live REPL test: BINGLE_IT_* env not set");
        return;
    };

    let mut child = Command::new(env!("CARGO_BIN_EXE_bingle_cli"))
        .args([
            "chat",
            "--state_file",
            &state_file,
            "--node-file",
            &node_file,
            "--to",
            &echo_handle,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn chat");

    // Send a message and then exit. Give the engine a moment to reach listening first.
    {
        let mut stdin = child.stdin.take().expect("child stdin");
        std::thread::sleep(Duration::from_secs(10));
        writeln!(stdin, "Hello, echo").expect("write message");
        std::thread::sleep(Duration::from_secs(5));
        writeln!(stdin, "!exit").expect("write exit");
        // stdin dropped here, closing the pipe.
    }

    let output = child.wait_with_output().expect("wait for chat");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!("{echo_handle}: Echo: Hello, echo")),
        "expected the echoed reply in the transcript; stdout was:\n{stdout}"
    );
}
