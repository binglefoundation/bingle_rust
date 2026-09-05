//! Deterministic conversion between the Ed25519 identity keys used on Algorand and the X25519
//! keys required for Diffie-Hellman.
//!
//! Bingle's store-and-forward seal targets a recipient's durable Ed25519 Algorand identity key —
//! an Algorand address *is* a 32-byte Ed25519 public key. Hybrid public key encryption (HPKE) and
//! its Diffie-Hellman-based key encapsulation mechanism (DHKEM) instead operate on X25519 keys, so
//! this module maps identity keys into the X25519 domain and nothing more (no encryption; that is
//! built on top of this in later store-and-forward stories).
//!
//! The two directions are:
//!
//! - [`ed25519_pub_to_x25519`] — recipient public: Ed25519 public key (a decoded Algorand address)
//!   → X25519 public key, via the birational Edwards-`y` → Montgomery-`u` map.
//! - [`ed25519_secret_to_x25519`] — own private: Ed25519 signing key → X25519 secret, computed as
//!   `clamp(SHA-512(seed)[0..32])`. This is exactly how Ed25519 already derives its signing scalar,
//!   so the converted keypair is internally consistent: the X25519 public derived from the
//!   converted secret equals the X25519 public converted from the matching Ed25519 public.
//!
//! The coordinate map is not hand-rolled: the public direction uses `ed25519-dalek`'s vetted
//! [`VerifyingKey::to_montgomery`](ed25519_dalek::VerifyingKey::to_montgomery) (backed by
//! `curve25519-dalek`), and the secret direction reuses the same crate's Ed25519 scalar expansion.

use ed25519_dalek::{SigningKey, VerifyingKey};

/// X25519 public key, re-exported from the vetted `x25519-dalek` primitive.
pub type X25519Public = x25519_dalek::PublicKey;

/// X25519 static secret key, re-exported from the vetted `x25519-dalek` primitive.
pub type X25519Secret = x25519_dalek::StaticSecret;

/// Error returned when an Ed25519 public key cannot be converted to X25519.
#[derive(Debug, thiserror::Error)]
pub enum KeyConvertError {
    /// The 32 bytes are not a canonical Ed25519 public key (they do not decompress to a valid
    /// curve point), so no Montgomery `u`-coordinate exists for them.
    #[error("invalid Ed25519 public key: not a canonical curve point ({0})")]
    InvalidEd25519Public(ed25519_dalek::SignatureError),
}

/// Converts an Ed25519 public key (a decoded Algorand address, 32 bytes) to its X25519 public key.
///
/// The conversion is the birational Edwards-`y` → Montgomery-`u` map; the input must be a
/// canonical Ed25519 public key. Algorand *identity* addresses are Ed25519 public keys and always
/// convert; addresses that are not curve points (for example application or logic-signature
/// addresses) are rejected.
///
/// # Errors
///
/// Returns [`KeyConvertError::InvalidEd25519Public`] if `pub_bytes` does not decompress to a valid
/// Ed25519 curve point.
pub fn ed25519_pub_to_x25519(pub_bytes: [u8; 32]) -> Result<X25519Public, KeyConvertError> {
    let verifying_key =
        VerifyingKey::from_bytes(&pub_bytes).map_err(KeyConvertError::InvalidEd25519Public)?;
    Ok(X25519Public::from(verifying_key.to_montgomery().to_bytes()))
}

/// Converts an Ed25519 signing key to its X25519 static secret.
///
/// The X25519 secret is `clamp(SHA-512(seed)[0..32])`, the same clamped scalar Ed25519 derives from
/// the signing key's seed. The returned secret is therefore consistent with the X25519 public
/// obtained by running [`ed25519_pub_to_x25519`] on the matching Ed25519 public key.
pub fn ed25519_secret_to_x25519(signing_key: &SigningKey) -> X25519Secret {
    // `to_scalar_bytes` returns the unclamped SHA-512(seed)[0..32]; clamp it here so the secret's
    // stored bytes match the canonical X25519 scalar (`x25519-dalek` stores the bytes verbatim and
    // clamps only at use, which would otherwise leave the raw, unclamped bytes exposed).
    X25519Secret::from(clamp_scalar_bytes(signing_key.to_scalar_bytes()))
}

/// Applies the X25519 (RFC 7748) scalar clamp: clear the three low bits of the first byte, clear
/// the top bit of the last byte, and set the second-highest bit of the last byte.
fn clamp_scalar_bytes(mut bytes: [u8; 32]) -> [u8; 32] {
    bytes[0] &= 248;
    bytes[31] &= 127;
    bytes[31] |= 64;
    bytes
}
