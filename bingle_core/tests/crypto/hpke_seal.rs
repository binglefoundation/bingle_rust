//! Tests for the HPKE seal/open primitive (issue #202).
//!
//! Round-trip and the three authentication-failure modes are exercised here. RFC 9180 conformance
//! of the underlying `DHKEM(X25519, HKDF-SHA256)` / HKDF-SHA256 / ChaCha20-Poly1305 suite is
//! validated upstream by the `hpke` crate's bundled RFC 9180 known-answer test vectors; the
//! `golden_open_decrypts_known_ciphertext` test additionally pins that our exact wiring (suite,
//! empty `info`, encapsulated-key handling) still decrypts a fixed ciphertext.

use bingle_core::crypto::{ENC_LEN, OpenError, X25519Public, X25519Secret, open, seal};

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
fn seal_open_round_trips() {
    let aad = b"message-header-v0";
    let plaintext = b"the ships hung in the sky in much the same way that bricks don't";

    let (enc, ciphertext) = seal(&recipient_pub(), aad, plaintext).expect("seal succeeds");
    assert_eq!(enc.len(), ENC_LEN);
    // AEAD ciphertext carries the plaintext plus a 16-byte Poly1305 tag, and never equals it.
    assert_ne!(ciphertext.as_slice(), plaintext.as_slice());

    let recovered = open(&recipient_secret(), &enc, aad, &ciphertext).expect("open succeeds");
    assert_eq!(recovered, plaintext);
}

#[test]
fn seal_open_round_trips_empty_plaintext() {
    let aad = b"";
    let (enc, ciphertext) = seal(&recipient_pub(), aad, b"").expect("seal succeeds");
    let recovered = open(&recipient_secret(), &enc, aad, &ciphertext).expect("open succeeds");
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
fn open_with_wrong_recipient_key_fails() {
    let aad = b"aad";
    let (enc, ciphertext) = seal(&recipient_pub(), aad, b"secret").expect("seal succeeds");

    // A different secret whose public key is not the seal target.
    let wrong_secret = X25519Secret::from([0x11u8; 32]);
    let result = open(&wrong_secret, &enc, aad, &ciphertext);
    assert!(matches!(result, Err(OpenError::Hpke(_))));
}

#[test]
fn open_with_tampered_ciphertext_fails() {
    let aad = b"aad";
    let (enc, mut ciphertext) = seal(&recipient_pub(), aad, b"secret").expect("seal succeeds");

    ciphertext[0] ^= 0x01;
    let result = open(&recipient_secret(), &enc, aad, &ciphertext);
    assert!(matches!(result, Err(OpenError::Hpke(_))));
}

/// Golden regression vector: a fixed ciphertext, previously produced by [`seal`] for the fixed
/// recipient, must still [`open`] to the known plaintext. This pins the exact suite, the empty
/// `info`, and the encapsulated-key handling — changing any of them makes this ciphertext
/// undecryptable and fails the test. (Cross-implementation RFC 9180 conformance of the suite itself
/// is covered by the `hpke` crate's bundled known-answer test vectors.)
#[test]
fn golden_open_decrypts_known_ciphertext() {
    let aad = b"golden-aad-v0";
    let expected_plaintext = b"golden plaintext for the store-and-forward seal";

    let enc: [u8; ENC_LEN] =
        decode_hex("e57dea3cdbf1c9fdf5b2c48d00651f81acda14a41789c75cf8f6fa55eaafad52")
            .try_into()
            .expect("enc is 32 bytes");
    let ciphertext = decode_hex(
        "a11405584d92d2d999743ded505b984c90f9c36889caf395baa818e68699b1d6\
         967114988bf41405df0d3794a9d9ae403f57899d301ff41ece25fcaa839ecf",
    );

    let recovered = open(&recipient_secret(), &enc, aad, &ciphertext).expect("golden opens");
    assert_eq!(recovered, expected_plaintext);
}

/// Decodes an ASCII hex string (no separators) into bytes.
fn decode_hex(s: &str) -> Vec<u8> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    assert_eq!(s.len() % 2, 0, "hex string has even length");
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("valid hex"))
        .collect()
}

#[test]
fn open_with_wrong_aad_fails() {
    let (enc, ciphertext) = seal(&recipient_pub(), b"aad-at-seal", b"secret").expect("seal");
    let result = open(&recipient_secret(), &enc, b"different-aad", &ciphertext);
    assert!(matches!(result, Err(OpenError::Hpke(_))));
}
