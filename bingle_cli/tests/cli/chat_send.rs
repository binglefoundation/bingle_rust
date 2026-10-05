// Unit tests for the chat send path (bingle_cli::chat_send) on the shared pending-message sender
// (issue #283), using a mock delivery so no live engine or chain is needed. The sender's own retry
// and scheduling behaviour is covered in bingle_local; these cover the chat wiring: queueing from a
// ChatState, the store handle, transcript lines and label routing.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use bingle_cli::chat::parse_chat_args;
use bingle_cli::chat_send::{SendTarget, is_account_id, report_line, shutdown_lines};
use bingle_cli::chat_state::ChatState;
use bingle_core::api::bingle_api::{BingleError, ProgressCallback, SendFailureKind, StartOptions};
use bingle_local::api::MailboxConfig;
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use bingle_local::api::pending_sender::{
    MessageDelivery, NotSentReason, PendingSender, PendingSenderOptions, SendOutcome, SendReport,
    ShutdownEntry, ShutdownReport,
};
use serde_json::Value;
use tempfile::TempDir;

const REPORT_TIMEOUT: Duration = Duration::from_secs(5);

/// How the mock answers every delivery.
#[derive(Clone, Copy)]
enum Answer {
    Delivered,
    Transient,
    Permanent,
}

/// A `MessageDelivery` that always gives the same answer and counts calls.
struct MockDelivery {
    answer: Answer,
    calls: AtomicUsize,
}

impl MockDelivery {
    fn new(answer: Answer) -> Arc<Self> {
        Arc::new(Self {
            answer,
            calls: AtomicUsize::new(0),
        })
    }
}

impl MessageDelivery for MockDelivery {
    fn deliver(
        &self,
        _recipient: &str,
        _message: Value,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.answer {
            Answer::Delivered => Ok(true),
            Answer::Transient => Err(BingleError::Send {
                kind: SendFailureKind::PeerUnreachable,
                detail: "peer offline".to_string(),
            }),
            Answer::Permanent => Err(BingleError::Send {
                kind: SendFailureKind::HandleNotFound,
                detail: "no such handle".to_string(),
            }),
        }
    }
}

/// Start a sender over `state`'s store, returning it with a receiver of its reports.
fn start_sender(
    state: &ChatState,
    delivery: Arc<MockDelivery>,
    retries_enabled: bool,
) -> (PendingSender, mpsc::Receiver<SendReport>) {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let sender = PendingSender::start(
        Arc::new(state.outbound_store()),
        delivery,
        PendingSenderOptions {
            tick: Duration::from_millis(20),
            retries_enabled,
            ..PendingSenderOptions::new()
        },
        Arc::new(|| true),
        Arc::new(move |report| {
            let _ = tx.lock().expect("report tx").send(report);
        }),
    );
    (sender, rx)
}

