// Unit tests for the `logclient` subcommand (bingle_cli::logclient, issue #64).
use std::sync::{Arc, Mutex};

use bingle_cli::chat::{StoreForwardMode, parse_chat_args};
use bingle_cli::chat_receive::receive_message;
use bingle_cli::chat_state::ChatState;
use bingle_cli::logclient::{JsonLinesSink, LogClient, MessageSink, parse_logclient_args};
use bingle_local::api::bingle_local_api::{BingleLocalApi, Message};
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use serde_json::{Value, json};
use tempfile::TempDir;

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// Write a BingleLocal state file registered as `handle` and build a `ChatState` over it. Returns
/// the state and the temp dir (kept alive).
fn state_registered_as(handle: &str) -> (ChatState, TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut local = BingleApiLocalImpl::new(LocalApiConfig::default());
    local.generate_keypair().expect("keypair");
    local.seed_own_handle_for_tests(handle.to_string());
    let path = dir.path().join("state.json").to_string_lossy().into_owned();
    local.save(&path).expect("save");

    let chat_args = parse_chat_args(args(&["--state_file", &path])).expect("parse");
    let state = ChatState::from_chat_args(&chat_args).expect("bridge");
    (state, dir)
}

/// A sink that records what it is given, and can be told to fail.
#[derive(Clone, Default)]
struct RecordingSink {
    delivered: Arc<Mutex<Vec<Message>>>,
    fail: bool,
}

impl MessageSink for RecordingSink {
    fn deliver(&mut self, message: &Message) -> Result<(), String> {
        if self.fail {
            return Err("sink unavailable".to_string());
        }
        self.delivered.lock().expect("lock").push(message.clone());
        Ok(())
    }
}

