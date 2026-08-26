//! The versioned store-and-forward envelope: the actual bytes posted to the Sidewinder Mailbox.
//!
//! This wraps the raw HPKE seal/unseal primitive ([`crate::crypto::hpke_seal`]) into the message
//! format described in the store-and-forward design (§3.3 / §3.4 / §7): a versioned binary frame
//! carrying the sender's authenticity signature and the ordering metadata, all opaque to the
//! sidechain.
//!
//! # Wire format
//!
//! ```text
//! SealedEnvelope = version:u8 | suite_id:u16(BE) | enc:[32] | ciphertext(AEAD(InnerPayload)+16-tag)
//! InnerPayload   = sender_id:[32] | sent_time:i64(BE) | message_id:[16] | signature:[64] | text
//! ```
//!
//! `version` and `suite_id` sit in the clear framing (and are bound into the AEAD's associated
//! data, so they cannot be swapped without breaking authentication); everything else lives inside
//! the ciphertext. Keeping `sender_id` / `sent_time` / `message_id` under the AEAD avoids leaking
//! the social graph and timing to sidechain observers and makes `sent_time` tamper-proof. The
//! recipient id is not on the wire — the Mailbox owner implies it, and the reader reconstructs it
//! from its own key.
//!
//! # Authenticity
//!
//! The [`InnerPayload`] carries an Ed25519 signature, made with the sender's identity key over the
//! canonical field set `{ sender_id, recipient_id, text, sent_time }` (see
//! [`canonical_signed_message`]). This is the #94-aligned non-repudiation artifact: one signature
//! serves live, stored, and reported messages. `sender_id` *is* the sender's Ed25519 public key, so
//! [`open`] resolves and verifies it without a chain lookup.
//!
//! # Scope
//!
//! Posting to / reading from the Mailbox and replay-dedup bookkeeping are orchestration (issue
//! #200); surfacing the opened fields onto the local message record is a later story. Perfect
//! forward secrecy is a deferred follow-up — the `version` tag reserves room for it.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand_core::{OsRng, RngCore};

use crate::crypto::hpke_seal::{self, ENC_LEN};
use crate::crypto::key_convert::{ed25519_pub_to_x25519, ed25519_secret_to_x25519};

/// Current envelope framing version. [`open`] rejects any other value.
pub const ENVELOPE_VERSION: u8 = 1;

/// Cipher-suite identifier for HPKE base mode over `DHKEM(X25519, HKDF-SHA256)`, HKDF-SHA256, and
/// ChaCha20-Poly1305 — the one suite implemented by [`crate::crypto::hpke_seal`].
pub const SUITE_HPKE_X25519_HKDF_SHA256_CHACHA20POLY1305: u16 = 0x0001;

/// Length of an Algorand identity key (a 32-byte Ed25519 public key).
const ID_LEN: usize = 32;
/// Length of the per-message identifier used for the reader's replay dedup.
const MESSAGE_ID_LEN: usize = 16;
/// Length of an Ed25519 signature.
const SIGNATURE_LEN: usize = 64;
/// Length of a big-endian `i64` timestamp.
const SENT_TIME_LEN: usize = 8;

/// Fixed-size portion of an [`InnerPayload`] preceding the variable-length text.
const INNER_HEADER_LEN: usize = ID_LEN + SENT_TIME_LEN + MESSAGE_ID_LEN + SIGNATURE_LEN;
/// Fixed-size portion of a [`SealedEnvelope`] preceding the variable-length ciphertext.
const ENVELOPE_HEADER_LEN: usize = 1 + 2 + ENC_LEN;

/// Returns the human-readable cipher-suite name for `suite_id`, or `None` if it is not a suite this
/// build understands.
pub fn suite_name(suite_id: u16) -> Option<&'static str> {
    match suite_id {
        SUITE_HPKE_X25519_HKDF_SHA256_CHACHA20POLY1305 => {
            Some("HPKE[DHKEM(X25519, HKDF-SHA256), HKDF-SHA256, ChaCha20-Poly1305]")
        }
        _ => None,
    }
}

