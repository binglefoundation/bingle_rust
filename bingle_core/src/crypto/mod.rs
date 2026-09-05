//! Cryptographic helpers for Bingle store-and-forward messaging.
//!
//! - [`key_convert`] — deterministic Ed25519 ⇄ X25519 conversion that maps Algorand identity keys
//!   into the X25519 domain used by hybrid public key encryption (HPKE).
//! - [`hpke_seal`] — the seal-at-rest primitive: single-shot HPKE seal/unseal to a recipient's
//!   X25519 public key.
//! - [`sealed_envelope`] — the versioned store-and-forward envelope with the inner Ed25519
//!   signature, built on top of [`hpke_seal`]. Its `seal` / `open` keep the module path (rather
//!   than being re-exported here) so they do not collide with [`hpke_seal::seal`].

pub mod hpke_seal;
pub mod key_convert;
pub mod sealed_envelope;

pub use hpke_seal::{ENC_LEN, SealError, UnsealError, seal, unseal};
pub use key_convert::{
    KeyConvertError, X25519Public, X25519Secret, ed25519_pub_to_x25519, ed25519_secret_to_x25519,
};
pub use sealed_envelope::{
    EnvelopeOpenError, EnvelopeParseError, EnvelopeSealError, InnerPayload, OpenedMessage,
    SealedEnvelope,
};