impl RecordingSink {
    fn texts(&self) -> Vec<String> {
        self.delivered
            .lock()
            .expect("lock")
            .iter()
            .map(|m| m.text.clone())
            .collect()
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn parses_state_file_and_turns_mailbox_receive_on_by_default() {
    let parsed = parse_logclient_args(args(&["--state_file", "support.json"])).expect("parse");
    assert_eq!(parsed.chat.state_file.as_deref(), Some("support.json"));
    assert_eq!(parsed.chat.store_forward, StoreForwardMode::Receive);
    assert!(parsed.chat.to.is_none());
    assert!(parsed.chat.to_id.is_none());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn store_forward_none_turns_mailbox_receive_off() {
    let parsed = parse_logclient_args(args(&[
        "--state_file",
        "support.json",
        "--store-forward",
        "none",
    ]))
    .expect("parse");
    assert_eq!(parsed.chat.store_forward, StoreForwardMode::None);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn store_forward_modes_that_send_are_refused() {
    for mode in ["send", "both"] {
        let err = parse_logclient_args(args(&[
            "--state_file",
            "support.json",
            "--store-forward",
            mode,
        ]))
        .expect_err("a sending mode should be refused");
        assert!(err.contains("receive or none"), "{mode}: {err}");
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn first_run_credentials_and_poll_interval_are_accepted() {
    let parsed = parse_logclient_args(args(&[
        "--state_file",
        "support.json",
        "--handle",
        "support-desk",
        "--passphrase",
        "secret words",
        "--poll-interval",
        "30",
    ]))
    .expect("parse");
    assert_eq!(parsed.chat.opts.handle, "support-desk");
    assert_eq!(
        parsed.chat.opts.algo_passphrase.as_deref(),
        Some("secret words")
    );
    assert_eq!(parsed.chat.poll_interval_secs, Some(30));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn state_file_is_required() {
    let err = parse_logclient_args(args(&["--handle", "support-desk", "--passphrase", "x"]))
        .expect_err("no state file should be an error");
    assert!(err.contains("--state_file"), "{err}");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn sending_and_prompt_flags_are_refused() {
    for flag in [
        &["--to", "bob"][..],
        &["--to-id", "PEER"][..],
        &["--no-retries"][..],
        &["--notify", "https://notify.example"][..],
        &["--plain"][..],
    ] {
        let mut items = vec!["--state_file", "support.json"];
        items.extend_from_slice(flag);
        let err = parse_logclient_args(args(&items)).expect_err("flag should be refused");
        assert!(
            err.contains("not supported by logclient"),
            "{flag:?}: {err}"
        );
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn json_lines_sink_writes_one_line_per_message_even_with_line_breaks() {
    let (mut state, _dir) = state_registered_as("support-desk");
    let message = json!({ "text": "first line\nsecond line", "sent_time": 1_700_000_000_123i64 });
    let stored = receive_message(&mut state, "PEER_BOB", "bob", &message).expect("accepted");

    let mut sink = JsonLinesSink::new(Vec::new());
    sink.deliver(&stored).expect("deliver");
    let written = String::from_utf8(sink.into_inner()).expect("utf8");

    assert_eq!(written.lines().count(), 1, "{written}");
    let line: Value = serde_json::from_str(written.trim_end()).expect("json");
    assert_eq!(line["id"], json!(stored.id));
    assert_eq!(line["sender"], json!("bob"));
    assert_eq!(line["sent_time"], json!(1_700_000_000_123i64));
    assert_eq!(line["text"], json!("first line\nsecond line"));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_received_text_message_is_delivered_once_and_kept_in_history() {
    let (state, _dir) = state_registered_as("support-desk");
    let sink = RecordingSink::default();
    let mut client = LogClient::new(state, Box::new(sink.clone()));

    assert!(client.receive("PEER_BOB", "bob", &json!({ "text": "help please" })));

    assert_eq!(sink.texts(), vec!["help please".to_string()]);
    assert_eq!(client.delivered(), 1);
    let history = client.state().messages().expect("messages");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].sender_handle, "bob");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_second_copy_of_a_message_is_not_delivered_again() {
    let (state, _dir) = state_registered_as("support-desk");
    let sink = RecordingSink::default();
    let mut client = LogClient::new(state, Box::new(sink.clone()));
    let message = json!({ "text": "help please", "sent_time": 1_700_000_000_000i64 });

    assert!(client.receive("PEER_BOB", "bob", &message));
    assert!(!client.receive("PEER_BOB", "bob", &message));

    assert_eq!(sink.texts().len(), 1);
    assert_eq!(client.delivered(), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn protocol_messages_are_not_delivered() {
    let (state, _dir) = state_registered_as("support-desk");
    let sink = RecordingSink::default();
    let mut client = LogClient::new(state, Box::new(sink.clone()));

    assert!(!client.receive("PEER_BOB", "bob", &json!({ "app": "ping", "text": "x" })));
    assert!(!client.receive("PEER_BOB", "bob", &json!({ "no_text": true })));

    assert!(sink.texts().is_empty());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_failed_delivery_is_not_counted_and_the_message_stays_in_history() {
    let (state, _dir) = state_registered_as("support-desk");
    let sink = RecordingSink {
        fail: true,
        ..RecordingSink::default()
    };
    let mut client = LogClient::new(state, Box::new(sink));

    assert!(!client.receive("PEER_BOB", "bob", &json!({ "text": "help please" })));

    assert_eq!(client.delivered(), 0);
    assert_eq!(client.state().messages().expect("messages").len(), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn mailbox_batches_are_delivered_in_order() {
    // Build two stored messages through the receive path of a separate state, standing in for a
    // Mailbox poll's already-recorded batch.
    let (mut source, _source_dir) = state_registered_as("support-desk");
    let first =
        receive_message(&mut source, "PEER_BOB", "bob", &json!({ "text": "one" })).expect("first");
    let second = receive_message(
        &mut source,
        "PEER_CAROL",
        "carol",
        &json!({ "text": "two" }),
    )
    .expect("second");

    let (state, _dir) = state_registered_as("support-desk");
    let sink = RecordingSink::default();
    let mut client = LogClient::new(state, Box::new(sink.clone()));

    assert_eq!(client.deliver_polled(&[first, second]), 2);
    assert_eq!(sink.texts(), vec!["one".to_string(), "two".to_string()]);
    assert_eq!(client.delivered(), 2);
}

/// Run the compiled binary with `args`.
fn run_binary(items: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_bingle_cli"))
        .args(items)
        .output()
        .expect("failed to run bingle_cli binary")
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn logclient_help_prints_usage_to_stdout() {
    let out = run_binary(&["logclient", "--help"]);
    assert!(out.status.success(), "logclient --help should exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Usage: bingle_cli logclient"),
        "help should be on stdout; got: {stdout}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn logclient_usage_error_keeps_stdout_empty() {
    // stdout is reserved for messages: the error, the usage text and the startup log line all go to
    // stderr.
    let out = run_binary(&["logclient", "--handle", "support-desk"]);
    assert_eq!(out.status.code(), Some(2), "a usage error should exit 2");
    assert!(
        out.stdout.is_empty(),
        "stdout should be empty; got: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--state_file"), "stderr: {stderr}");
    assert!(
        stderr.contains("Bingle CLI starting"),
        "the startup log line should be on stderr: {stderr}"
    );
}
