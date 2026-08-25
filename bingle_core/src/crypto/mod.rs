//! Cryptographic helpers for Bingle store-and-forward messaging.
//!
//! - [`key_convert`] — deterministic Ed25519 ⇄ X25519 conversion that maps Algorand identity keys
//!   into the X25519 domain used by hybrid public key encryption (HPKE).
//! - [`hpke_seal`] — the seal-at-rest primitive: single-shot HPKE seal/unseal to a recipient's
//!   X25519 public key.

pub mod hpke_seal;
pub mod key_convert;

pub use hpke_seal::{ENC_LEN, SealError, UnsealError, seal, unseal};
pub use key_convert::{
    KeyConvertError, X25519Public, X25519Secret, ed25519_pub_to_x25519, ed25519_secret_to_x25519,
};
