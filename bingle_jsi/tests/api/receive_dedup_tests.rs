//! Tests that the host is told about a live message only once when it also reaches the user from
//! their Mailbox (issue #355). The live message is delivered through the JSI's `on_message` path;
//! the Mailbox copy is stored as a Mailbox read stores it.

use super::message_queue_integration_tests::mock_api;
use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::BingleJsiApiImpl;
use bingle_jsi::api::callback::MessageCallback;
use bingle_jsi::api::types::BingleMessage;
use bingle_local::api::bingle_local_api::{BingleLocalApi, DeliveryRoute, Message};
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use serde_json::json;
use std::sync::{Arc, Mutex};

const SENT: i64 = 1_700_000_000_456;

/// Counts message callback invocations.
struct CountingCallback {
    count: Arc<Mutex<usize>>,
}

impl MessageCallback for CountingCallback {
    fn on_message(&self, _sender_id: String, _sender_handle: String, _message: BingleMessage) {
        *self.count.lock().expect("count lock") += 1;
    }
}

/// A JSI over a fresh local store, with a counting host callback. Returns the JSI, a clone of the
/// store sharing its state, and the callback count.
fn jsi_with_callback() -> (Arc<BingleJsiApiImpl>, BingleApiLocalImpl, Arc<Mutex<usize>>) {
    let local = BingleApiLocalImpl::new(LocalApiConfig::default());
    let boxed: Arc<Mutex<Box<dyn BingleLocalApi>>> = Arc::new(Mutex::new(Box::new(local.clone())));
    let jsi = BingleJsiApiImpl::init_for_tests(mock_api(vec![], 0), Some(boxed));
    let count = Arc::new(Mutex::new(0));
    jsi.set_message_callback(Box::new(CountingCallback {
        count: count.clone(),
    }));
    (jsi, local, count)
}

fn live(jsi: &BingleJsiApiImpl, text: &str) {
    jsi.receive_live_message_for_tests(
        "ALICEID".to_string(),
        "alice".to_string(),
        json!({ "text": text, "sent_time": SENT, "signature": "ZGlyZWN0" }),
    );
}

fn mailbox_copy(text: &str) -> Message {
    Message {
        id: "mailbox-copy".to_string(),
        sender_handle: "alice".to_string(),
        recipient_handles: vec!["testuser".to_string()],
        timestamp: SENT + 5_000,
        text: text.to_string(),
        cipher_suite: None,
        progress: Some(1.0),
        failure_reason: None,
        failure_kind: None,
        sent_time: Some(SENT),
        delivered_time: Some(SENT + 5_000),
        signature: Some("bWFpbGJveA==".to_string()),
        delivery_route: Some(DeliveryRoute::StoreAndForward),
        store_and_forward_failure: None,
    }
}

fn stored(local: &BingleApiLocalImpl) -> Vec<Message> {
    local.get_messages().expect("messages")
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_live_message_is_stored_and_shown() {
    let (jsi, local, count) = jsi_with_callback();

    live(&jsi, "hello");

    assert_eq!(*count.lock().expect("count"), 1);
    assert_eq!(stored(&local).len(), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_live_copy_of_a_mailbox_message_is_not_shown_again() {
    let (jsi, local, count) = jsi_with_callback();
    assert!(local.record_mailbox_message(mailbox_copy("hello")));

    live(&jsi, "hello");

    assert_eq!(
        *count.lock().expect("count"),
        0,
        "the host is not told again"
    );
    let messages = stored(&local);
    assert_eq!(messages.len(), 1, "stored once: {messages:?}");
    assert_eq!(
        messages[0].delivery_route,
        Some(DeliveryRoute::StoreAndForward)
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_mailbox_copy_of_a_live_message_is_not_stored_again() {
    let (jsi, local, count) = jsi_with_callback();
    live(&jsi, "hello");

    assert!(
        !local.record_mailbox_message(mailbox_copy("hello")),
        "left out of the Mailbox batch, so the host is not told again"
    );

    assert_eq!(*count.lock().expect("count"), 1);
    let messages = stored(&local);
    assert_eq!(messages.len(), 1, "stored once: {messages:?}");
    assert_eq!(messages[0].delivery_route, Some(DeliveryRoute::Direct));
}
