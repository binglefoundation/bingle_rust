//! Tests for the shared background sender of pending outbound messages (issue #283):
//! bingle_local::api::pending_sender. Uses the real local store and a scripted delivery mock, so no
//! engine or network is needed.
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use bingle_core::api::bingle_api::{BingleError, ProgressCallback, SendFailureKind};
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::pending_sender::{
    LocalOutboundStore, MessageDelivery, PendingSender, PendingSenderOptions, SendOutcome,
    SendReport,
};
use bingle_local::api::{BingleApiLocalImpl, LocalApiConfig, MailboxConfig};
use serde_json::Value as JsonValue;

const REPORT_TIMEOUT: Duration = Duration::from_secs(5);

/// A scripted result for one delivery. `BingleError` is not `Clone`, so results are built per call.
#[derive(Clone)]
enum Scripted {
    Delivered,
    Transient,
    Permanent,
    Panic,
    /// Block until the test sets the flag, then deliver.
    BlockUntil(Arc<AtomicBool>),
}

/// Delivery mock: per-recipient scripts (falling back to `Delivered`), with a call log.
struct MockDelivery {
    scripts: Mutex<HashMap<String, VecDeque<Scripted>>>,
    calls: Mutex<Vec<String>>,
}

impl MockDelivery {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            scripts: Mutex::new(HashMap::new()),
            calls: Mutex::new(Vec::new()),
        })
    }

    fn script(&self, recipient: &str, results: Vec<Scripted>) {
        self.scripts
            .lock()
            .expect("scripts")
            .insert(recipient.to_string(), results.into());
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("calls").clone()
    }
}

impl MessageDelivery for MockDelivery {
    fn deliver(
        &self,
        recipient: &str,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        self.calls
            .lock()
            .expect("calls")
            .push(recipient.to_string());
        let next = self
            .scripts
            .lock()
            .expect("scripts")
            .get_mut(recipient)
            .and_then(|q| q.pop_front())
            .unwrap_or(Scripted::Delivered);
        match next {
            Scripted::Delivered => Ok(true),
            Scripted::Transient => Err(BingleError::Send {
                kind: SendFailureKind::PeerUnreachable,
                detail: "offline".to_string(),
            }),
            Scripted::Permanent => Err(BingleError::Send {
                kind: SendFailureKind::HandleNotFound,
                detail: "no such handle".to_string(),
            }),
            Scripted::Panic => panic!("scripted panic in delivery"),
            Scripted::BlockUntil(flag) => {
                while !flag.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Ok(true)
            }
        }
    }
}

/// A local store with `alice` as the sender.
fn local_store(config: LocalApiConfig) -> BingleApiLocalImpl {
    let mut local = BingleApiLocalImpl::new(config);
    local.generate_keypair().expect("keypair");
    local.seed_own_handle_for_tests("alice".to_string());
    local
}

/// Persist a pending outbound message and return its timestamp.
fn queue(local: &BingleApiLocalImpl, timestamp: i64, recipients: &[&str], text: &str) -> i64 {
    local
        .add_message_shared(
            "alice".to_string(),
            recipients.iter().map(|r| r.to_string()).collect(),
            timestamp,
            text.to_string(),
            None,
        )
        .expect("add");
    local
        .update_message_status_shared(timestamp, 0.0, None, None)
        .expect("pending");
    timestamp
}

fn progress_of(local: &BingleApiLocalImpl, timestamp: i64) -> Option<f32> {
    local
        .get_messages()
        .expect("messages")
        .into_iter()
        .find(|m| m.timestamp == timestamp)
        .and_then(|m| m.progress)
}

fn fast_options() -> PendingSenderOptions {
    PendingSenderOptions {
        tick: Duration::from_millis(20),
        ..PendingSenderOptions::new()
    }
}

/// Start a sender over `local`, returning it with a receiver of its reports.
fn start(
    local: &BingleApiLocalImpl,
    delivery: Arc<MockDelivery>,
    options: PendingSenderOptions,
    ready: Arc<AtomicBool>,
) -> (PendingSender, mpsc::Receiver<SendReport>) {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let sender = PendingSender::start(
        Arc::new(LocalOutboundStore::new(local.clone(), None)),
        delivery,
        options,
        Arc::new(move || ready.load(Ordering::SeqCst)),
        Arc::new(move |report| {
            let _ = tx.lock().expect("report tx").send(report);
        }),
    );
    (sender, rx)
}

fn ready() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(true))
}

#[test]
fn delivers_a_pending_message_when_woken() {
    let local = local_store(LocalApiConfig::default());
    let ts = queue(&local, 1, &["bob"], "hi");
    let (sender, rx) = start(&local, MockDelivery::new(), fast_options(), ready());
    sender.wake();

    let report = rx.recv_timeout(REPORT_TIMEOUT).expect("report");
    assert_eq!(report.timestamp, ts);
    assert_eq!(report.outcome, SendOutcome::Delivered);
    assert_eq!(report.recipients, vec!["bob".to_string()]);
    assert!(!report.previously_failed);
    assert_eq!(progress_of(&local, ts), Some(1.0));
    assert!(local.get_pending_messages().expect("pending").is_empty());
}

