// Unit tests for the outbound send/retry path (bingle_cli::chat_send), using a mock sender so no
// live engine or chain is needed.
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use bingle_cli::chat::parse_chat_args;
use bingle_cli::chat_send::{
    MessageSender, RetryResult, SendReport, SendTarget, retry_pending, send_once,
};
use bingle_cli::chat_state::ChatState;
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use serde_json::Value;
use tempfile::TempDir;

/// A scripted `MessageSender`: pops the next result per call, falling back to `default` when the
/// script is exhausted.
struct MockSender {
    results: Mutex<VecDeque<Result<bool, String>>>,
    default: Result<bool, String>,
}

impl MockSender {
    fn scripted(seq: Vec<Result<bool, String>>) -> Self {
        Self {
            results: Mutex::new(seq.into()),
            default: Ok(true),
        }
    }
    fn always(result: Result<bool, String>) -> Self {
        Self {
            results: Mutex::new(VecDeque::new()),
            default: result,
        }
    }
}

impl MessageSender for MockSender {
    fn send_text(&self, _target: &SendTarget, _message: &Value) -> Result<bool, String> {
        self.results
            .lock()
            .expect("mock lock")
            .pop_front()
            .unwrap_or_else(|| self.default.clone())
    }
}

/// A `ChatState` registered as "alice" over a temp state file, so queue/persist works offline.
fn alice_state() -> (ChatState, TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut local = BingleApiLocalImpl::new(LocalApiConfig::default());
    local.generate_keypair().expect("keypair");
    local.seed_own_handle_for_tests("alice".to_string());
    let path = dir.path().join("state.json").to_string_lossy().into_owned();
    local.save(&path).expect("save");

    let chat_args = parse_chat_args(vec!["--state_file".to_string(), path.clone()]).expect("parse");
    let state = ChatState::from_chat_args(&chat_args).expect("bridge");
    (state, dir)
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn send_once_delivered_marks_message_delivered() {
    let (mut state, _dir) = alice_state();
    let sender = MockSender::always(Ok(true));

    let report = send_once(&sender, &mut state, &SendTarget::Handle("bob".into()), "hi");
    assert_eq!(report, SendReport::Delivered);

    // Nothing left pending; the stored message is delivered (progress 1.0, no failure).
    assert!(state.pending_outbound().expect("pending").is_empty());
    let messages = state.messages().expect("messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].text, "hi");
    assert_eq!(messages[0].progress, Some(1.0));
    assert!(messages[0].failure_reason.is_none());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn send_once_not_accepted_leaves_pending() {
    let (mut state, _dir) = alice_state();
    let sender = MockSender::always(Ok(false));

    let report = send_once(&sender, &mut state, &SendTarget::Handle("bob".into()), "hi");
    match report {
        SendReport::Failed(reason) => assert!(!reason.is_empty()),
        other => panic!("expected Failed, got {other:?}"),
    }

    // The message is pending (progress < 1.0) so the retry worker will pick it up.
    let pending = state.pending_outbound().expect("pending");
    assert_eq!(pending.len(), 1);
    assert!(pending[0].progress.unwrap_or(1.0) < 1.0);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn send_once_error_leaves_pending() {
    let (mut state, _dir) = alice_state();
    let sender = MockSender::always(Err("transport down".to_string()));

    let report = send_once(&sender, &mut state, &SendTarget::Handle("bob".into()), "hi");
    assert_eq!(report, SendReport::Failed("transport down".to_string()));
    assert_eq!(state.pending_outbound().expect("pending").len(), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn failed_send_then_retry_delivers() {
    let (mut state, _dir) = alice_state();
    // First attempt (send_once) errors; the retry then succeeds.
    let sender = MockSender::scripted(vec![Err("temporary".to_string()), Ok(true)]);

    let report = send_once(
        &sender,
        &mut state,
        &SendTarget::Handle("bob".into()),
        "hello",
    );
    assert_eq!(report, SendReport::Failed("temporary".to_string()));
    assert_eq!(state.pending_outbound().expect("pending").len(), 1);

    let mut attempts = HashMap::new();
    let outcomes = retry_pending(&sender, &mut state, &mut attempts, 5);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].result, RetryResult::Delivered);
    assert_eq!(outcomes[0].recipient, "bob");

    // Delivered: nothing pending, message marked delivered.
    assert!(state.pending_outbound().expect("pending").is_empty());
    let messages = state.messages().expect("messages");
    assert_eq!(messages[0].progress, Some(1.0));
    assert!(messages[0].failure_reason.is_none());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn retry_gives_up_after_max_attempts() {
    let (mut state, _dir) = alice_state();
    // Queue a pending message directly, then retry with an always-failing sender.
    let ts = state.queue_outbound("bob", "hello").expect("queue");
    assert_eq!(state.pending_outbound().expect("pending").len(), 1);

    let sender = MockSender::always(Err("still down".to_string()));
    let mut attempts = HashMap::new();

    // max_attempts = 2: first retry keeps it pending, second gives up.
    let first = retry_pending(&sender, &mut state, &mut attempts, 2);
    assert_eq!(
        first[0].result,
        RetryResult::Retrying("still down".to_string())
    );
    assert_eq!(state.pending_outbound().expect("pending").len(), 1);

    let second = retry_pending(&sender, &mut state, &mut attempts, 2);
    assert_eq!(
        second[0].result,
        RetryResult::GaveUp("still down".to_string())
    );

    // Given up: no longer pending; recorded as a permanent failure (progress 1.0 + reason).
    assert!(state.pending_outbound().expect("pending").is_empty());
    let stored = state
        .messages()
        .expect("messages")
        .into_iter()
        .find(|m| m.timestamp == ts)
        .expect("message present");
    assert_eq!(stored.progress, Some(1.0));
    assert_eq!(stored.failure_reason.as_deref(), Some("still down"));
}
