//! The store-and-forward backstop Mailbox poller fires the message callback for each message it
//! reads (issue #307), so a host that refreshes on `onMessage` shows Mailbox messages at once. It
//! also polls at once when foregrounded while running, retries soon after a node it could not
//! reach, and reports each check that reached the node to the host (issue #358).
//!
//! A stub local API returns scripted poll outcomes (delegating everything else to a real
//! `BingleApiLocalImpl`), so the poller runs without a Sidewinder node.

use crate::api::message_queue_integration_tests::MockBingleApi;
use bingle_core::api::bingle_api::{BingleError, SendFailureKind};
use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::{
    BingleJsiApiImpl, MAILBOX_RETRY_INITIAL, mailbox_message_to_bingle, mailbox_retry_delay,
};
use bingle_jsi::api::callback::{MailboxCheckCallback, MessageCallback};
use bingle_jsi::api::types::{BingleMessage, DeliveryRoute};
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use bingle_local::api::{
    Contact, ContactSource, DeliveryRoute as LocalRoute, Keypair, KeypairStatus, MailboxPoll,
    MailboxPollStatus, Message, MessagingSettings,
};
use std::collections::VecDeque;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A local API whose polls return scripted outcomes (then an empty, drained Mailbox), recording
/// when each poll ran. Every other method delegates to a real in-memory `BingleApiLocalImpl`.
struct ScriptedMailboxLocal {
    inner: BingleApiLocalImpl,
    outcomes: Mutex<VecDeque<MailboxPoll>>,
    polls: Arc<Mutex<Vec<Instant>>>,
}

impl ScriptedMailboxLocal {
    fn new(outcomes: Vec<MailboxPoll>, polls: Arc<Mutex<Vec<Instant>>>) -> Self {
        Self {
            inner: BingleApiLocalImpl::new(LocalApiConfig::default()),
            outcomes: Mutex::new(outcomes.into()),
            polls,
        }
    }
}