#[test]
fn transient_failure_stays_pending_and_backs_off() {
    let local = local_store(LocalApiConfig::default());
    let ts = queue(&local, 1, &["bob"], "hi");
    let delivery = MockDelivery::new();
    delivery.script("bob", vec![Scripted::Transient]);
    let (_sender, rx) = start(&local, delivery.clone(), fast_options(), ready());

    let report = rx.recv_timeout(REPORT_TIMEOUT).expect("report");
    assert!(
        matches!(report.outcome, SendOutcome::Retrying(_)),
        "{report:?}"
    );
    assert_eq!(local.get_pending_messages().expect("pending").len(), 1);
    assert!(progress_of(&local, ts).unwrap_or(1.0) < 1.0);

    // Backed off: not re-attempted within the next many ticks.
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(delivery.calls().len(), 1);
}

#[test]
fn permanent_failure_is_terminal() {
    let local = local_store(LocalApiConfig::default());
    let ts = queue(&local, 1, &["bob"], "hi");
    let delivery = MockDelivery::new();
    delivery.script("bob", vec![Scripted::Permanent]);
    let (_sender, rx) = start(&local, delivery, fast_options(), ready());

    let report = rx.recv_timeout(REPORT_TIMEOUT).expect("report");
    assert!(
        matches!(report.outcome, SendOutcome::Failed(_)),
        "{report:?}"
    );
    assert_eq!(progress_of(&local, ts), Some(1.0));
    assert!(local.get_pending_messages().expect("pending").is_empty());
}

#[test]
fn retries_disabled_makes_a_transient_failure_terminal() {
    let local = local_store(LocalApiConfig::default());
    queue(&local, 1, &["bob"], "hi");
    let delivery = MockDelivery::new();
    delivery.script("bob", vec![Scripted::Transient]);
    let options = PendingSenderOptions {
        retries_enabled: false,
        ..fast_options()
    };
    let (_sender, rx) = start(&local, delivery, options, ready());

    let report = rx.recv_timeout(REPORT_TIMEOUT).expect("report");
    assert!(
        matches!(report.outcome, SendOutcome::Failed(_)),
        "{report:?}"
    );
    assert!(local.get_pending_messages().expect("pending").is_empty());
}

#[test]
fn a_permanent_failure_moves_on_to_the_next_recipient() {
    let local = local_store(LocalApiConfig::default());
    queue(&local, 1, &["bob", "carol"], "hi");
    let delivery = MockDelivery::new();
    delivery.script("bob", vec![Scripted::Permanent]);
    let (_sender, rx) = start(&local, delivery.clone(), fast_options(), ready());

    let report = rx.recv_timeout(REPORT_TIMEOUT).expect("report");
    assert_eq!(
        delivery.calls(),
        vec!["bob".to_string(), "carol".to_string()]
    );
    assert!(
        matches!(report.outcome, SendOutcome::Failed(_)),
        "{report:?}"
    );
}

#[test]
fn a_transient_failure_stops_the_remaining_recipients() {
    let local = local_store(LocalApiConfig::default());
    queue(&local, 1, &["bob", "carol"], "hi");
    let delivery = MockDelivery::new();
    delivery.script("bob", vec![Scripted::Transient]);
    let (_sender, rx) = start(&local, delivery.clone(), fast_options(), ready());

    let report = rx.recv_timeout(REPORT_TIMEOUT).expect("report");
    assert_eq!(delivery.calls(), vec!["bob".to_string()]);
    assert!(
        matches!(report.outcome, SendOutcome::Retrying(_)),
        "{report:?}"
    );
}

#[test]
fn nothing_is_sent_until_ready() {
    let local = local_store(LocalApiConfig::default());
    queue(&local, 1, &["bob"], "hi");
    let delivery = MockDelivery::new();
    let is_ready = Arc::new(AtomicBool::new(false));
    let (sender, rx) = start(&local, delivery.clone(), fast_options(), is_ready.clone());
    sender.wake();

    assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
    assert!(delivery.calls().is_empty());

    is_ready.store(true, Ordering::SeqCst);
    let report = rx.recv_timeout(REPORT_TIMEOUT).expect("report once ready");
    assert_eq!(report.outcome, SendOutcome::Delivered);
}

#[test]
fn wake_attempts_a_new_message_before_the_next_tick() {
    let local = local_store(LocalApiConfig::default());
    let options = PendingSenderOptions {
        tick: Duration::from_secs(30),
        ..PendingSenderOptions::new()
    };
    let (sender, rx) = start(&local, MockDelivery::new(), options, ready());
    // Let the scheduler settle into its (long) wait, then queue and wake.
    std::thread::sleep(Duration::from_millis(50));
    queue(&local, 1, &["bob"], "hi");
    sender.wake();

    let report = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("attempted on wake, not after the 30s tick");
    assert_eq!(report.outcome, SendOutcome::Delivered);
}

