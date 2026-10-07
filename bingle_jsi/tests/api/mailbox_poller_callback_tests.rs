//! The store-and-forward backstop Mailbox poller fires the message callback for each message it
//! reads (issue #307), so a host that refreshes on `onMessage` shows Mailbox messages at once.
//!
//! A stub local API returns scripted `poll_mailbox` batches (delegating everything else to a real
//! `BingleApiLocalImpl`), so the poller runs without a Sidewinder node.

use crate::api::message_queue_integration_tests::MockBingleApi;
use bingle_core::api::bingle_api::{BingleError, SendFailureKind};
use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::{BingleJsiApiImpl, mailbox_message_to_bingle};
use bingle_jsi::api::callback::MessageCallback;
use bingle_jsi::api::types::{BingleMessage, DeliveryRoute};
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use bingle_local::api::{
    Contact, ContactSource, DeliveryRoute as LocalRoute, Keypair, KeypairStatus, Message,
    MessagingSettings,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A local API whose `poll_mailbox` returns scripted batches (then empty), counting each poll.
/// Every other method delegates to a real in-memory `BingleApiLocalImpl`.
struct ScriptedMailboxLocal {
    inner: BingleApiLocalImpl,
    batches: Mutex<VecDeque<Vec<Message>>>,
    polls: Arc<AtomicUsize>,
}

impl ScriptedMailboxLocal {
    fn new(batches: Vec<Vec<Message>>, polls: Arc<AtomicUsize>) -> Self {
        Self {
            inner: BingleApiLocalImpl::new(LocalApiConfig::default()),
            batches: Mutex::new(batches.into()),
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
    fn remove_contact(&mut self, id: String) -> Result<(), BingleError> {
        self.inner.remove_contact(id)
    }
    fn is_blocked(&self, id: &str) -> Result<bool, BingleError> {
        self.inner.is_blocked(id)
    }
    fn get_contacts(&self) -> Result<Vec<Contact>, BingleError> {
        self.inner.get_contacts()
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
    ) -> Result<(), BingleError> {
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
        self.polls.fetch_add(1, Ordering::SeqCst);
        let mut batches = self.batches.lock().expect("batches lock");
        Ok(batches.pop_front().unwrap_or_default())
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
    }
}

fn mock_api() -> Arc<MockBingleApi> {
    Arc::new(MockBingleApi {
        progress_steps: vec![],
        on_listening: Mutex::new(None),
        send_fail_count: Arc::new(AtomicUsize::new(0)),
        payloads: Mutex::new(Vec::new()),
    })
}

/// Start a JSI over a scripted local API with a recording callback, foreground it so the poller
/// runs its immediate first poll, wait for `min_polls` polls, then background it. Returns the
/// recorded callback invocations.
fn run_poller(
    batches: Vec<Vec<Message>>,
    min_polls: usize,
) -> Vec<(String, String, BingleMessage)> {
    let polls = Arc::new(AtomicUsize::new(0));
    let local: Arc<Mutex<Box<dyn BingleLocalApi>>> = Arc::new(Mutex::new(Box::new(
        ScriptedMailboxLocal::new(batches, polls.clone()),
    )));
    let jsi = BingleJsiApiImpl::init_for_tests(mock_api(), Some(local));
    let received = Arc::new(Mutex::new(Vec::new()));
    jsi.set_message_callback(Box::new(RecordingCallback {
        received: received.clone(),
    }));

    jsi.foregrounding();
    let deadline = Instant::now() + Duration::from_secs(10);
    while polls.load(Ordering::SeqCst) < min_polls {
        assert!(Instant::now() < deadline, "the poller did not poll in time");
        std::thread::sleep(Duration::from_millis(20));
    }
    jsi.backgrounding();
    // The callback runs on the poller thread right after its poll; give it a moment to land.
    std::thread::sleep(Duration::from_millis(200));
    received.lock().expect("received lock").clone()
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
