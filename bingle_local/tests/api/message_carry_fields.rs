//! Tests for carrying `sent_time` / `delivered` / `signature` onto the local message (issue #204).

use base64::{Engine as _, engine::general_purpose};
use bingle_core::crypto::sealed_envelope::OpenedMessage;
use bingle_local::api::Message;

fn sample_opened() -> OpenedMessage {
    OpenedMessage {
        sender_id: [0x07u8; 32],
        sent_time: 1_700_000_000_123,
        message_id: [0x09u8; 16],
        text: "hello from the mailbox".to_string(),
        signature: [0x03u8; 64],
    }
}

#[test]
fn from_opened_carries_sent_time_delivered_and_signature() {
    let opened = sample_opened();
    let delivered_time = 1_700_000_050_000;

    let msg = Message::from_opened(&opened, "alice".into(), vec!["bob".into()], delivered_time);

    assert_eq!(msg.sender_handle, "alice");
    assert_eq!(msg.recipient_handles, vec!["bob".to_string()]);
    assert_eq!(msg.text, opened.text);
    // sender-stamped time carried from the envelope
    assert_eq!(msg.sent_time, Some(opened.sent_time));
    // locally stamped delivered clock, and arrival timestamp set to it
    assert_eq!(msg.delivered_time, Some(delivered_time));
    assert_eq!(msg.timestamp, delivered_time);
    // a received message is complete
    assert_eq!(msg.progress, Some(1.0));
    // retained signature is the base64 of the raw 64 envelope bytes
    let decoded = general_purpose::STANDARD
        .decode(msg.signature.as_ref().expect("signature carried"))
        .expect("valid base64");
    assert_eq!(decoded, opened.signature.to_vec());
}

#[test]
fn old_message_files_without_new_fields_still_load() {
    // A message serialized before #204: none of sent_time / delivered_time / signature present.
    let old_json = r#"{
        "sender_handle": "alice",
        "recipient_handles": ["bob"],
        "timestamp": 5,
        "text": "old message",
        "progress": 1.0
    }"#;

    let msg: Message = serde_json::from_str(old_json).expect("old file loads");

    assert_eq!(msg.text, "old message");
    assert_eq!(msg.sent_time, None);
    assert_eq!(msg.delivered_time, None);
    assert_eq!(msg.signature, None);
}

#[test]
fn none_carry_fields_are_omitted_and_round_trip() {
    let opened = sample_opened();
    let carried = Message::from_opened(&opened, "alice".into(), vec!["bob".into()], 42);

    // A message without carry fields (e.g. a locally queued one) omits them from JSON.
    let mut plain = carried.clone();
    plain.sent_time = None;
    plain.delivered_time = None;
    plain.signature = None;
    let plain_json = serde_json::to_string(&plain).expect("serialize");
    assert!(!plain_json.contains("sent_time"));
    assert!(!plain_json.contains("delivered_time"));
    assert!(!plain_json.contains("signature"));

    // A carried message serializes the fields and round-trips unchanged.
    let carried_json = serde_json::to_string(&carried).expect("serialize");
    assert!(carried_json.contains("sent_time"));
    assert!(carried_json.contains("delivered_time"));
    assert!(carried_json.contains("signature"));
    let back: Message = serde_json::from_str(&carried_json).expect("round-trip");
    assert_eq!(back, carried);
}
