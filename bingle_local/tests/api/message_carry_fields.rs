//! Tests for carrying `sent_time` / `delivered` / `signature` onto the local message (issue #204).

use base64::{Engine as _, engine::general_purpose};
use bingle_core::crypto::sealed_envelope::{
    OpenedMessage, SUITE_HPKE_X25519_HKDF_SHA256_CHACHA20POLY1305,
};
use bingle_local::api::Message;

fn sample_opened() -> OpenedMessage {
    OpenedMessage {
        sender_id: [0x07u8; 32],
        sent_time: 1_700_000_000_123,
        message_id: [0x09u8; 16],
        text: "hello from the mailbox".to_string(),
        signature: [0x03u8; 64],
        suite_id: SUITE_HPKE_X25519_HKDF_SHA256_CHACHA20POLY1305,
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

#[test]
fn a_received_live_message_keeps_the_senders_signature_and_sent_time() {
    use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, DeliveryRoute, LocalApiConfig};
    // The engine's on_message JSON for a signed live text message (issue #94).
    let received = serde_json::json!({
        "text": "hello live",
        "cipher_suite": "TLS_AES_256_GCM_SHA384",
        "sent_time": 1_700_000_000_456i64,
        "signature": "AwMDAw==",
    });
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());

    api.add_received_message("alice".into(), vec!["me".into()], 9, &received)
        .expect("store");

    let stored = &api.get_messages().expect("messages")[0];
    assert_eq!(stored.text, "hello live");
    assert_eq!(
        stored.cipher_suite.as_deref(),
        Some("TLS_AES_256_GCM_SHA384")
    );
    assert_eq!(stored.sent_time, Some(1_700_000_000_456));
    assert_eq!(stored.signature.as_deref(), Some("AwMDAw=="));
    assert_eq!(stored.delivered_time, None);
    assert_eq!(stored.delivery_route, Some(DeliveryRoute::Direct));
}

#[test]
fn a_received_message_from_an_older_client_has_no_signature() {
    use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig};
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());

    api.add_received_message(
        "alice".into(),
        vec!["me".into()],
        9,
        &serde_json::json!({ "text": "unsigned" }),
    )
    .expect("store");

    let stored = &api.get_messages().expect("messages")[0];
    assert_eq!(stored.text, "unsigned");
    assert_eq!(stored.sent_time, None);
    assert_eq!(stored.signature, None);
}
