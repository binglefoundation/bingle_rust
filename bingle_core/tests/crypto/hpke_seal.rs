//! Tests for the HPKE seal/unseal primitive (issue #202).
//!
//! Round-trip and the three authentication-failure modes are exercised here. RFC 9180 conformance
//! of the underlying `DHKEM(X25519, HKDF-SHA256)` / HKDF-SHA256 / ChaCha20-Poly1305 suite is
//! validated upstream by the `hpke` crate's bundled RFC 9180 known-answer test vectors; the
//! `golden_unseal_decrypts_known_ciphertext` test additionally pins that our exact wiring (suite,
//! empty `info`, encapsulated-key handling) still decrypts a fixed ciphertext.

use bingle_core::crypto::{ENC_LEN, UnsealError, X25519Public, X25519Secret, seal, unseal};

/// Recipient X25519 public key: the #201 conversion of the fixed Ed25519 seed `00 01 … 1f`.
const RECIPIENT_PUB: [u8; 32] = [
    0x47, 0x01, 0xd0, 0x84, 0x88, 0x45, 0x1f, 0x54, 0x5a, 0x40, 0x9f, 0xb5, 0x8a, 0xe3, 0xe5, 0x85,
    0x81, 0xca, 0x40, 0xac, 0x3f, 0x7f, 0x11, 0x46, 0x98, 0xcd, 0x71, 0xde, 0xac, 0x73, 0xca, 0x01,
];

/// Recipient X25519 secret (clamped) matching [`RECIPIENT_PUB`].
const RECIPIENT_SECRET: [u8; 32] = [
    0x38, 0x94, 0xee, 0xa4, 0x9c, 0x58, 0x0a, 0xef, 0x81, 0x69, 0x35, 0x76, 0x2b, 0xe0, 0x49, 0x55,
    0x9d, 0x6d, 0x14, 0x40, 0xde, 0xde, 0x12, 0xe6, 0xa1, 0x25, 0xf1, 0x84, 0x1f, 0xff, 0x8e, 0x6f,
];

fn recipient_pub() -> X25519Public {
    X25519Public::from(RECIPIENT_PUB)
}

fn recipient_secret() -> X25519Secret {
    X25519Secret::from(RECIPIENT_SECRET)
}

#[test]
fn seal_unseal_round_trips() {
    let aad = b"message-header-v0";
    let plaintext = b"the ships hung in the sky in much the same way that bricks don't";

    let (enc, ciphertext) = seal(&recipient_pub(), aad, plaintext).expect("seal succeeds");
    assert_eq!(enc.len(), ENC_LEN);
    // AEAD ciphertext carries the plaintext plus a 16-byte Poly1305 tag, and never equals it.
    assert_ne!(ciphertext.as_slice(), plaintext.as_slice());

    let recovered = unseal(&recipient_secret(), &enc, aad, &ciphertext).expect("unseal succeeds");
    assert_eq!(recovered, plaintext);
}

#[test]
fn seal_unseal_round_trips_empty_plaintext() {
    let aad = b"";
    let (enc, ciphertext) = seal(&recipient_pub(), aad, b"").expect("seal succeeds");
    let recovered = unseal(&recipient_secret(), &enc, aad, &ciphertext).expect("unseal succeeds");
    assert!(recovered.is_empty());
}

#[test]
fn each_seal_uses_a_fresh_ephemeral() {
    let aad = b"aad";
    let pt = b"same plaintext";
    let (enc1, ct1) = seal(&recipient_pub(), aad, pt).expect("seal 1");
    let (enc2, ct2) = seal(&recipient_pub(), aad, pt).expect("seal 2");
    // Fresh ephemeral per call: both the encapsulated key and the ciphertext differ.
    assert_ne!(enc1, enc2);
    assert_ne!(ct1, ct2);
}