impl BingleLocalApi for ScriptedMailboxLocal {
    fn generate_keypair(&mut self) -> Result<Keypair, BingleError> {
        self.inner.generate_keypair()
    }
    fn import_keypair(&mut self, passphrase: String) -> Result<Keypair, BingleError> {
        self.inner.import_keypair(passphrase)
    }
    fn register_keypair(&self, handle: String) -> Result<bool, BingleError> {
        self.inner.register_keypair(handle)
    }
    fn register_apns_token(&self, token: Vec<u8>) -> Result<bool, BingleError> {
        self.inner.register_apns_token(token)
    }
    fn ensure_local_migrated(&self) -> Result<Option<String>, BingleError> {
        self.inner.ensure_local_migrated()
    }
    fn get_algo_ops(&self) -> Result<algo_ops::AlgoOps, BingleError> {
        self.inner.get_algo_ops()
    }
    fn add_contact(
        &mut self,
        handle: String,
        id: String,
        source: ContactSource,
    ) -> Result<(), BingleError> {
        self.inner.add_contact(handle, id, source)
    }
    fn block_contact(&mut self, id: String) -> Result<(), BingleError> {
        self.inner.block_contact(id)
    }
    fn unblock_contact(&mut self, id: String) -> Result<(), BingleError> {
        self.inner.unblock_contact(id)
    }
    fn remove_contact(&mut self, id: String) -> Result<(), BingleError> {
        self.inner.remove_contact(id)
    }
    fn is_blocked(&self, id: &str) -> Result<bool, BingleError> {
        self.inner.is_blocked(id)
    }
    fn get_contacts(&self) -> Result<Vec<Contact>, BingleError> {
        self.inner.get_contacts()
    }
    fn get_blocked_contacts(&self) -> Result<Vec<Contact>, BingleError> {
        self.inner.get_blocked_contacts()
    }
    fn add_message(
        &mut self,
        sender_handle: String,
        recipient_handles: Vec<String>,
        timestamp: i64,
        text: String,
        cipher_suite: Option<String>,
    ) -> Result<(), BingleError> {
        self.inner.add_message(
            sender_handle,
            recipient_handles,
            timestamp,
            text,
            cipher_suite,
        )
    }
    fn add_received_message(
        &mut self,
        sender_handle: String,
        recipient_handles: Vec<String>,
        timestamp: i64,
        message: &serde_json::Value,
    ) -> Result<bool, BingleError> {
        self.inner
            .add_received_message(sender_handle, recipient_handles, timestamp, message)
    }
    fn queue_message(
        &mut self,
        recipient_handles: Vec<String>,
        text: String,
    ) -> Result<(), BingleError> {
        self.inner.queue_message(recipient_handles, text)
    }
    fn update_message_status(
        &mut self,
        id: &str,
        progress: f32,
        failure_reason: Option<String>,
        failure_kind: Option<SendFailureKind>,
    ) -> Result<(), BingleError> {
        self.inner
            .update_message_status(id, progress, failure_reason, failure_kind)
    }
    fn get_pending_messages(&self) -> Result<Vec<Message>, BingleError> {
        self.inner.get_pending_messages()
    }
    fn get_messages(&self) -> Result<Vec<Message>, BingleError> {
        self.inner.get_messages()
    }
    fn poll_mailbox(&self) -> Result<Vec<Message>, BingleError> {
        self.poll_mailbox_outcome().map(|poll| poll.messages)
    }
    fn poll_mailbox_outcome(&self) -> Result<MailboxPoll, BingleError> {
        self.polls.lock().expect("polls lock").push(Instant::now());
        let mut outcomes = self.outcomes.lock().expect("outcomes lock");
        Ok(outcomes.pop_front().unwrap_or_else(|| drained(vec![])))
    }
    fn forward_pending_to_mailbox(&self) -> Result<usize, BingleError> {
        self.inner.forward_pending_to_mailbox()
    }
    fn set_store_and_forward(&mut self, send: bool, receive: bool) {
        self.inner.set_store_and_forward(send, receive)
    }
    fn set_notify(&mut self, enabled: bool, gateway_url: Option<String>) {
        self.inner.set_notify(enabled, gateway_url)
    }
    fn messaging_settings(&self) -> MessagingSettings {
        self.inner.messaging_settings()
    }
    fn save(&self, path: &str) -> Result<(), BingleError> {
        self.inner.save(path)
    }
    fn load(&mut self, path: &str) -> Result<(), BingleError> {
        self.inner.load(path)
    }
    fn network_available(&self, force_recheck: bool) -> Result<bool, BingleError> {
        self.inner.network_available(force_recheck)
    }
    fn keypair_status(&self) -> Result<KeypairStatus, BingleError> {
        self.inner.keypair_status()
    }
    fn get_keypair(&self) -> Result<Option<Keypair>, BingleError> {
        self.inner.get_keypair()
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Records every message callback invocation.
struct RecordingCallback {
    received: Arc<Mutex<Vec<(String, String, BingleMessage)>>>,
}

impl MessageCallback for RecordingCallback {
    fn on_message(&self, sender_id: String, sender_handle: String, message: BingleMessage) {
        self.received
            .lock()
            .expect("received lock")
            .push((sender_id, sender_handle, message));
    }
}

/// Records every Mailbox check reported to the host.
struct RecordingCheckCallback {
    checks: Arc<Mutex<Vec<u32>>>,
}

impl MailboxCheckCallback for RecordingCheckCallback {
    fn on_mailbox_checked(&self, read: u32) {
        self.checks.lock().expect("checks lock").push(read);
    }
}

/// A poll that reached the node and read `messages`.
fn drained(messages: Vec<Message>) -> MailboxPoll {
    MailboxPoll {
        messages,
        status: MailboxPollStatus::Drained,
    }
}

/// A poll that could not reach the node.
fn unreachable() -> MailboxPoll {
    MailboxPoll {
        messages: vec![],
        status: MailboxPollStatus::Unreachable("node not reachable".to_string()),
    }
}

/// A message as a Mailbox read stores it: received, store-and-forward, with a delivered time.
fn mailbox_message(id: &str, sender: &str, text: &str) -> Message {
    Message {
        id: id.to_string(),
        sender_handle: sender.to_string(),
        recipient_handles: vec!["testuser".to_string()],
        timestamp: 1_700_000_050_000,
        text: text.to_string(),
        cipher_suite: None,
        progress: Some(1.0),
        failure_reason: None,
        failure_kind: None,
        sent_time: Some(1_700_000_000_000),
        delivered_time: Some(1_700_000_050_000),
        signature: None,
        delivery_route: Some(LocalRoute::StoreAndForward),
        store_and_forward_failure: None,
    }
}

fn mock_api() -> Arc<MockBingleApi> {
    Arc::new(MockBingleApi {
        progress_steps: vec![],
        on_listening: Mutex::new(None),
        send_fail_count: Arc::new(AtomicUsize::new(0)),
        payloads: Mutex::new(Vec::new()),
        hold_sends: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    })
}

/// A JSI over a scripted local API, with recording message and Mailbox check callbacks.
struct PollerHarness {
    jsi: Arc<BingleJsiApiImpl>,
    polls: Arc<Mutex<Vec<Instant>>>,
    received: Arc<Mutex<Vec<(String, String, BingleMessage)>>>,
    checks: Arc<Mutex<Vec<u32>>>,
}

impl PollerHarness {
    /// Build over `outcomes`, polling every `interval` and retrying an unreachable poll after
    /// `retry_initial`.
    fn new(outcomes: Vec<MailboxPoll>, interval: Duration, retry_initial: Duration) -> Self {
        let polls = Arc::new(Mutex::new(Vec::new()));
        let local: Arc<Mutex<Box<dyn BingleLocalApi>>> = Arc::new(Mutex::new(Box::new(
            ScriptedMailboxLocal::new(outcomes, polls.clone()),
        )));
        let jsi = BingleJsiApiImpl::init_for_tests_with_mailbox_timing(
            mock_api(),
            Some(local),
            interval,
            retry_initial,
        );
        let received = Arc::new(Mutex::new(Vec::new()));
        jsi.set_message_callback(Box::new(RecordingCallback {
            received: received.clone(),
        }));
        let checks = Arc::new(Mutex::new(Vec::new()));
        jsi.set_mailbox_check_callback(Box::new(RecordingCheckCallback {
            checks: checks.clone(),
        }));
        Self {
            jsi,
            polls,
            received,
            checks,
        }
    }

    fn poll_count(&self) -> usize {
        self.polls.lock().expect("polls lock").len()
    }

    /// Wait up to `within` for at least `count` polls; return whether they happened.
    fn wait_for_polls(&self, count: usize, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        while self.poll_count() < count {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        true
    }

    /// Background the JSI, and give the callbacks the poller runs right after its poll a moment to
    /// land.
    fn stop(&self) {
        self.jsi.backgrounding();
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Start a JSI over a scripted local API with a recording callback, foreground it so the poller
/// runs its immediate first poll, wait for `min_polls` polls, then background it. Returns the
/// recorded callback invocations.
fn run_poller(
    batches: Vec<Vec<Message>>,
    min_polls: usize,
) -> Vec<(String, String, BingleMessage)> {
    let harness = PollerHarness::new(
        batches.into_iter().map(drained).collect(),
        Duration::from_secs(120),
        MAILBOX_RETRY_INITIAL,
    );
    harness.jsi.foregrounding();
    assert!(
        harness.wait_for_polls(min_polls, Duration::from_secs(10)),
        "the poller did not poll in time"
    );
    harness.stop();
    harness.received.lock().expect("received lock").clone()
}

#[test]
fn each_mailbox_message_fires_the_callback_once_with_its_stored_id() {
    let received = run_poller(
        vec![vec![
            mailbox_message("0001", "alice", "first"),
            mailbox_message("0002", "bob", "second"),
        ]],
        1,
    );

    assert_eq!(received.len(), 2, "one callback per message read");
    let (sender_id, sender_handle, message) = &received[0];
    assert_eq!(
        sender_id, "",
        "the sender id is not known from a Mailbox read"
    );
    assert_eq!(sender_handle, "alice");
    assert_eq!(message.id.as_deref(), Some("0001"));
    assert_eq!(message.text.as_deref(), Some("first"));
    assert_eq!(message.delivery_route, Some(DeliveryRoute::StoreAndForward));
    assert_eq!(message.delivered_time, Some(1_700_000_050_000));
    assert_eq!(received[1].1, "bob");
    assert_eq!(received[1].2.id.as_deref(), Some("0002"));
}

#[test]
fn an_empty_poll_fires_no_callback() {
    let received = run_poller(vec![], 1);
    assert!(received.is_empty(), "no callback on an empty poll");
}

#[test]
fn mailbox_message_maps_text_cipher_suite_and_store_and_forward_fields() {
    let mut local = mailbox_message("0042", "carol", "hello");
    local.cipher_suite = Some("TLS_ECDHE".to_string());
    let message = mailbox_message_to_bingle(&local);

    assert_eq!(message.text.as_deref(), Some("hello"));
    assert_eq!(message.cipher_suite.as_deref(), Some("TLS_ECDHE"));
    assert_eq!(message.id.as_deref(), Some("0042"));
    assert_eq!(message.delivery_route, Some(DeliveryRoute::StoreAndForward));
    assert_eq!(message.delivered_time, Some(1_700_000_050_000));
    assert!(message.app.is_none() && message.r#type.is_none() && message.data.is_none());
}

/// Foregrounding while the poller runs polls at once rather than at the next cycle (issue #358).
#[test]
fn foregrounding_a_running_poller_polls_at_once() {
    let harness = PollerHarness::new(vec![], Duration::from_secs(60), MAILBOX_RETRY_INITIAL);
    harness.jsi.foregrounding();
    assert!(
        harness.wait_for_polls(1, Duration::from_secs(5)),
        "foregrounding polls at once"
    );

    harness.jsi.foregrounding();
    assert!(
        harness.wait_for_polls(2, Duration::from_secs(1)),
        "a second foregrounding wakes the running poller for another poll"
    );
    assert!(harness.jsi.mailbox_poller_running_for_tests());
    harness.stop();
}

/// An empty Mailbox waits the full interval before the next poll (issue #358).
#[test]
fn an_empty_mailbox_waits_the_full_interval() {
    let harness = PollerHarness::new(vec![], Duration::from_secs(60), Duration::from_millis(50));
    harness.jsi.foregrounding();
    assert!(harness.wait_for_polls(1, Duration::from_secs(5)));

    assert!(
        !harness.wait_for_polls(2, Duration::from_millis(500)),
        "no early poll after a poll that drained the Mailbox"
    );
    harness.stop();
}

/// A poll that could not reach the node is retried soon, not after a full interval (issue #358).
#[test]
fn an_unreachable_poll_is_retried_soon() {
    let harness = PollerHarness::new(
        vec![unreachable(), unreachable()],
        Duration::from_secs(60),
        Duration::from_millis(100),
    );
    harness.jsi.foregrounding();
    assert!(
        harness.wait_for_polls(3, Duration::from_secs(5)),
        "two unreachable polls are each retried soon"
    );
    // Then the drained poll waits the full interval.
    assert!(
        !harness.wait_for_polls(4, Duration::from_millis(500)),
        "a drained poll resets the retry"
    );

    let polls = harness.polls.lock().expect("polls lock").clone();
    let first_gap = polls[1] - polls[0];
    let second_gap = polls[2] - polls[1];
    assert!(
        first_gap >= Duration::from_millis(100),
        "the first retry waits the initial delay: {first_gap:?}"
    );
    assert!(
        second_gap >= Duration::from_millis(200),
        "the second retry waits twice as long: {second_gap:?}"
    );
    harness.stop();
}

/// Each poll that reached the node is reported with the number of messages it read; a poll that
/// did not reach the node, or did not run, is not (issue #358).
#[test]
fn each_check_that_reached_the_node_is_reported_to_the_host() {
    let harness = PollerHarness::new(
        vec![
            unreachable(),
            drained(vec![
                mailbox_message("0001", "alice", "first"),
                mailbox_message("0002", "bob", "second"),
            ]),
            MailboxPoll::skipped(),
        ],
        Duration::from_secs(60),
        Duration::from_millis(50),
    );
    harness.jsi.foregrounding();
    assert!(harness.wait_for_polls(2, Duration::from_secs(5)));
    // The skipped poll and an empty drained one, each on a wake.
    harness.jsi.foregrounding();
    assert!(harness.wait_for_polls(3, Duration::from_secs(1)));
    harness.jsi.foregrounding();
    assert!(harness.wait_for_polls(4, Duration::from_secs(1)));
    harness.stop();

    assert_eq!(
        *harness.checks.lock().expect("checks lock"),
        vec![2, 0],
        "the two drained polls are reported, with how many messages each read"
    );
    assert_eq!(
        harness.received.lock().expect("received lock").len(),
        2,
        "the messages are passed to the message callback too"
    );
}

/// The retry delay doubles from the initial delay per failure, capped at the interval, and is the
/// full interval with no failures (issue #358).
#[test]
fn retry_delay_doubles_up_to_the_interval() {
    let initial = Duration::from_secs(5);
    let interval = Duration::from_secs(120);
    assert_eq!(mailbox_retry_delay(0, initial, interval), interval);
    assert_eq!(
        mailbox_retry_delay(1, initial, interval),
        Duration::from_secs(5)
    );
    assert_eq!(
        mailbox_retry_delay(2, initial, interval),
        Duration::from_secs(10)
    );
    assert_eq!(
        mailbox_retry_delay(3, initial, interval),
        Duration::from_secs(20)
    );
    assert_eq!(
        mailbox_retry_delay(5, initial, interval),
        Duration::from_secs(80)
    );
    assert_eq!(mailbox_retry_delay(6, initial, interval), interval);
    assert_eq!(mailbox_retry_delay(u32::MAX, initial, interval), interval);
}