fn report(outcome: SendOutcome, previously_failed: bool) -> SendReport {
    SendReport {
        id: "m1".to_string(),
        recipients: vec!["bob".to_string()],
        outcome,
        previously_failed,
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

/// A `ChatState` registered as "alice" whose local store has the store-and-forward SEND gate set as
/// given and (optionally) a Sidewinder Mailbox configured. Built directly from parts so a test can
/// exercise the forward-on-give-up path with an arbitrary Mailbox config, without the state-file
/// bridge's `validate_store_and_forward` (which rejects a gate with no Mailbox). Store is in-memory.
fn alice_state_gated(send_gate: bool, sidewinder: Option<MailboxConfig>) -> ChatState {
    let mut local = BingleApiLocalImpl::new(LocalApiConfig {
        sidewinder,
        store_and_forward_send: send_gate,
        ..LocalApiConfig::default()
    });
    local.generate_keypair().expect("keypair");
    local.seed_own_handle_for_tests("alice".to_string());
    ChatState::from_parts_for_tests(local, StartOptions::new("alice".to_string()))
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn queued_message_is_delivered_and_saved() {
    let (mut state, dir) = alice_state();
    let ts = state.queue_outbound("bob", "hi").expect("queue");
    let (sender, rx) = start_sender(&state, MockDelivery::new(Answer::Delivered), true);
    sender.wake();

    let report = rx.recv_timeout(REPORT_TIMEOUT).expect("report");
    assert_eq!(report.id, ts);
    assert_eq!(report.outcome, SendOutcome::Delivered);
    assert!(state.pending_outbound().expect("pending").is_empty());

    // Saved to the state file by the sender's store handle.
    let path = dir.path().join("state.json").to_string_lossy().into_owned();
    let mut reloaded = BingleApiLocalImpl::new(LocalApiConfig::default());
    reloaded.load(&path).expect("reload");
    let stored = reloaded
        .get_messages()
        .expect("messages")
        .into_iter()
        .find(|m| m.id == ts)
        .expect("message saved");
    assert_eq!(stored.progress, Some(1.0));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn transient_failure_stays_pending() {
    let (mut state, _dir) = alice_state();
    state.queue_outbound("bob", "hi").expect("queue");
    let (_sender, rx) = start_sender(&state, MockDelivery::new(Answer::Transient), true);

    let report = rx.recv_timeout(REPORT_TIMEOUT).expect("report");
    match &report.outcome {
        SendOutcome::Retrying(reason) => assert!(reason.contains("keep retrying"), "got: {reason}"),
        other => panic!("expected Retrying, got {other:?}"),
    }
    assert_eq!(state.pending_outbound().expect("pending").len(), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn permanent_failure_is_terminal() {
    let (mut state, _dir) = alice_state();
    state.queue_outbound("bob", "hi").expect("queue");
    let (_sender, rx) = start_sender(&state, MockDelivery::new(Answer::Permanent), true);

    let report = rx.recv_timeout(REPORT_TIMEOUT).expect("report");
    assert!(
        matches!(report.outcome, SendOutcome::Failed(_)),
        "{report:?}"
    );
    assert!(state.pending_outbound().expect("pending").is_empty());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_retries_marks_a_transient_failure_terminal() {
    let (mut state, _dir) = alice_state();
    state.queue_outbound("bob", "hi").expect("queue");
    let (_sender, rx) = start_sender(&state, MockDelivery::new(Answer::Transient), false);

    let report = rx.recv_timeout(REPORT_TIMEOUT).expect("report");
    assert!(
        matches!(report.outcome, SendOutcome::Failed(_)),
        "{report:?}"
    );
    assert!(state.pending_outbound().expect("pending").is_empty());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn send_gate_on_failed_send_forwards_to_mailbox() {
    // Issue #272: with the store-and-forward SEND gate on, a failed direct send to an offline
    // recipient is handed to their Mailbox. Pre-mark it posted (test seam) so no node is needed.
    let mut state = alice_state_gated(true, Some(MailboxConfig::new("http://localhost:9", "tok")));
    let ts = state
        .queue_outbound("bob", "hi while offline")
        .expect("queue");
    state.mark_forwarded_for_tests(&ts, "bob");
    let (_sender, rx) = start_sender(&state, MockDelivery::new(Answer::Transient), true);

    let report = rx.recv_timeout(REPORT_TIMEOUT).expect("report");
    assert!(
        matches!(report.outcome, SendOutcome::Forwarded(_)),
        "{report:?}"
    );
    assert!(state.pending_outbound().expect("pending").is_empty());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn send_gate_on_but_forward_incomplete_falls_back_to_retry() {
    let mut state = alice_state_gated(true, None);
    state.queue_outbound("bob", "hi").expect("queue");
    let (_sender, rx) = start_sender(&state, MockDelivery::new(Answer::Transient), true);

    let report = rx.recv_timeout(REPORT_TIMEOUT).expect("report");
    assert!(
        matches!(report.outcome, SendOutcome::Retrying(_)),
        "{report:?}"
    );
    assert_eq!(state.pending_outbound().expect("pending").len(), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn sending_does_not_need_the_session_lock() {
    // The REPL, receive callback and sender share the ChatState behind a mutex; the sender uses its
    // own store handle, so a send completes while that mutex is held.
    let (mut state, _dir) = alice_state();
    let ts = state.queue_outbound("bob", "hi").expect("queue");
    let store = state.outbound_store();
    let shared = Mutex::new(state);
    let held = shared.lock().expect("session lock");

    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let _sender = PendingSender::start(
        Arc::new(store),
        MockDelivery::new(Answer::Delivered),
        PendingSenderOptions {
            tick: Duration::from_millis(20),
            ..PendingSenderOptions::new()
        },
        Arc::new(|| true),
        Arc::new(move |report: SendReport| {
            let _ = tx.lock().expect("report tx").send(report);
        }),
    );
    let report = rx
        .recv_timeout(REPORT_TIMEOUT)
        .expect("report while locked");
    assert_eq!(report.id, ts);
    assert_eq!(report.outcome, SendOutcome::Delivered);
    drop(held);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn messages_queued_in_the_same_millisecond_get_distinct_timestamps() {
    let (mut state, _dir) = alice_state();
    let a = state.queue_outbound("bob", "one").expect("queue");
    let b = state.queue_outbound("bob", "two").expect("queue");
    let c = state.queue_outbound("bob", "three").expect("queue");
    assert!(a != b && b != c && a != c);
    assert_eq!(state.pending_outbound().expect("pending").len(), 3);
}

#[test]
pub fn report_line_is_quiet_for_a_first_time_delivery() {
    assert_eq!(report_line(&report(SendOutcome::Delivered, false)), None);
}

#[test]
pub fn report_line_announces_a_delivery_after_a_failure() {
    assert_eq!(
        report_line(&report(SendOutcome::Delivered, true)).as_deref(),
        Some("✓ delivered to bob")
    );
}

#[test]
pub fn report_line_reports_only_the_first_transient_failure() {
    let first = report_line(&report(SendOutcome::Retrying("offline".into()), false));
    assert_eq!(
        first.as_deref(),
        Some("! send to bob not delivered (offline); will keep retrying…")
    );
    assert_eq!(
        report_line(&report(SendOutcome::Retrying("offline".into()), true)),
        None
    );
}

#[test]
pub fn report_line_reports_forwarding_and_permanent_failure() {
    assert_eq!(
        report_line(&report(SendOutcome::Forwarded("offline".into()), false)).as_deref(),
        Some("↪ bob is offline; queued to their mailbox — they'll get it when they reconnect")
    );
    assert_eq!(
        report_line(&report(SendOutcome::Failed("no such handle".into()), false)).as_deref(),
        Some("! send to bob failed: no such handle")
    );
}

#[test]
pub fn account_ids_are_told_apart_from_handles() {
    assert!(is_account_id(
        "7XDA6VEISOVEDQUS3Z7QKR2ANXXWDHJ5ICNVIQJT7BSJZ4FHP5FDATK4E4"
    ));
    assert!(!is_account_id("sidewinder_caller_0"));
    assert!(!is_account_id("bob"));
    // Right length but not base32 (lower case).
    assert!(!is_account_id(
        "7xda6veisovedqus3z7qkr2anxxwdhj5icnviqjt7bsjz4fhp5fdatk4e4"
    ));
}

#[test]
pub fn send_target_label_is_the_handle_or_id() {
    assert_eq!(SendTarget::Handle("bob".into()).label(), "bob");
    assert_eq!(SendTarget::Id("ABC".into()).label(), "ABC");
}

fn shutdown_entry(recipient: &str) -> ShutdownEntry {
    ShutdownEntry {
        id: "m1".to_string(),
        recipients: vec![recipient.to_string()],
    }
}

#[test]
pub fn shutdown_lines_report_each_message_left_at_exit() {
    // Issue #282: one line per message still queued at exit.
    let report = ShutdownReport {
        forwarded: vec![shutdown_entry("bob")],
        not_sent: vec![
            (shutdown_entry("carol"), NotSentReason::StoreForwardOff),
            (shutdown_entry("dave"), NotSentReason::PostFailed),
            (shutdown_entry("erin"), NotSentReason::InFlight),
            (shutdown_entry("frank"), NotSentReason::DeadlineReached),
        ],
    };
    let retry = "it will be retried next time you start chat";
    assert_eq!(
        shutdown_lines(&report),
        vec![
            "↪ bob is offline; queued to their mailbox — they'll get it when they reconnect"
                .to_string(),
            format!("! message to carol not delivered (store-and-forward is off); {retry}"),
            format!("! message to dave could not be posted to the mailbox; {retry}"),
            format!("! message to erin was still sending at exit; {retry}"),
            format!("! message to frank not delivered before exit; {retry}"),
        ]
    );
}

#[test]
pub fn shutdown_lines_are_empty_when_nothing_was_queued() {
    assert!(shutdown_lines(&ShutdownReport::default()).is_empty());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn exit_flush_forwards_a_message_queued_just_before_exit() {
    // Issue #282: the message is queued, and the session exits before any send; the exit flush
    // hands it to the Mailbox (pre-marked posted via the test seam, so no node is needed).
    let mut state = alice_state_gated(true, Some(MailboxConfig::new("http://localhost:9", "tok")));
    let ts = state.queue_outbound("bob", "bye").expect("queue");
    state.mark_forwarded_for_tests(&ts, "bob");
    let sender = PendingSender::start(
        Arc::new(state.outbound_store()),
        MockDelivery::new(Answer::Transient),
        PendingSenderOptions::new(),
        Arc::new(|| false),
        Arc::new(|_| {}),
    );

    let report = sender.shutdown(Duration::from_secs(5));
    assert_eq!(report.forwarded.len(), 1);
    assert!(report.not_sent.is_empty());
    assert!(state.pending_outbound().expect("pending").is_empty());
}