#[test]
fn unseal_with_wrong_recipient_key_fails() {
    let aad = b"aad";
    let (enc, ciphertext) = seal(&recipient_pub(), aad, b"secret").expect("seal succeeds");

    // A different secret whose public key is not the seal target.
    let wrong_secret = X25519Secret::from([0x11u8; 32]);
    let result = unseal(&wrong_secret, &enc, aad, &ciphertext);
    assert!(matches!(result, Err(UnsealError::Hpke(_))));
}

#[test]
fn unseal_with_tampered_ciphertext_fails() {
    let aad = b"aad";
    let (enc, mut ciphertext) = seal(&recipient_pub(), aad, b"secret").expect("seal succeeds");

    ciphertext[0] ^= 0x01;
    let result = unseal(&recipient_secret(), &enc, aad, &ciphertext);
    assert!(matches!(result, Err(UnsealError::Hpke(_))));
}

#[test]
fn unseal_with_wrong_aad_fails() {
    let (enc, ciphertext) = seal(&recipient_pub(), b"aad-at-seal", b"secret").expect("seal");
    let result = unseal(&recipient_secret(), &enc, b"different-aad", &ciphertext);
    assert!(matches!(result, Err(UnsealError::Hpke(_))));
}

/// Associated data (`aad`) supplied at seal time to produce the golden vector below.
const GOLDEN_AAD: &[u8] = b"golden-aad-v0";
/// Plaintext sealed to produce the golden vector below.
const GOLDEN_PLAINTEXT: &[u8] = b"golden plaintext for the store-and-forward seal";
/// Golden encapsulated key: captured from [`seal`] for the fixed recipient.
const GOLDEN_ENC: [u8; ENC_LEN] = [
    0xe5, 0x7d, 0xea, 0x3c, 0xdb, 0xf1, 0xc9, 0xfd, 0xf5, 0xb2, 0xc4, 0x8d, 0x00, 0x65, 0x1f, 0x81,
    0xac, 0xda, 0x14, 0xa4, 0x17, 0x89, 0xc7, 0x5c, 0xf8, 0xf6, 0xfa, 0x55, 0xea, 0xaf, 0xad, 0x52,
];
/// Golden ciphertext (plaintext + 16-byte Poly1305 tag) matching [`GOLDEN_ENC`].
const GOLDEN_CIPHERTEXT: [u8; 63] = [
    0xa1, 0x14, 0x05, 0x58, 0x4d, 0x92, 0xd2, 0xd9, 0x99, 0x74, 0x3d, 0xed, 0x50, 0x5b, 0x98, 0x4c,
    0x90, 0xf9, 0xc3, 0x68, 0x89, 0xca, 0xf3, 0x95, 0xba, 0xa8, 0x18, 0xe6, 0x86, 0x99, 0xb1, 0xd6,
    0x96, 0x71, 0x14, 0x98, 0x8b, 0xf4, 0x14, 0x05, 0xdf, 0x0d, 0x37, 0x94, 0xa9, 0xd9, 0xae, 0x40,
    0x3f, 0x57, 0x89, 0x9d, 0x30, 0x1f, 0xf4, 0x1e, 0xce, 0x25, 0xfc, 0xaa, 0x83, 0x9e, 0xcf,
];

/// Golden regression vector: a fixed ciphertext, previously produced by [`seal`] for the fixed
/// recipient, must still [`unseal`] to the known plaintext. This pins the exact suite, the empty
/// `info`, and the encapsulated-key handling — changing any of them makes this ciphertext
/// undecryptable and fails the test. (Cross-implementation RFC 9180 conformance of the suite itself
/// is covered by the `hpke` crate's bundled known-answer test vectors.)
#[test]
fn golden_unseal_decrypts_known_ciphertext() {
    let recovered = unseal(
        &recipient_secret(),
        &GOLDEN_ENC,
        GOLDEN_AAD,
        &GOLDEN_CIPHERTEXT,
    )
    .expect("golden unseals");
    assert_eq!(recovered, GOLDEN_PLAINTEXT);
}
