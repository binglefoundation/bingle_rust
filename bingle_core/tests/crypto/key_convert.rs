//! Tests for the Ed25519 ⇄ X25519 key conversion (issue #201).
//!
//! The known-answer vectors below were produced by an independent oracle: the Edwards-`y` →
//! Montgomery-`u` birational map and the RFC 7748 scalar clamp implemented directly in big-integer
//! arithmetic (Python), cross-checked against the `cryptography` library's X25519. They do not
//! depend on the crate under test, so they genuinely pin the conversion rather than restating it.

use bingle_core::crypto::{
    KeyConvertError, X25519Public, ed25519_pub_to_x25519, ed25519_secret_to_x25519,
};
use ed25519_dalek::SigningKey;

/// Deterministic Ed25519 seed `00 01 02 … 1f`.
const SEED: [u8; 32] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];

/// Ed25519 public key for [`SEED`].
const ED25519_PUB: [u8; 32] = [
    0x03, 0xa1, 0x07, 0xbf, 0xf3, 0xce, 0x10, 0xbe, 0x1d, 0x70, 0xdd, 0x18, 0xe7, 0x4b, 0xc0, 0x99,
    0x67, 0xe4, 0xd6, 0x30, 0x9b, 0xa5, 0x0d, 0x5f, 0x1d, 0xdc, 0x86, 0x64, 0x12, 0x55, 0x31, 0xb8,
];

/// X25519 public key that [`ED25519_PUB`] converts to.
const X25519_PUB: [u8; 32] = [
    0x47, 0x01, 0xd0, 0x84, 0x88, 0x45, 0x1f, 0x54, 0x5a, 0x40, 0x9f, 0xb5, 0x8a, 0xe3, 0xe5, 0x85,
    0x81, 0xca, 0x40, 0xac, 0x3f, 0x7f, 0x11, 0x46, 0x98, 0xcd, 0x71, 0xde, 0xac, 0x73, 0xca, 0x01,
];

/// X25519 secret (clamped) that the [`SEED`] signing key converts to.
const X25519_SECRET_CLAMPED: [u8; 32] = [
    0x38, 0x94, 0xee, 0xa4, 0x9c, 0x58, 0x0a, 0xef, 0x81, 0x69, 0x35, 0x76, 0x2b, 0xe0, 0x49, 0x55,
    0x9d, 0x6d, 0x14, 0x40, 0xde, 0xde, 0x12, 0xe6, 0xa1, 0x25, 0xf1, 0x84, 0x1f, 0xff, 0x8e, 0x6f,
];

/// Sanity check that the fixture seed really derives the fixture Ed25519 public key, so the rest of
/// the vectors are anchored to the same identity as the independent oracle.
#[test]
fn seed_derives_expected_ed25519_public() {
    let signing_key = SigningKey::from_bytes(&SEED);
    assert_eq!(signing_key.verifying_key().to_bytes(), ED25519_PUB);
}

/// Known-answer test for the public direction: Ed25519 public → X25519 public.
#[test]
fn ed25519_pub_to_x25519_matches_known_answer() {
    let x_pub = ed25519_pub_to_x25519(ED25519_PUB).expect("valid curve point converts");
    assert_eq!(x_pub.to_bytes(), X25519_PUB);
}

/// Known-answer test for the secret direction: Ed25519 signing key → X25519 secret.
#[test]
fn ed25519_secret_to_x25519_matches_known_answer() {
    let signing_key = SigningKey::from_bytes(&SEED);
    let x_secret = ed25519_secret_to_x25519(&signing_key);
    assert_eq!(x_secret.to_bytes(), X25519_SECRET_CLAMPED);
}

/// The converted keypair is self-consistent: the X25519 public derived from the converted secret
/// equals `base · x25519_priv`, and equals the X25519 public converted from the Ed25519 public.
#[test]
fn converted_keypair_is_self_consistent() {
    let signing_key = SigningKey::from_bytes(&SEED);

    let x_secret = ed25519_secret_to_x25519(&signing_key);
    let x_pub_from_secret = X25519Public::from(&x_secret);

    let x_pub_from_public = ed25519_pub_to_x25519(signing_key.verifying_key().to_bytes())
        .expect("own public is a valid curve point");

    assert_eq!(x_pub_from_secret.to_bytes(), x_pub_from_public.to_bytes());
    assert_eq!(x_pub_from_secret.to_bytes(), X25519_PUB);
}

/// A recipient address round-trips: address → Ed25519 public → X25519 public equals the
/// recipient's own converted X25519 public (the value the recipient derives from its secret).
#[test]
fn recipient_address_round_trips_to_own_x25519_public() {
    let signing_key = SigningKey::from_bytes(&SEED);
    let own_pub_bytes = signing_key.verifying_key().to_bytes();

    // Recipient identity as seen by a sender: an Algorand address string.
    let address = algo_ops::byte_key_to_address(&own_pub_bytes).expect("address encodes");
    let recovered = algo_ops::address_to_byte_key(&address).expect("address decodes");
    assert_eq!(recovered, own_pub_bytes);

    // Sender's view: seal target derived from the address.
    let x_pub_from_address =
        ed25519_pub_to_x25519(recovered).expect("recipient identity is a valid curve point");

    // Recipient's view: X25519 public derived from its own secret.
    let own_secret = ed25519_secret_to_x25519(&signing_key);
    let own_x_pub = X25519Public::from(&own_secret);

    assert_eq!(x_pub_from_address.to_bytes(), own_x_pub.to_bytes());
}

/// Bytes that are not a canonical Ed25519 curve point (`y = 2` has no matching `x`) are rejected
/// rather than silently producing a bogus X25519 key.
#[test]
fn invalid_public_key_is_rejected() {
    let mut not_a_point = [0u8; 32];
    not_a_point[0] = 2;
    let result = ed25519_pub_to_x25519(not_a_point);
    assert!(matches!(
        result,
        Err(KeyConvertError::InvalidEd25519Public(_))
    ));
}