#[test]
fn a_panicking_send_does_not_kill_the_worker() {
    let local = local_store(LocalApiConfig::default());
    queue(&local, 1, &["bob"], "first");
    queue(&local, 2, &["carol"], "second");
    let delivery = MockDelivery::new();
    delivery.script("bob", vec![Scripted::Panic]);
    let (_sender, rx) = start(&local, delivery, fast_options(), ready());

    let first = rx.recv_timeout(REPORT_TIMEOUT).expect("first report");
    assert_eq!(first.timestamp, 1);
    assert!(
        matches!(first.outcome, SendOutcome::Retrying(_)),
        "{first:?}"
    );
    let second = rx.recv_timeout(REPORT_TIMEOUT).expect("second report");
    assert_eq!(second.timestamp, 2);
    assert_eq!(second.outcome, SendOutcome::Delivered);
}

#[test]
fn a_failing_recipient_does_not_starve_newer_messages() {
    let local = local_store(LocalApiConfig::default());
    queue(&local, 1, &["bob"], "to an offline peer");
    queue(&local, 2, &["carol"], "to an online peer");
    let delivery = MockDelivery::new();
    delivery.script("bob", vec![Scripted::Transient, Scripted::Transient]);
    let (_sender, rx) = start(&local, delivery, fast_options(), ready());

    let first = rx.recv_timeout(REPORT_TIMEOUT).expect("first");
    assert_eq!(first.timestamp, 1);
    let second = rx.recv_timeout(REPORT_TIMEOUT).expect("second");
    assert_eq!(second.timestamp, 2);
    assert_eq!(second.outcome, SendOutcome::Delivered);
}

#[test]
fn previously_failed_is_reported_on_a_later_success() {
    let local = local_store(LocalApiConfig::default());
    let ts = queue(&local, 1, &["bob"], "hi");
    local
        .update_message_status_shared(
            ts,
            0.0,
            Some("Recipient unreachable".to_string()),
            Some(SendFailureKind::PeerUnreachable),
        )
        .expect("mark failed");
    let (_sender, rx) = start(&local, MockDelivery::new(), fast_options(), ready());

    let report = rx.recv_timeout(REPORT_TIMEOUT).expect("report");
    assert_eq!(report.outcome, SendOutcome::Delivered);
    assert!(report.previously_failed);
}

#[test]
fn a_handed_off_failure_is_reported_forwarded() {
    // Send gate on and the recipient already posted (test seam), so the forward completes offline.
    let local = local_store(LocalApiConfig {
        sidewinder: Some(MailboxConfig::new("http://localhost:9", "tok")),
        store_and_forward_send: true,
        ..LocalApiConfig::default()
    });
    let ts = queue(&local, 1, &["bob"], "hi");
    local.mark_forwarded_for_tests(ts, "bob");
    let delivery = MockDelivery::new();
    delivery.script("bob", vec![Scripted::Transient]);
    let (_sender, rx) = start(&local, delivery, fast_options(), ready());

    let report = rx.recv_timeout(REPORT_TIMEOUT).expect("report");
    assert!(
        matches!(report.outcome, SendOutcome::Forwarded(_)),
        "{report:?}"
    );
    assert!(local.get_pending_messages().expect("pending").is_empty());
}

#[test]
fn sends_and_status_updates_do_not_need_the_callers_lock() {
    // A client keeps its store behind its own mutex (as bingle_jsi does). Holding that lock must not
    // stop the sender from sending and recording the outcome through its own handle.
    let local = local_store(LocalApiConfig::default());
    let ts = queue(&local, 1, &["bob"], "hi");
    let client_lock: Mutex<Box<dyn BingleLocalApi>> = Mutex::new(Box::new(local.clone()));
    let _held = client_lock.lock().expect("client lock");

    let (_sender, rx) = start(&local, MockDelivery::new(), fast_options(), ready());
    let report = rx
        .recv_timeout(REPORT_TIMEOUT)
        .expect("report while the lock is held");
    assert_eq!(report.outcome, SendOutcome::Delivered);
    assert_eq!(progress_of(&local, ts), Some(1.0));
}

#[test]
fn stop_does_not_wait_for_a_slow_send() {
    let local = local_store(LocalApiConfig::default());
    queue(&local, 1, &["bob"], "hi");
    let release = Arc::new(AtomicBool::new(false));
    let delivery = MockDelivery::new();
    delivery.script("bob", vec![Scripted::BlockUntil(release.clone())]);
    let (sender, _rx) = start(&local, delivery.clone(), fast_options(), ready());

    // Wait until the send is in flight.
    for _ in 0..200 {
        if !delivery.calls().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(delivery.calls().len(), 1, "send in flight");

    let started = std::time::Instant::now();
    sender.stop();
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "stop returned promptly"
    );
    release.store(true, Ordering::SeqCst);
}