/// Builds the canonical byte encoding of the signed field set `{ sender_id, recipient_id, text,
/// sent_time }`, shared with the Engine message-signing story (issue #94).
///
/// The encoding is the plain concatenation `sender_id | recipient_id | sent_time(BE i64) | text`,
/// with `text` last so its variable length is unambiguous. It is signed with pure Ed25519 (no
/// separate prehash), which is what verifies it. `sender_id` and `recipient_id` are not both on the
/// wire — the verifier reconstructs `recipient_id` from the Mailbox owner (its own key).
pub fn canonical_signed_message(
    sender_id: &[u8; ID_LEN],
    recipient_id: &[u8; ID_LEN],
    sent_time: i64,
    text: &str,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(ID_LEN + ID_LEN + SENT_TIME_LEN + text.len());
    out.extend_from_slice(sender_id);
    out.extend_from_slice(recipient_id);
    out.extend_from_slice(&sent_time.to_be_bytes());
    out.extend_from_slice(text.as_bytes());
    out
}

/// The plaintext sealed inside a [`SealedEnvelope`]: the sender's identity, the ordering metadata,
/// the authenticity signature, and the message text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InnerPayload {
    /// Sender's Algorand identity (their 32-byte Ed25519 public key).
    pub sender_id: [u8; ID_LEN],
    /// Sender-asserted send time (epoch milliseconds), authenticated by the signature.
    pub sent_time: i64,
    /// Random per-message identifier for the reader's replay dedup.
    pub message_id: [u8; MESSAGE_ID_LEN],
    /// Ed25519 signature over [`canonical_signed_message`].
    pub signature: [u8; SIGNATURE_LEN],
    /// The message text.
    pub text: String,
}

impl InnerPayload {
    /// Builds an inner payload and signs it with `sender_key` over the canonical field set.
    ///
    /// `recipient_id` is the recipient's Algorand identity; it is folded into the signature but not
    /// stored in the payload (the reader reconstructs it from its own key).
    pub fn new_signed(
        sender_key: &SigningKey,
        recipient_id: &[u8; ID_LEN],
        sent_time: i64,
        message_id: [u8; MESSAGE_ID_LEN],
        text: String,
    ) -> InnerPayload {
        let sender_id = sender_key.verifying_key().to_bytes();
        let message = canonical_signed_message(&sender_id, recipient_id, sent_time, &text);
        let signature = sender_key.sign(&message).to_bytes();
        InnerPayload {
            sender_id,
            sent_time,
            message_id,
            signature,
            text,
        }
    }

    /// Serializes the payload to its canonical wire bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(INNER_HEADER_LEN + self.text.len());
        out.extend_from_slice(&self.sender_id);
        out.extend_from_slice(&self.sent_time.to_be_bytes());
        out.extend_from_slice(&self.message_id);
        out.extend_from_slice(&self.signature);
        out.extend_from_slice(self.text.as_bytes());
        out
    }

    /// Parses an inner payload from its wire bytes.
    ///
    /// # Errors
    ///
    /// Returns [`EnvelopeParseError::InnerTooShort`] if `bytes` is shorter than the fixed header, or
    /// [`EnvelopeParseError::InvalidText`] if the trailing text is not valid UTF-8.
    pub fn from_bytes(bytes: &[u8]) -> Result<InnerPayload, EnvelopeParseError> {
        if bytes.len() < INNER_HEADER_LEN {
            return Err(EnvelopeParseError::InnerTooShort);
        }
        let mut sender_id = [0u8; ID_LEN];
        sender_id.copy_from_slice(&bytes[0..ID_LEN]);

        let mut sent_time_bytes = [0u8; SENT_TIME_LEN];
        sent_time_bytes.copy_from_slice(&bytes[ID_LEN..ID_LEN + SENT_TIME_LEN]);
        let sent_time = i64::from_be_bytes(sent_time_bytes);

        let id_start = ID_LEN + SENT_TIME_LEN;
        let mut message_id = [0u8; MESSAGE_ID_LEN];
        message_id.copy_from_slice(&bytes[id_start..id_start + MESSAGE_ID_LEN]);

        let sig_start = id_start + MESSAGE_ID_LEN;
        let mut signature = [0u8; SIGNATURE_LEN];
        signature.copy_from_slice(&bytes[sig_start..sig_start + SIGNATURE_LEN]);

        let text = String::from_utf8(bytes[INNER_HEADER_LEN..].to_vec())
            .map_err(|_| EnvelopeParseError::InvalidText)?;

        Ok(InnerPayload {
            sender_id,
            sent_time,
            message_id,
            signature,
            text,
        })
    }

    /// Verifies the inner signature against `recipient_id` (the opener's own identity).
    ///
    /// # Errors
    ///
    /// Returns [`EnvelopeOpenError::BadSenderKey`] if `sender_id` is not a valid Ed25519 public key,
    /// or [`EnvelopeOpenError::BadSignature`] if the signature does not verify — including when any
    /// signed field (`sender_id`, `recipient_id`, `text`, `sent_time`) has been altered.
    pub fn verify(&self, recipient_id: &[u8; ID_LEN]) -> Result<(), EnvelopeOpenError> {
        let sender_key =
            VerifyingKey::from_bytes(&self.sender_id).map_err(EnvelopeOpenError::BadSenderKey)?;
        let message =
            canonical_signed_message(&self.sender_id, recipient_id, self.sent_time, &self.text);
        let signature = Signature::from_bytes(&self.signature);
        sender_key
            .verify(&message, &signature)
            .map_err(|_| EnvelopeOpenError::BadSignature)
    }
}

