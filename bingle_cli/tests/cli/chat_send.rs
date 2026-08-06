// Unit tests for the outbound send/retry path (bingle_cli::chat_send), using a mock sender so no
// live engine or chain is needed. Mirrors the RN client's policy: transient failures keep retrying
// (with backoff); only non-transient failures — or any failure under --no-retries — are permanent.
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use bingle_cli::chat::parse_chat_args;
use bingle_cli::chat_send::{MessageSender, SendOutcome, SendTarget, retry_pending, send_once};
use bingle_cli::chat_state::ChatState;
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use bingle_local::api::send_retry::RETRY_BACKOFF;
use serde_json::Value;
use tempfile::TempDir;

const RETRIES_ON: bool = true;
const RETRIES_OFF: bool = false;

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

fn bob() -> SendTarget {
    SendTarget::Handle("bob".into())
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn send_once_delivered_marks_message_delivered() {
    let (mut state, _dir) = alice_state();
    let sender = MockSender::always(Ok(true));

    assert_eq!(
        send_once(&sender, &mut state, &bob(), "hi", RETRIES_ON),
        SendOutcome::Delivered
    );
    assert!(state.pending_outbound().expect("pending").is_empty());
    let messages = state.messages().expect("messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].text, "hi");
    assert_eq!(messages[0].progress, Some(1.0));
    assert!(messages[0].failure_reason.is_none());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn transient_failure_stays_pending_and_retrying() {
    let (mut state, _dir) = alice_state();
    // Ok(false) ("Send returned false") is a transient failure.
    let sender = MockSender::always(Ok(false));

    match send_once(&sender, &mut state, &bob(), "hi", RETRIES_ON) {
        SendOutcome::Retrying(reason) => assert!(reason.contains("keep retrying"), "got: {reason}"),
        other => panic!("expected Retrying, got {other:?}"),
    }
    let pending = state.pending_outbound().expect("pending");
    assert_eq!(pending.len(), 1);
    assert!(pending[0].progress.unwrap_or(1.0) < 1.0);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn non_transient_failure_is_permanent() {
    let (mut state, _dir) = alice_state();
    // A non-connectivity error is a permanent failure — not retried.
    let sender = MockSender::always(Err("recipient handle is invalid".into()));

    match send_once(&sender, &mut state, &bob(), "hi", RETRIES_ON) {
        SendOutcome::Failed(reason) => assert!(reason.contains("Message failed to send")),
        other => panic!("expected Failed, got {other:?}"),
    }
    // Marked terminal (progress 1.0), not left pending.
    assert!(state.pending_outbound().expect("pending").is_empty());
    assert_eq!(state.messages().expect("messages")[0].progress, Some(1.0));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_retries_marks_even_transient_failure_permanent() {
    let (mut state, _dir) = alice_state();
    // Transient error, but --no-retries: must be permanent (nothing left pending).
    let sender = MockSender::always(Err("Retryable: relay connect timeout".into()));

    match send_once(&sender, &mut state, &bob(), "hi", RETRIES_OFF) {
        SendOutcome::Failed(_) => {}
        other => panic!("expected Failed under --no-retries, got {other:?}"),
    }
    assert!(state.pending_outbound().expect("pending").is_empty());
    assert_eq!(state.messages().expect("messages")[0].progress, Some(1.0));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn transient_send_then_retry_delivers() {
    let (mut state, _dir) = alice_state();
    // First attempt is a transient error; the background retry then succeeds.
    let sender = MockSender::scripted(vec![Err("Retryable: temporarily offline".into()), Ok(true)]);

    assert!(matches!(
        send_once(&sender, &mut state, &bob(), "hello", RETRIES_ON),
        SendOutcome::Retrying(_)
    ));
    assert_eq!(state.pending_outbound().expect("pending").len(), 1);

    let mut retry_after = std::collections::HashMap::new();
    let outcome = retry_pending(&sender, &mut state, &mut retry_after, Instant::now())
        .expect("a pending message to attempt");
    assert_eq!(outcome.outcome, SendOutcome::Delivered);
    assert_eq!(outcome.recipient, "bob");

    assert!(state.pending_outbound().expect("pending").is_empty());
    assert_eq!(state.messages().expect("messages")[0].progress, Some(1.0));
    assert!(
        state.messages().expect("messages")[0]
            .failure_reason
            .is_none()
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn retry_keeps_transient_pending_forever_with_backoff() {
    let (mut state, _dir) = alice_state();
    let _ts = state.queue_outbound("bob", "hello").expect("queue");
    let sender = MockSender::always(Err("Retryable: still offline".into()));
    let mut retry_after = std::collections::HashMap::new();
    let t0 = Instant::now();

    // First attempt: transient failure → stays pending, backed off (never gives up).
    let first = retry_pending(&sender, &mut state, &mut retry_after, t0).expect("attempt");
    assert!(matches!(first.outcome, SendOutcome::Retrying(_)));
    assert_eq!(state.pending_outbound().expect("pending").len(), 1);

    // Immediately after: the message is backed off, so nothing is eligible.
    assert!(retry_pending(&sender, &mut state, &mut retry_after, t0).is_none());

    // Once the backoff elapses it retries again — and still never permanently fails.
    let t1 = t0 + RETRY_BACKOFF + Duration::from_millis(1);
    let second = retry_pending(&sender, &mut state, &mut retry_after, t1).expect("attempt");
    assert!(matches!(second.outcome, SendOutcome::Retrying(_)));
    assert_eq!(state.pending_outbound().expect("pending").len(), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn retry_marks_non_transient_permanent() {
    let (mut state, _dir) = alice_state();
    let ts = state.queue_outbound("bob", "hello").expect("queue");
    let sender = MockSender::always(Err("account not opted in".into()));
    let mut retry_after = std::collections::HashMap::new();

    let outcome =
        retry_pending(&sender, &mut state, &mut retry_after, Instant::now()).expect("attempt");
    assert!(matches!(outcome.outcome, SendOutcome::Failed(_)));

    // No longer pending; recorded as a permanent failure.
    assert!(state.pending_outbound().expect("pending").is_empty());
    let stored = state
        .messages()
        .expect("messages")
        .into_iter()
        .find(|m| m.timestamp == ts)
        .expect("message present");
    assert_eq!(stored.progress, Some(1.0));
    assert!(stored.failure_reason.is_some());
}
