//! Tests for the versioned store-and-forward envelope (issue #203).
//!
//! Covers the round-trip, the full negative-test bar (wrong recipient key, tampered ciphertext,
//! tampered `sent_time`, bad inner signature, unknown version / suite), suite-name derivation, and
//! the canonical signed-field byte layout shared with issue #94.

use bingle_core::crypto::sealed_envelope::{
    self, ENVELOPE_VERSION, EnvelopeOpenError, InnerPayload,
    SUITE_HPKE_X25519_HKDF_SHA256_CHACHA20POLY1305, SealedEnvelope, canonical_signed_message,
    suite_name,
};
use ed25519_dalek::SigningKey;

/// Fixed sender / recipient identity keys.
fn sender_key() -> SigningKey {
    SigningKey::from_bytes(&[0x11u8; 32])
}
fn recipient_key() -> SigningKey {
    SigningKey::from_bytes(&[0x22u8; 32])
}
fn recipient_pub() -> [u8; 32] {
    recipient_key().verifying_key().to_bytes()
}

const SENT_TIME: i64 = 1_700_000_000_123;
const TEXT: &str = "meet me where the shadow of the crane falls at noon";

#[test]
fn seal_open_round_trips() {
    let bytes = sealed_envelope::seal(recipient_pub(), &sender_key(), SENT_TIME, TEXT)
        .expect("seal succeeds");

    let opened = sealed_envelope::unseal(&recipient_key(), &bytes).expect("open succeeds");

    assert_eq!(opened.text, TEXT);
    assert_eq!(opened.sent_time, SENT_TIME);
    // The verified sender is the sender's Ed25519 public key.
    assert_eq!(opened.sender_id, sender_key().verifying_key().to_bytes());
    // The signature is retained (non-zero) for later report attachment.
    assert_ne!(opened.signature, [0u8; 64]);
}

#[test]
fn seal_from_private_key_matches_seal_and_round_trips() {
    // `seal_from_private_key` (the private-key convenience used by bingle_local's store-and-forward
    // post, #214) builds the signing key from the raw 32-byte account private key. Sealing from the
    // private key and from the equivalent SigningKey must produce interchangeable envelopes: opening
    // the private-key-sealed bytes recovers the same sender identity the SigningKey has.
    let sender_private_key = [0x11u8; 32];
    let bytes = sealed_envelope::seal_from_private_key(
        sender_private_key,
        recipient_pub(),
        SENT_TIME,
        TEXT,
    )
    .expect("seal_from_private_key succeeds");

    let opened = sealed_envelope::unseal(&recipient_key(), &bytes).expect("open succeeds");
    assert_eq!(opened.text, TEXT);
    assert_eq!(opened.sent_time, SENT_TIME);
    // The private key derives the same identity as SigningKey::from_bytes(key) — i.e. the sender key.
    assert_eq!(
        opened.sender_id,
        SigningKey::from_bytes(&sender_private_key)
            .verifying_key()
            .to_bytes()
    );
}

#[test]
fn unseal_with_private_key_round_trips() {
    // `open_with_private_key` (the private-key convenience used by bingle_local's read-on-reconnect,
    // #215) builds the recipient signing key from the raw 32-byte account private key. It must open
    // an envelope sealed to that account exactly as `open` with the equivalent SigningKey does.
    let recipient_private_key = [0x22u8; 32]; // matches recipient_key() / recipient_pub()
    let bytes = sealed_envelope::seal(recipient_pub(), &sender_key(), SENT_TIME, TEXT)
        .expect("seal succeeds");

    let opened = sealed_envelope::unseal_with_private_key(recipient_private_key, &bytes)
        .expect("unseal_with_private_key succeeds");
    assert_eq!(opened.text, TEXT);
    assert_eq!(opened.sent_time, SENT_TIME);
    assert_eq!(opened.sender_id, sender_key().verifying_key().to_bytes());
}

#[test]
fn unseal_with_wrong_private_key_rejected() {
    let bytes = sealed_envelope::seal(recipient_pub(), &sender_key(), SENT_TIME, TEXT).unwrap();
    let err = sealed_envelope::unseal_with_private_key([0x33u8; 32], &bytes).unwrap_err();
    assert!(matches!(err, EnvelopeOpenError::Unseal(_)));
}

#[test]
fn each_seal_differs() {
    let a = sealed_envelope::seal(recipient_pub(), &sender_key(), SENT_TIME, TEXT).unwrap();
    let b = sealed_envelope::seal(recipient_pub(), &sender_key(), SENT_TIME, TEXT).unwrap();
    // Fresh HPKE ephemeral + random message_id per call, so the bytes and the ids both differ.
    assert_ne!(a, b);
    let ida = sealed_envelope::unseal(&recipient_key(), &a)
        .unwrap()
        .message_id;
    let idb = sealed_envelope::unseal(&recipient_key(), &b)
        .unwrap()
        .message_id;
    assert_ne!(ida, idb);
}

