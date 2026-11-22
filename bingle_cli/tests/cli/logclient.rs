// Unit tests for the `logclient` subcommand (bingle_cli::logclient, issue #64).
use std::sync::{Arc, Mutex};

use bingle_cli::chat::{StoreForwardMode, parse_chat_args};
use bingle_cli::chat_receive::receive_message;
use bingle_cli::chat_state::ChatState;
use bingle_cli::logclient::{
    JsonLinesSink, LogClient, MaildirSink, MessageSink, parse_logclient_args,
};
use bingle_cli::spam_limit::{SPAM_LIMIT_RECORD, SpamLimiter};
use bingle_local::api::bingle_local_api::{BingleLocalApi, Message};
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::spam_limit::{TestClock, utc_millis};

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

/// Each message a [`RecordingSink`] was given, with its sender id.
type Recorded = Arc<Mutex<Vec<(Message, Option<String>)>>>;

/// A sink that records what it is given (with the sender id), and can be told to fail.
#[derive(Clone, Default)]
struct RecordingSink {
    delivered: Recorded,
    fail: bool,
}

impl MessageSink for RecordingSink {
    fn deliver(&mut self, message: &Message, sender_id: Option<&str>) -> Result<(), String> {
        if self.fail {
            return Err("sink unavailable".to_string());
        }
        self.delivered
            .lock()
            .expect("lock")
            .push((message.clone(), sender_id.map(str::to_string)));
        Ok(())
    }
}

impl RecordingSink {
    fn texts(&self) -> Vec<String> {
        self.delivered
            .lock()
            .expect("lock")
            .iter()
            .map(|(m, _)| m.text.clone())
            .collect()
    }

