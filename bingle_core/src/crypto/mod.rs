//! Cryptographic key-material helpers for Bingle store-and-forward messaging.
//!
//! Currently this is [`key_convert`], the deterministic Ed25519 ⇄ X25519 conversion that maps
//! Algorand identity keys into the X25519 domain used by hybrid public key encryption (HPKE).

pub mod key_convert;

pub use key_convert::{
    KeyConvertError, X25519Public, X25519Secret, ed25519_pub_to_x25519, ed25519_secret_to_x25519,
};