#[test]
fn wrong_recipient_key_rejected() {
    let bytes = sealed_envelope::seal(recipient_pub(), &sender_key(), SENT_TIME, TEXT).unwrap();
    let other = SigningKey::from_bytes(&[0x33u8; 32]);
    let err = sealed_envelope::unseal(&other, &bytes).unwrap_err();
    assert!(matches!(err, EnvelopeOpenError::Unseal(_)));
}

#[test]
fn tampered_ciphertext_rejected() {
    let mut bytes = sealed_envelope::seal(recipient_pub(), &sender_key(), SENT_TIME, TEXT).unwrap();
    // Flip a byte inside the ciphertext (past the version | suite | enc header).
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    let err = sealed_envelope::unseal(&recipient_key(), &bytes).unwrap_err();
    assert!(matches!(err, EnvelopeOpenError::Unseal(_)));
}

#[test]
fn tampered_sent_time_rejected() {
    // Sign over one sent_time, then alter it in the payload before sealing: the signature no longer
    // covers the payload's sent_time, so open must reject it.
    let inner = InnerPayload::new_signed(
        &sender_key(),
        &recipient_pub(),
        SENT_TIME,
        [0u8; 16],
        TEXT.to_string(),
    );
    let mut tampered = inner.clone();
    tampered.sent_time = SENT_TIME + 5_000;

    let bytes = SealedEnvelope::seal(recipient_pub(), &tampered)
        .unwrap()
        .to_bytes();
    let err = sealed_envelope::unseal(&recipient_key(), &bytes).unwrap_err();
    assert!(matches!(err, EnvelopeOpenError::BadSignature));
}

#[test]
fn bad_inner_signature_rejected() {
    let mut inner = InnerPayload::new_signed(
        &sender_key(),
        &recipient_pub(),
        SENT_TIME,
        [0u8; 16],
        TEXT.to_string(),
    );
    inner.signature = [0u8; 64];

    let bytes = SealedEnvelope::seal(recipient_pub(), &inner)
        .unwrap()
        .to_bytes();
    let err = sealed_envelope::unseal(&recipient_key(), &bytes).unwrap_err();
    assert!(matches!(err, EnvelopeOpenError::BadSignature));
}

#[test]
fn unknown_version_rejected() {
    let mut bytes = sealed_envelope::seal(recipient_pub(), &sender_key(), SENT_TIME, TEXT).unwrap();
    bytes[0] = 0xFF; // version byte
    let err = sealed_envelope::unseal(&recipient_key(), &bytes).unwrap_err();
    assert!(matches!(err, EnvelopeOpenError::UnknownVersion(0xFF)));
}

#[test]
fn unknown_suite_rejected() {
    let mut bytes = sealed_envelope::seal(recipient_pub(), &sender_key(), SENT_TIME, TEXT).unwrap();
    bytes[1] = 0x99; // suite_id high byte
    bytes[2] = 0x99; // suite_id low byte
    let err = sealed_envelope::unseal(&recipient_key(), &bytes).unwrap_err();
    assert!(matches!(err, EnvelopeOpenError::UnknownSuite(0x9999)));
}

#[test]
fn suite_name_derivable_from_sealed_message() {
    let bytes = sealed_envelope::seal(recipient_pub(), &sender_key(), SENT_TIME, TEXT).unwrap();
    let envelope = SealedEnvelope::from_bytes(&bytes).expect("parses");

    assert_eq!(envelope.version, ENVELOPE_VERSION);
    assert_eq!(
        envelope.suite_id,
        SUITE_HPKE_X25519_HKDF_SHA256_CHACHA20POLY1305
    );
    assert_eq!(
        envelope.suite_name(),
        Some("HPKE[DHKEM(X25519, HKDF-SHA256), HKDF-SHA256, ChaCha20-Poly1305]")
    );
    assert_eq!(suite_name(0xFFFF), None);
}

#[test]
fn canonical_signed_field_layout() {
    let sender_id = [0xAAu8; 32];
    let recipient_id = [0xBBu8; 32];
    let sent_time: i64 = 0x0102_0304_0506_0708;
    let text = "hi";

    let got = canonical_signed_message(&sender_id, &recipient_id, sent_time, text);

    let mut expected = Vec::new();
    expected.extend_from_slice(&sender_id);
    expected.extend_from_slice(&recipient_id);
    expected.extend_from_slice(&sent_time.to_be_bytes());
    expected.extend_from_slice(text.as_bytes());

    assert_eq!(got, expected);
    assert_eq!(got.len(), 32 + 32 + 8 + text.len());
}
