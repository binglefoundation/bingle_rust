//! The `/register` envelope: its body type and the builder that signs it.
//!
//! Registers one APNs device token for the local handle so the gateway can fan out `/alert` nudges
//! to it. Signed with the shared canonical `bingle-notify:v1` signer
//! ([`AlgoOps::sign_notify_envelope`]) so the bytes match the gateway's `verify.ts` and the
//! committed cross-impl vector. No new crypto lives here.

use bingle_core::api::bingle_api::BingleError;
use bingle_core::blockchain::algo_ops::AlgoOps;
use serde::Serialize;

/// APNs device tokens are 32 bytes. The gateway rejects any other length as `BadDeviceToken`, so we
/// reject a mis-sized token here — before signing or sending — rather than register a dead token.
pub const APNS_TOKEN_BYTES: usize = 32;

/// Body of a `POST /register` to the notify gateway. Field names match the gateway's request schema
/// (`bingle_notify/src/handlers/register.ts`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegisterRequest {
    /// The local handle registering a device (envelope issuer).
    pub iss: String,
    /// Lowercase-hex APNs device token (64 hex chars).
    pub token: String,
    /// APNs environment the token belongs to: `"sandbox"` or `"production"`.
    pub env: String,
    /// Fresh per-request nonce (base64 of >=16 random bytes).
    pub nonce: String,
    /// Absolute expiry (unix seconds), a short window ahead of now.
    pub exp: i64,
    /// Base64 Ed25519 signature over the canonical `bingle-notify:v1` register message.
    pub sig: String,
}

/// Hex-encode a raw APNs device token exactly as APNs expects it (lowercase, no separators).
///
/// This is the single place the raw `Data` from iOS becomes a token string — keeping it out of the
/// Swift bridge is why the 80-byte-token bug (a mis-encoded capture) cannot recur. Errors if the
/// token is not [`APNS_TOKEN_BYTES`] long.
pub fn encode_apns_token(raw: &[u8]) -> Result<String, BingleError> {
    if raw.len() != APNS_TOKEN_BYTES {
        return Err(BingleError::Other(format!(
            "APNs device token must be {APNS_TOKEN_BYTES} bytes, got {}",
            raw.len()
        )));
    }
    let mut hex = String::with_capacity(raw.len() * 2);
    for byte in raw {
        hex.push_str(&format!("{byte:02x}"));
    }
    Ok(hex)
}

/// Build and sign the `/register` envelope for `token` under the local handle `iss`.
///
/// `token` is the lowercase-hex device token; `env` is `"sandbox"`/`"production"`. Uses the shared
/// signer so the bytes are byte-for-byte identical to the gateway's `verify.ts`
/// (`route = "register"`, `bodyHash = sha256(token + "\n" + env)`). No new crypto.
pub fn build_register_request(
    ops: &AlgoOps,
    iss: &str,
    token: &str,
    env: &str,
    nonce: &str,
    exp: i64,
) -> Result<RegisterRequest, BingleError> {
    let sig = ops
        .sign_notify_envelope("register", iss, "", token, env, nonce, exp)
        .map_err(BingleError::from_anyhow)?;
    Ok(RegisterRequest {
        iss: iss.to_string(),
        token: token.to_string(),
        env: env.to_string(),
        nonce: nonce.to_string(),
        exp,
        sig,
    })
}