/// The versioned binary envelope posted to the Mailbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedEnvelope {
    /// Framing version (see [`ENVELOPE_VERSION`]).
    pub version: u8,
    /// Cipher-suite identifier (see [`suite_name`]).
    pub suite_id: u16,
    /// HPKE encapsulated key.
    pub enc: [u8; ENC_LEN],
    /// AEAD ciphertext over the [`InnerPayload`] (includes the 16-byte tag).
    pub ciphertext: Vec<u8>,
}

impl SealedEnvelope {
    /// Seals `inner` for `recipient_ed25519_pub` (a decoded Algorand address), producing the
    /// envelope. The version and suite id are bound into the AEAD's associated data.
    ///
    /// # Errors
    ///
    /// Returns [`EnvelopeSealError::KeyConvert`] if the recipient key is not a valid identity key,
    /// or [`EnvelopeSealError::Hpke`] if the HPKE seal fails.
    pub fn seal(
        recipient_ed25519_pub: [u8; ID_LEN],
        inner: &InnerPayload,
    ) -> Result<SealedEnvelope, EnvelopeSealError> {
        let recipient_x =
            ed25519_pub_to_x25519(recipient_ed25519_pub).map_err(EnvelopeSealError::KeyConvert)?;
        let aad = associated_data(
            ENVELOPE_VERSION,
            SUITE_HPKE_X25519_HKDF_SHA256_CHACHA20POLY1305,
        );
        let (enc, ciphertext) = hpke_seal::seal(&recipient_x, &aad, &inner.to_bytes())
            .map_err(EnvelopeSealError::Hpke)?;
        Ok(SealedEnvelope {
            version: ENVELOPE_VERSION,
            suite_id: SUITE_HPKE_X25519_HKDF_SHA256_CHACHA20POLY1305,
            enc,
            ciphertext,
        })
    }

    /// Decrypts the envelope with `recipient_ed25519_key`, returning the inner payload *without*
    /// verifying its signature (callers use [`InnerPayload::verify`], as [`open`] does).
    ///
    /// # Errors
    ///
    /// Returns [`EnvelopeOpenError::UnknownVersion`] / [`EnvelopeOpenError::UnknownSuite`] if the
    /// framing is not understood, [`EnvelopeOpenError::Unseal`] if AEAD authentication fails (wrong
    /// key or tampered ciphertext / framing), or [`EnvelopeOpenError::Parse`] if the decrypted inner
    /// bytes are malformed.
    pub fn unseal(
        &self,
        recipient_ed25519_key: &SigningKey,
    ) -> Result<InnerPayload, EnvelopeOpenError> {
        if self.version != ENVELOPE_VERSION {
            return Err(EnvelopeOpenError::UnknownVersion(self.version));
        }
        if suite_name(self.suite_id).is_none() {
            return Err(EnvelopeOpenError::UnknownSuite(self.suite_id));
        }
        let recipient_x = ed25519_secret_to_x25519(recipient_ed25519_key);
        let aad = associated_data(self.version, self.suite_id);
        let inner_bytes = hpke_seal::unseal(&recipient_x, &self.enc, &aad, &self.ciphertext)
            .map_err(EnvelopeOpenError::Unseal)?;
        InnerPayload::from_bytes(&inner_bytes).map_err(EnvelopeOpenError::Parse)
    }