    fn sender_ids(&self) -> Vec<Option<String>> {
        self.delivered
            .lock()
            .expect("lock")
            .iter()
            .map(|(_, id)| id.clone())
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
    sink.deliver(&stored, Some("PEER_BOB")).expect("deliver");
    let written = String::from_utf8(sink.into_inner()).expect("utf8");

    assert_eq!(written.lines().count(), 1, "{written}");
    let line: Value = serde_json::from_str(written.trim_end()).expect("json");
    assert_eq!(line["id"], json!(stored.id));
    assert_eq!(line["sender"], json!("bob"));
    assert_eq!(line["sender_id"], json!("PEER_BOB"));
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

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_real_time_message_is_delivered_with_its_sender_id() {
    let (state, _dir) = state_registered_as("support-desk");
    let sink = RecordingSink::default();
    let mut client = LogClient::new(state, Box::new(sink.clone()));

    assert!(client.receive("PEER_BOB", "bob", &json!({ "text": "help please" })));

    assert_eq!(sink.sender_ids(), vec![Some("PEER_BOB".to_string())]);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_mailbox_message_gets_the_sender_id_only_for_a_known_contact() {
    let (mut source, _source_dir) = state_registered_as("support-desk");
    let from_contact =
        receive_message(&mut source, "PEER_BOB", "bob", &json!({ "text": "one" })).expect("first");
    let from_stranger = receive_message(
        &mut source,
        "PEER_CAROL",
        "carol",
        &json!({ "text": "two" }),
    )
    .expect("second");

    let (state, _dir) = state_registered_as("support-desk");
    let sink = RecordingSink::default();
    let mut client = LogClient::new(state, Box::new(sink.clone()));
    // bob becomes a contact by messaging in real time first.
    assert!(client.receive("PEER_BOB", "bob", &json!({ "text": "earlier" })));

    client.deliver_polled(&[from_contact, from_stranger]);

    assert_eq!(
        sink.sender_ids(),
        vec![
            Some("PEER_BOB".to_string()),
            Some("PEER_BOB".to_string()),
            None
        ]
    );
}

/// A sink that checks, at the moment it is called, whether the message is already in the saved
/// state file.
struct SavedYetSink {
    state_file: String,
    saved_before_delivery: Arc<Mutex<Vec<bool>>>,
}

impl MessageSink for SavedYetSink {
    fn deliver(&mut self, message: &Message, _sender_id: Option<&str>) -> Result<(), String> {
        let saved = std::fs::read_to_string(&self.state_file).unwrap_or_default();
        self.saved_before_delivery
            .lock()
            .expect("lock")
            .push(saved.contains(&message.text));
        Ok(())
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_message_is_handed_on_before_the_state_file_is_saved() {
    // Issue #65: a crash between saving and handing on would otherwise leave a message in the saved
    // history that was never forwarded.
    let (state, _dir) = state_registered_as("support-desk");
    let state_file = state.state_file_path().expect("state file");
    let saved_before_delivery = Arc::new(Mutex::new(Vec::new()));
    let sink = SavedYetSink {
        state_file: state_file.clone(),
        saved_before_delivery: saved_before_delivery.clone(),
    };
    let mut client = LogClient::new(state, Box::new(sink));

    assert!(client.receive(
        "PEER_BOB",
        "bob",
        &json!({ "text": "a distinctive message" })
    ));

    assert_eq!(*saved_before_delivery.lock().expect("lock"), vec![false]);
    let saved = std::fs::read_to_string(&state_file).expect("read state file");
    assert!(
        saved.contains("a distinctive message"),
        "the message should be saved after delivery"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn maildir_and_from_are_parsed_together() {
    let parsed = parse_logclient_args(args(&[
        "--state_file",
        "support.json",
        "--maildir",
        "/var/spool/support",
        "--from",
        "support@example.com",
    ]))
    .expect("parse");
    assert_eq!(parsed.maildir.as_deref(), Some("/var/spool/support"));
    assert_eq!(parsed.from.as_deref(), Some("support@example.com"));

    let plain = parse_logclient_args(args(&["--state_file", "support.json"])).expect("parse");
    assert!(plain.maildir.is_none());
    assert!(plain.from.is_none());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn maildir_without_from_and_from_without_maildir_are_errors() {
    let err = parse_logclient_args(args(&["--state_file", "s.json", "--maildir", "/tmp/m"]))
        .expect_err("--maildir alone should fail");
    assert!(err.contains("--from"), "{err}");
    let err = parse_logclient_args(args(&["--state_file", "s.json", "--from", "a@example.com"]))
        .expect_err("--from alone should fail");
    assert!(err.contains("--maildir"), "{err}");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn an_invalid_from_address_is_an_error() {
    let err = parse_logclient_args(args(&[
        "--state_file",
        "s.json",
        "--maildir",
        "/tmp/m",
        "--from",
        "not an address",
    ]))
    .expect_err("bad address should fail");
    assert!(err.contains("--from expects an email address"), "{err}");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_received_message_lands_in_the_maildir_as_one_email() {
    let (state, _dir) = state_registered_as("support-desk");
    let maildir = tempfile::tempdir().expect("maildir");
    let sink = MaildirSink::open(maildir.path(), "support@example.com").expect("open sink");
    let mut client = LogClient::new(state, Box::new(sink));

    assert!(client.receive("PEER_BOB", "bob", &json!({ "text": "help please" })));

    let new: Vec<_> = std::fs::read_dir(maildir.path().join("new"))
        .expect("read new/")
        .collect();
    assert_eq!(new.len(), 1);
    let path = new[0].as_ref().expect("entry").path();
    let email = std::fs::read_to_string(path).expect("read email");
    assert!(email.contains("X-Bingle-Sender: PEER_BOB\r\n"), "{email}");
    assert!(email.ends_with("\r\n\r\nhelp please\r\n"), "{email}");
    assert_eq!(
        std::fs::read_dir(maildir.path().join("tmp"))
            .expect("read tmp/")
            .count(),
        0
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn spam_limit_is_parsed_and_off_by_default() {
    let parsed = parse_logclient_args(args(&["--state_file", "s.json", "--spam-limit", "20"]))
        .expect("parse");
    assert_eq!(parsed.spam_limit, Some(20));
    let parsed = parse_logclient_args(args(&["--state_file", "s.json"])).expect("parse");
    assert_eq!(parsed.spam_limit, None);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_spam_limit_that_is_not_a_positive_whole_number_is_an_error() {
    for bad in ["0", "-1", "ten", "1.5"] {
        let err = parse_logclient_args(args(&["--state_file", "s.json", "--spam-limit", bad]))
            .expect_err(bad);
        assert!(err.contains("--spam-limit"), "{err}");
    }
    let err = parse_logclient_args(args(&["--state_file", "s.json", "--spam-limit"]))
        .expect_err("missing value");
    assert!(err.contains("--spam-limit"), "{err}");
}

/// A client over a fresh registered state with a spam limit of `limit`, at a fixed time.
fn limited_client(limit: u32, sink: &RecordingSink) -> (LogClient, TempDir) {
    let (state, dir) = state_registered_as("support-desk");
    let clock = TestClock::at(utc_millis(2026, 11, 21, 12, 0, 0));
    let client = LogClient::new(state, Box::new(sink.clone()))
        .with_spam_limit(SpamLimiter::new(limit, Box::new(clock)));
    (client, dir)
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_message_over_the_spam_limit_is_kept_in_history_but_not_handed_on() {
    let sink = RecordingSink::default();
    let (mut client, _dir) = limited_client(2, &sink);

    assert!(client.receive("PEER_BOB", "bob", &json!({ "text": "one" })));
    assert!(client.receive("PEER_BOB", "bob", &json!({ "text": "two" })));
    assert!(!client.receive("PEER_BOB", "bob", &json!({ "text": "three" })));
    assert!(client.receive("PEER_CAROL", "carol", &json!({ "text": "hello" })));

    assert_eq!(sink.texts(), vec!["one", "two", "hello"]);
    assert_eq!(client.delivered(), 3);
    assert_eq!(client.suppressed(), 1);
    let history: Vec<String> = client
        .state()
        .messages()
        .expect("messages")
        .into_iter()
        .map(|m| m.text)
        .collect();
    assert!(history.contains(&"three".to_string()), "{history:?}");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn the_spam_limit_counts_survive_a_restart() {
    let sink = RecordingSink::default();
    let (mut client, dir) = limited_client(2, &sink);
    assert!(client.receive("PEER_BOB", "bob", &json!({ "text": "one" })));
    assert!(client.receive("PEER_BOB", "bob", &json!({ "text": "two" })));
    drop(client);

    // A new run over the same state file, later the same day.
    let path = dir.path().join("state.json").to_string_lossy().into_owned();
    let chat_args = parse_chat_args(args(&["--state_file", &path])).expect("parse");
    let state = ChatState::from_chat_args(&chat_args).expect("bridge");
    assert!(state.app_record(SPAM_LIMIT_RECORD).expect("read").is_some());
    let clock = TestClock::at(utc_millis(2026, 11, 21, 18, 0, 0));
    let mut client = LogClient::new(state, Box::new(sink.clone()))
        .with_spam_limit(SpamLimiter::new(2, Box::new(clock)));

    assert!(!client.receive("PEER_BOB", "bob", &json!({ "text": "three" })));
    assert_eq!(sink.texts(), vec!["one", "two"]);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_mailbox_sender_without_a_known_id_is_limited_by_handle() {
    let (mut source, _source_dir) = state_registered_as("support-desk");
    let read: Vec<Message> = ["one", "two"]
        .iter()
        .map(|text| {
            receive_message(&mut source, "PEER_BOB", "bob", &json!({ "text": text }))
                .expect("stored")
        })
        .collect();

    let sink = RecordingSink::default();
    let (mut client, _dir) = limited_client(1, &sink);

    assert_eq!(client.deliver_polled(&read), 1);
    assert_eq!(sink.texts(), vec!["one"]);
    assert_eq!(client.limiter().expect("limiter").count("bob"), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn without_a_spam_limit_every_message_is_handed_on() {
    let (state, _dir) = state_registered_as("support-desk");
    let sink = RecordingSink::default();
    let mut client = LogClient::new(state, Box::new(sink.clone()));
    for text in ["one", "two", "three", "four"] {
        assert!(client.receive("PEER_BOB", "bob", &json!({ "text": text })));
    }
    assert_eq!(client.suppressed(), 0);
    assert!(client.limiter().is_none());
}
