//! Tests for dropping the second copy of a message that reaches the recipient both directly and
//! from their Mailbox (issue #355). The two copies share the sender, the signed send time and the
//! text, but not the id or signature. The Mailbox copy is stored through `record_mailbox_message`,
//! the store step of a Mailbox read, so no Sidewinder node is needed.

use bingle_local::api::{
    BingleApiLocalImpl, BingleLocalApi, DeliveryRoute, LocalApiConfig, Message,
};
use serde_json::json;

const SENT: i64 = 1_700_000_000_123;

fn direct(text: &str, sent_time: Option<i64>) -> serde_json::Value {
    match sent_time {
        Some(t) => json!({ "text": text, "sent_time": t, "signature": "ZGlyZWN0" }),
        None => json!({ "text": text }),
    }
}

fn mailbox(sender: &str, text: &str, sent_time: i64) -> Message {
    Message {
        id: "mailbox-copy".to_string(),
        sender_handle: sender.to_string(),
        recipient_handles: vec!["me".to_string()],
        timestamp: sent_time + 5_000,
        text: text.to_string(),
        cipher_suite: None,
        progress: Some(1.0),
        failure_reason: None,
        failure_kind: None,
        sent_time: Some(sent_time),
        delivered_time: Some(sent_time + 5_000),
        signature: Some("bWFpbGJveA==".to_string()),
        delivery_route: Some(DeliveryRoute::StoreAndForward),
        store_and_forward_failure: None,
    }
}

fn receive(api: &mut BingleApiLocalImpl, sender: &str, message: &serde_json::Value) -> bool {
    api.add_received_message(
        sender.to_string(),
        vec!["me".to_string()],
        SENT + 1_000,
        message,
    )
    .expect("add received message")
}

fn messages(api: &BingleApiLocalImpl) -> Vec<Message> {
    api.get_messages().expect("messages")
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_mailbox_copy_of_a_direct_message_is_dropped() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    assert!(receive(&mut api, "alice", &direct("hello", Some(SENT))));
    let first = messages(&api).remove(0);

    assert!(!api.record_mailbox_message(mailbox("alice", "hello", SENT)));

    let stored = messages(&api);
    assert_eq!(stored.len(), 1, "stored once: {stored:?}");
    assert_eq!(stored[0].id, first.id, "the first copy is unchanged");
    assert_eq!(stored[0].delivery_route, Some(DeliveryRoute::Direct));
    assert_eq!(stored[0].signature, first.signature);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_direct_copy_of_a_mailbox_message_is_dropped() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    assert!(api.record_mailbox_message(mailbox("alice", "hello", SENT)));

    assert!(!receive(&mut api, "alice", &direct("hello", Some(SENT))));

    let stored = messages(&api);
    assert_eq!(stored.len(), 1, "stored once: {stored:?}");
    assert_eq!(stored[0].id, "mailbox-copy", "the first copy is unchanged");
    assert_eq!(
        stored[0].delivery_route,
        Some(DeliveryRoute::StoreAndForward)
    );
    assert_eq!(stored[0].signature.as_deref(), Some("bWFpbGJveA=="));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn the_same_text_sent_at_another_time_is_kept() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    assert!(receive(&mut api, "alice", &direct("ok", Some(SENT))));
    assert!(api.record_mailbox_message(mailbox("alice", "ok", SENT + 1)));
    assert!(receive(&mut api, "alice", &direct("ok", Some(SENT + 2))));

    assert_eq!(messages(&api).len(), 3);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn the_same_text_and_time_from_another_sender_is_kept() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    assert!(receive(&mut api, "alice", &direct("hello", Some(SENT))));
    assert!(api.record_mailbox_message(mailbox("bob", "hello", SENT)));

    assert_eq!(messages(&api).len(), 2);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn other_text_at_the_same_time_is_kept() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    assert!(receive(&mut api, "alice", &direct("hello", Some(SENT))));
    assert!(api.record_mailbox_message(mailbox("alice", "goodbye", SENT)));

    assert_eq!(messages(&api).len(), 2);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_message_without_a_send_time_is_never_a_copy() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    assert!(receive(&mut api, "alice", &direct("hello", None)));
    assert!(receive(&mut api, "alice", &direct("hello", None)));

    assert_eq!(messages(&api).len(), 2);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn an_outbound_record_is_not_taken_for_a_received_copy() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.add_message(
        "alice".to_string(),
        vec!["me".to_string()],
        SENT,
        "hello".to_string(),
        None,
    )
    .expect("add message");

    assert!(receive(&mut api, "alice", &direct("hello", Some(SENT))));
    assert_eq!(messages(&api).len(), 2);
}