    /// Serializes the envelope to its wire bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(ENVELOPE_HEADER_LEN + self.ciphertext.len());
        out.push(self.version);
        out.extend_from_slice(&self.suite_id.to_be_bytes());
        out.extend_from_slice(&self.enc);
        out.extend_from_slice(&self.ciphertext);
        out
    }

    /// Parses an envelope from its wire bytes. Structural only: version and suite id are checked at
    /// [`SealedEnvelope::unseal`] / [`open`] time.
    ///
    /// # Errors
    ///
    /// Returns [`EnvelopeParseError::EnvelopeTooShort`] if `bytes` is shorter than the fixed header.
    pub fn from_bytes(bytes: &[u8]) -> Result<SealedEnvelope, EnvelopeParseError> {
        if bytes.len() < ENVELOPE_HEADER_LEN {
            return Err(EnvelopeParseError::EnvelopeTooShort);
        }
        let version = bytes[0];
        let suite_id = u16::from_be_bytes([bytes[1], bytes[2]]);
        let mut enc = [0u8; ENC_LEN];
        enc.copy_from_slice(&bytes[3..3 + ENC_LEN]);
        let ciphertext = bytes[ENVELOPE_HEADER_LEN..].to_vec();
        Ok(SealedEnvelope {
            version,
            suite_id,
            enc,
            ciphertext,
        })
    }

    /// Returns this envelope's cipher-suite name, or `None` if the suite id is unknown.
    pub fn suite_name(&self) -> Option<&'static str> {
        suite_name(self.suite_id)
    }
}

/// A message recovered and verified by [`open`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedMessage {
    /// Verified sender identity (their Ed25519 public key / Algorand address bytes).
    pub sender_id: [u8; ID_LEN],
    /// Authenticated send time (epoch milliseconds).
    pub sent_time: i64,
    /// Per-message identifier for the reader's replay dedup (issue #200).
    pub message_id: [u8; MESSAGE_ID_LEN],
    /// The message text.
    pub text: String,
    /// The retained Ed25519 signature, for later attachment to a report.
    pub signature: [u8; SIGNATURE_LEN],
}

/// Seals `text` for `recipient_ed25519_pub`, signing it with `sender_ed25519_key`, and returns the
/// wire bytes of the [`SealedEnvelope`].
///
/// A random `message_id` is generated for the reader's replay dedup, and a fresh HPKE ephemeral is
/// drawn per call, so sealing the same text twice yields different bytes.
///
/// # Errors
///
/// Returns [`EnvelopeSealError`] if the recipient key is invalid or the HPKE seal fails.
pub fn seal(
    recipient_ed25519_pub: [u8; ID_LEN],
    sender_ed25519_key: &SigningKey,
    sent_time: i64,
    text: &str,
) -> Result<Vec<u8>, EnvelopeSealError> {
    let mut message_id = [0u8; MESSAGE_ID_LEN];
    OsRng.fill_bytes(&mut message_id);
    let inner = InnerPayload::new_signed(
        sender_ed25519_key,
        &recipient_ed25519_pub,
        sent_time,
        message_id,
        text.to_string(),
    );
    Ok(SealedEnvelope::seal(recipient_ed25519_pub, &inner)?.to_bytes())
}

/// Seals `text` for `recipient_ed25519_pub` using the sender's 32-byte Ed25519 private key
/// `sender_private_key` (the account secret, as returned by
/// `algo_ops::AlgoOps::seed_from_passphrase`), and returns the wire bytes of the [`SealedEnvelope`].
///
/// A convenience over [`seal`] for callers that hold the raw account private key rather than a
/// [`SigningKey`] — it builds the signing key from the private key so the caller (e.g.
/// `bingle_local`'s store-and-forward post, issue #214) need not depend on `ed25519_dalek`. The
/// private key's derived public key is the account's Algorand address, matching how
/// `address_from_passphrase` derives it.
///
/// # Errors
///
/// Returns [`EnvelopeSealError`] if the recipient key is invalid or the HPKE seal fails.
pub fn seal_from_private_key(
    sender_private_key: [u8; ID_LEN],
    recipient_ed25519_pub: [u8; ID_LEN],
    sent_time: i64,
    text: &str,
) -> Result<Vec<u8>, EnvelopeSealError> {
    let signing_key = SigningKey::from_bytes(&sender_private_key);
    seal(recipient_ed25519_pub, &signing_key, sent_time, text)
}

