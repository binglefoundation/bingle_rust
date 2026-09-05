//! Seal-at-rest primitive: encrypt opaque bytes to a recipient's X25519 public key so only the
//! recipient can open them.
//!
//! This is single-shot hybrid public key encryption (HPKE, RFC 9180) in base mode over one fixed
//! cipher suite:
//!
//! - key encapsulation mechanism (KEM): `DHKEM(X25519, HKDF-SHA256)`
//! - key derivation function (KDF): HKDF-SHA256
//! - authenticated encryption with associated data (AEAD): ChaCha20-Poly1305 (RFC 8439)
//!
//! [`seal`] draws a fresh ephemeral key per call (one HPKE epoch per message) and returns the
//! encapsulated key alongside the ciphertext; [`unseal`] reverses it. There is no envelope framing
//! or signature here — those are later store-and-forward stories (issue #199).
//!
//! Reusing the recipient's durable Ed25519 identity key (converted to X25519 in
//! [`crate::crypto::key_convert`]) for key exchange is safe precisely because `DHKEM` runs the
//! Diffie-Hellman output through the KDF's extract/expand before it is used as key material
//! (Thormarker, IACR ePrint 2021/509).
//!
//! The suite is assembled from the reviewed [`hpke`] crate rather than by hand-combining a KEM, KDF
//! and AEAD; that crate is validated against the RFC 9180 known-answer test vectors.

use hpke::aead::ChaCha20Poly1305;
use hpke::kdf::HkdfSha256;
use hpke::kem::X25519HkdfSha256;
use hpke::{Deserializable, Kem as KemTrait, OpModeR, OpModeS, Serializable};
use rand_core::OsRng;

use crate::crypto::key_convert::{X25519Public, X25519Secret};

/// The fixed KEM: `DHKEM(X25519, HKDF-SHA256)`.
type Kem = X25519HkdfSha256;
/// The fixed KDF: HKDF-SHA256.
type Kdf = HkdfSha256;
/// The fixed AEAD: ChaCha20-Poly1305.
type Aead = ChaCha20Poly1305;

/// Length in bytes of the encapsulated key produced by [`seal`] (the X25519 KEM emits a 32-byte
/// ephemeral public key).
pub const ENC_LEN: usize = 32;

/// HPKE `info` (context) string for this story: empty. Context binding — for example a
/// cipher-suite name — is a later store-and-forward story and deliberately not applied here, so
/// both directions must agree that `info` is empty.
const HPKE_INFO: &[u8] = b"";

/// Error returned by [`seal`].
#[derive(Debug, thiserror::Error)]
pub enum SealError {
    /// The recipient's X25519 public key was rejected by the KEM (for example a low-order point).
    #[error("invalid recipient public key: {0}")]
    InvalidRecipientPublic(hpke::HpkeError),
    /// The HPKE encryption itself failed.
    #[error("HPKE seal failed: {0}")]
    Hpke(hpke::HpkeError),
}

/// Error returned by [`unseal`].
#[derive(Debug, thiserror::Error)]
pub enum UnsealError {
    /// The recipient's X25519 secret bytes were rejected by the KEM.
    #[error("invalid recipient private key: {0}")]
    InvalidRecipientPrivate(hpke::HpkeError),
    /// The encapsulated key bytes were not a valid KEM encapsulation.
    #[error("invalid encapsulated key: {0}")]
    InvalidEncappedKey(hpke::HpkeError),
    /// Decryption failed: a wrong recipient key, tampered ciphertext, or mismatched associated
    /// data — all surface here as an authentication failure, and the three are indistinguishable
    /// by design.
    #[error("HPKE unseal failed (wrong key, tampered ciphertext, or wrong associated data): {0}")]
    Hpke(hpke::HpkeError),
}

/// Seals `plaintext` to `recipient_pub`, binding `aad` (associated data) into the authentication
/// tag, and returns the ephemeral encapsulated key together with the ciphertext.
///
/// A fresh ephemeral keypair is drawn from the operating system RNG on every call, so sealing the
/// same plaintext twice yields different outputs. Only the holder of the matching X25519 secret can
/// [`unseal`] the result, and only with the identical `aad`.
///
/// # Errors
///
/// Returns [`SealError::InvalidRecipientPublic`] if the KEM rejects `recipient_pub`, or
/// [`SealError::Hpke`] if encryption fails.
pub fn seal(
    recipient_pub: &X25519Public,
    aad: &[u8],
    plaintext: &[u8],
) -> Result<([u8; ENC_LEN], Vec<u8>), SealError> {
    let pk_recip = <Kem as KemTrait>::PublicKey::from_bytes(recipient_pub.as_bytes())
        .map_err(SealError::InvalidRecipientPublic)?;

    let (encapped_key, ciphertext) = hpke::single_shot_seal::<Aead, Kdf, Kem, _>(
        &OpModeS::Base,
        &pk_recip,
        HPKE_INFO,
        plaintext,
        aad,
        &mut OsRng,
    )
    .map_err(SealError::Hpke)?;

    // The X25519 encapsulated key is exactly `ENC_LEN` bytes for this fixed suite.
    let mut enc = [0u8; ENC_LEN];
    enc.copy_from_slice(&encapped_key.to_bytes());
    Ok((enc, ciphertext))
}

/// Unseals a ciphertext produced by [`seal`], returning the recovered plaintext.
///
/// `recipient_priv` must be the X25519 secret matching the public key the message was sealed to,
/// and `aad` must equal the associated data supplied at seal time; otherwise authentication fails.
///
/// # Errors
///
/// Returns [`UnsealError::InvalidEncappedKey`] if `enc` is not a valid encapsulation, or
/// [`UnsealError::Hpke`] if authentication fails — a wrong key, a tampered `ciphertext`, or a
/// mismatched `aad` are indistinguishable and all reported here.
pub fn unseal(
    recipient_priv: &X25519Secret,
    enc: &[u8; ENC_LEN],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, UnsealError> {
    // Our own secret bytes are always a valid X25519 private key, so this conversion does not fail
    // in practice; it is still checked rather than unwrapped.
    let sk_recip = <Kem as KemTrait>::PrivateKey::from_bytes(&recipient_priv.to_bytes())
        .map_err(UnsealError::InvalidRecipientPrivate)?;
    let encapped_key =
        <Kem as KemTrait>::EncappedKey::from_bytes(enc).map_err(UnsealError::InvalidEncappedKey)?;

    hpke::single_shot_open::<Aead, Kdf, Kem>(
        &OpModeR::Base,
        &sk_recip,
        &encapped_key,
        HPKE_INFO,
        ciphertext,
        aad,
    )
    .map_err(UnsealError::Hpke)
}
