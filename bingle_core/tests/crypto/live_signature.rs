//! Tests for the sender signature on live text messages (issue #94): text messages are signed over
//! the store-and-forward canonical field set, control messages are not, and changing a signed field
//! breaks verification.

use base64::{Engine as _, engine::general_purpose};
use bingle_core::crypto::live_signature::sign_text_message;
use bingle_core::crypto::sealed_envelope::canonical_signed_message;
use ed25519_dalek::{Signature, SigningKey, Verifier};
use serde_json::{Value, json};

const NOW: i64 = 1_700_000_000_000;

fn sender() -> SigningKey {
    SigningKey::from_bytes(&[0x11u8; 32])
}

fn recipient_id() -> [u8; 32] {
    SigningKey::from_bytes(&[0x22u8; 32])
        .verifying_key()
        .to_bytes()
}

/// Verify `message`'s signature as a report verifier would, from the sender and recipient ids.
fn verifies(message: &Value, sender: &SigningKey, recipient_id: &[u8; 32]) -> bool {
    let sent_time = message["sent_time"].as_i64().expect("sent_time");
    let text = message["text"].as_str().expect("text");
    let bytes = general_purpose::STANDARD
        .decode(message["signature"].as_str().expect("signature"))
        .expect("base64");
    let signature = Signature::from_slice(&bytes).expect("64-byte signature");
    let signed = canonical_signed_message(
        &sender.verifying_key().to_bytes(),
        recipient_id,
        sent_time,
        text,
    );
    sender.verifying_key().verify(&signed, &signature).is_ok()
}

#[test]
fn a_text_message_is_signed_and_verifies() {
    let mut message = json!({ "text": "hello" });

    assert!(sign_text_message(
        &mut message,
        &sender(),
        &recipient_id(),
        NOW
    ));

    assert_eq!(
        message["sent_time"], NOW,
        "stamped now when the sender gave no time"
    );
    assert!(verifies(&message, &sender(), &recipient_id()));
}

#[test]
fn the_senders_queued_time_is_the_signed_time() {
    let queued: i64 = 1_699_999_999_000;
    let mut message = json!({ "text": "hello", "sent_time": queued });

    assert!(sign_text_message(
        &mut message,
        &sender(),
        &recipient_id(),
        NOW
    ));

    assert_eq!(message["sent_time"], queued);
    assert!(verifies(&message, &sender(), &recipient_id()));
}

#[test]
fn control_messages_are_not_signed() {
    for control in [
        json!({ "app": "ping", "type": "ping", "text": "ping" }),
        json!({ "app": "ddb", "type": "get", "id": "X" }),
        json!({ "type": "relay_call" }),
    ] {
        let mut message = control.clone();
        assert!(
            !sign_text_message(&mut message, &sender(), &recipient_id(), NOW),
            "{control} should not be signed"
        );
        assert_eq!(message, control, "a control message is left unchanged");
    }
}

#[test]
fn changing_a_signed_field_fails_verification() {
    let mut message = json!({ "text": "hello" });
    sign_text_message(&mut message, &sender(), &recipient_id(), NOW);

    let mut text_changed = message.clone();
    text_changed["text"] = json!("goodbye");
    assert!(!verifies(&text_changed, &sender(), &recipient_id()));

    let mut time_changed = message.clone();
    time_changed["sent_time"] = json!(NOW + 1);
    assert!(!verifies(&time_changed, &sender(), &recipient_id()));

    let other_recipient = SigningKey::from_bytes(&[0x33u8; 32])
        .verifying_key()
        .to_bytes();
    assert!(!verifies(&message, &sender(), &other_recipient));
}