/// Opens the wire bytes of a [`SealedEnvelope`] with `recipient_ed25519_key`, verifying the sender's
/// signature, and returns the recovered [`OpenedMessage`].
///
/// # Errors
///
/// Returns [`EnvelopeOpenError`] on malformed framing, an unknown version or suite, a wrong
/// recipient key, tampered ciphertext / framing, malformed inner bytes, or a bad inner signature.
pub fn open(
    recipient_ed25519_key: &SigningKey,
    bytes: &[u8],
) -> Result<OpenedMessage, EnvelopeOpenError> {
    let envelope = SealedEnvelope::from_bytes(bytes).map_err(EnvelopeOpenError::Parse)?;
    let inner = envelope.unseal(recipient_ed25519_key)?;
    let recipient_id = recipient_ed25519_key.verifying_key().to_bytes();
    inner.verify(&recipient_id)?;
    Ok(OpenedMessage {
        sender_id: inner.sender_id,
        sent_time: inner.sent_time,
        message_id: inner.message_id,
        text: inner.text,
        signature: inner.signature,
    })
}

/// Opens the wire bytes of a [`SealedEnvelope`] with the recipient's 32-byte Ed25519 private key
/// `recipient_private_key` (the account secret, as returned by
/// `algo_ops::AlgoOps::seed_from_passphrase`), verifying the sender's signature, and returns the
/// recovered [`OpenedMessage`].
///
/// A convenience over [`open`] for callers that hold the raw account private key rather than a
/// [`SigningKey`], so the caller (e.g. `bingle_local`'s read-on-reconnect, issue #215) need not
/// depend on `ed25519_dalek`.
///
/// # Errors
///
/// Returns [`EnvelopeOpenError`] on the same conditions as [`open`].
pub fn open_with_private_key(
    recipient_private_key: [u8; ID_LEN],
    bytes: &[u8],
) -> Result<OpenedMessage, EnvelopeOpenError> {
    let signing_key = SigningKey::from_bytes(&recipient_private_key);
    open(&signing_key, bytes)
}

/// Associated data bound into the AEAD: the clear framing header `version | suite_id(BE)`, so the
/// two cannot be swapped without breaking authentication.
fn associated_data(version: u8, suite_id: u16) -> [u8; 3] {
    let [hi, lo] = suite_id.to_be_bytes();
    [version, hi, lo]
}

/// Error returned when sealing an envelope.
#[derive(Debug, thiserror::Error)]
pub enum EnvelopeSealError {
    /// The recipient's Ed25519 identity key could not be converted to X25519.
    #[error("invalid recipient identity key: {0}")]
    KeyConvert(crate::crypto::key_convert::KeyConvertError),
    /// The underlying HPKE seal failed.
    #[error("envelope seal failed: {0}")]
    Hpke(hpke_seal::SealError),
}

/// Error returned when parsing envelope or inner-payload bytes.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EnvelopeParseError {
    /// The envelope is shorter than its fixed `version | suite_id | enc` header.
    #[error("sealed envelope is too short")]
    EnvelopeTooShort,
    /// The decrypted inner payload is shorter than its fixed header.
    #[error("inner payload is too short")]
    InnerTooShort,
    /// The inner payload's trailing text is not valid UTF-8.
    #[error("inner payload text is not valid UTF-8")]
    InvalidText,
}

/// Error returned when opening an envelope.
#[derive(Debug, thiserror::Error)]
pub enum EnvelopeOpenError {
    /// The envelope or inner bytes were malformed.
    #[error("malformed envelope: {0}")]
    Parse(EnvelopeParseError),
    /// The framing version is not [`ENVELOPE_VERSION`].
    #[error("unknown envelope version: {0}")]
    UnknownVersion(u8),
    /// The suite id is not one this build understands.
    #[error("unknown cipher suite id: {0:#06x}")]
    UnknownSuite(u16),
    /// AEAD authentication failed: a wrong recipient key, or tampered ciphertext / framing.
    #[error("envelope decryption failed: {0}")]
    Unseal(hpke_seal::UnsealError),
    /// The recovered `sender_id` is not a valid Ed25519 public key.
    #[error("invalid sender identity key: {0}")]
    BadSenderKey(ed25519_dalek::SignatureError),
    /// The inner signature did not verify against the signed field set.
    #[error("inner signature verification failed")]
    BadSignature,
}
