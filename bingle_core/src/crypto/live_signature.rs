//! Sender signature on live text messages (issue #94).
//!
//! A text message sent over a live DTLS session carries the sender's Ed25519 signature over the
//! same canonical field set the store-and-forward envelope signs
//! ([`canonical_signed_message`]), so one non-repudiation artifact serves both routes: a recipient
//! keeps it with the stored message and can attach it to a content report. Nothing verifies it on
//! receipt; the DTLS handshake already authenticates the sender.

use base64::{Engine as _, engine::general_purpose};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::Value as JsonValue;

use crate::crypto::sealed_envelope::canonical_signed_message;
use crate::messages::marshal::from_json_value;
use crate::messages::types::Message;

/// Sign `message` in place if it is a text message, and report whether it was signed.
///
/// A text message is what the receiver routes as plain text: `text` with no `app` or `type`.
/// Control messages (ping, relay, ddb, ...) are left unchanged.
///
/// The signed `sent_time` is the message's own `sent_time` field when present (the sender's queued
/// timestamp, so a Mailbox fallback seals the same time), otherwise `now_ms`. Both `sent_time` and
/// the base64 `signature` are written to the message. The signature covers
/// `canonical_signed_message(sender, recipient_id, sent_time, text)`, where the sender is
/// `signing_key`'s public key, which is the sender's Algorand account.
pub fn sign_text_message(
    message: &mut JsonValue,
    signing_key: &SigningKey,
    recipient_id: &[u8; 32],
    now_ms: i64,
) -> bool {
    let Ok(Message::PlainText(plain)) = from_json_value(message.clone()) else {
        return false;
    };
    let Some(fields) = message.as_object_mut() else {
        return false;
    };
    let sent_time = fields
        .get("sent_time")
        .and_then(JsonValue::as_i64)
        .unwrap_or(now_ms);
    let sender_id = signing_key.verifying_key().to_bytes();
    let signed = canonical_signed_message(&sender_id, recipient_id, sent_time, &plain.text);
    let signature = signing_key.sign(&signed).to_bytes();
    fields.insert("sent_time".to_string(), JsonValue::from(sent_time));
    fields.insert(
        "signature".to_string(),
        JsonValue::String(general_purpose::STANDARD.encode(signature)),
    );
    true
}
