//! The content-free `/alert` envelope: its body type, the signer, and the freshness helpers.
//!
//! The envelope is signed with the shared canonical `bingle-notify:v1` signer
//! ([`AlgoOps::sign_notify_envelope`]) so the bytes match the gateway's `verify.ts` and the
//! committed cross-impl vector. No new crypto lives here.

use base64::{Engine as _, engine::general_purpose};
use bingle_core::api::bingle_api::BingleError;
use bingle_core::blockchain::algo_ops::AlgoOps;
use serde::Serialize;

/// How far ahead the alert envelope's `exp` is set. Kept short (the contract allows up to 60s) so a
/// captured envelope cannot be replayed long after give-up.
const ALERT_EXP_SECS: i64 = 45;

/// Number of random bytes in a fresh nonce. The contract requires at least 16 bytes of entropy.
const NONCE_BYTES: usize = 16;

/// Body of a `POST /alert` to the notify gateway. Content-free: it carries no sender, preview, or
/// ciphertext — only the signed envelope identifying `(iss, audience)` and its freshness. Field
/// names match the gateway's request schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AlertRequest {
    /// The local handle that gave up (envelope issuer).
    pub iss: String,
    /// The recipient handle to wake.
    pub audience: String,
    /// Fresh per-request nonce (base64 of >=16 random bytes).
    pub nonce: String,
    /// Absolute expiry (unix seconds), a short window ahead of now.
    pub exp: i64,
    /// Base64 Ed25519 signature over the canonical `bingle-notify:v1` alert message.
    pub sig: String,
}

/// Build and sign the canonical content-free `/alert` envelope for `audience`.
///
/// `iss` is the local handle; `audience` is the recipient to wake. Uses the shared signer so the
/// bytes are byte-for-byte identical to the gateway's `verify.ts` and the committed cross-impl
/// vector (`route = "alert"`, `bodyHash = sha256("")`, `token`/`env` unused). No new crypto.
#[doc(hidden)]
pub fn build_alert_request(
    ops: &AlgoOps,
    iss: &str,
    audience: &str,
    nonce: &str,
    exp: i64,
) -> Result<AlertRequest, BingleError> {
    let sig = ops
        .sign_notify_envelope("alert", iss, audience, "", "", nonce, exp)
        .map_err(BingleError::from_anyhow)?;
    Ok(AlertRequest {
        iss: iss.to_string(),
        audience: audience.to_string(),
        nonce: nonce.to_string(),
        exp,
        sig,
    })
}

/// A fresh nonce: base64 (no padding) of [`NONCE_BYTES`] random bytes. Random per request so the
/// gateway can reject replays.
#[doc(hidden)]
pub fn fresh_nonce() -> String {
    let mut bytes = [0u8; NONCE_BYTES];
    // getrandom draws from the OS CSPRNG; on the astronomically unlikely failure fall back to a
    // time-derived value rather than panic in the best-effort give-up path.
    if getrandom::getrandom(&mut bytes).is_err() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        bytes[..16].copy_from_slice(&now.to_le_bytes());
    }
    general_purpose::STANDARD_NO_PAD.encode(bytes)
}

/// The alert envelope expiry: a short window ([`ALERT_EXP_SECS`]) ahead of `now_secs`.
#[doc(hidden)]
pub fn alert_exp(now_secs: i64) -> i64 {
    now_secs + ALERT_EXP_SECS
}

/// Current unix time in seconds.
#[doc(hidden)]
pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Whether a gateway HTTP status counts as an accepted best-effort alert. A `2xx` (delivered/queued)
/// and the gateway's coalescing/rate-limiting `429` are all fine; any other status is treated as a
/// rejection that is logged and ignored — never retried (the nudge fires only once, on give-up).
#[doc(hidden)]
pub fn alert_status_accepted(status: u16) -> bool {
    (200..300).contains(&status) || status == 429
}
