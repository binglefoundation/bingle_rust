//! The `/register` envelope: its body type and the builder that signs it.
//!
//! Registers one APNs device token for the local handle so the gateway can fan out `/alert` nudges
//! to it. Signed with the shared canonical `bingle-notify:v1` signer
//! ([`AlgoOps::sign_notify_envelope`]) so the bytes match the gateway's `verify.ts` and the
//! committed cross-impl vector. No new crypto lives here.

use algo_ops::AlgoOps;
use bingle_core::api::bingle_api::BingleError;
use serde::Serialize;

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
    /// APNs topic (the app's bundle ID) the gateway pushes this token to. Absent means the
    /// gateway's default topic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
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
/// Swift bridge means the encoding can't be mis-captured on the way through. Modern APNs device
/// tokens are no longer a fixed 32 bytes, so length is not validated beyond rejecting an empty
/// token: APNs is the authority on validity (a bad token is pruned on the 410 at fan-out), matching
/// the gateway's `/register` contract (bingle_notify: relax token validation).
#[doc(hidden)]
pub fn encode_apns_token(raw: &[u8]) -> Result<String, BingleError> {
    if raw.is_empty() {
        return Err(BingleError::Other(
            "APNs device token must not be empty".to_string(),
        ));
    }
    let mut hex = String::with_capacity(raw.len() * 2);
    for byte in raw {
        hex.push_str(&format!("{byte:02x}"));
    }
    Ok(hex)
}

/// Sign the `/register` envelope for `token` under the local handle `iss`, returning the base64
/// signature.
///
/// The register `bodyHash` preimage is `token + "\n" + env`, with `"\n" + topic` appended when the
/// client names its APNs topic (the gateway's `bodyHash` in `verify.ts`). An absent topic and an
/// empty one differ: an empty topic still appends the delimiter. The shared signer joins its
/// `token` and `env` arguments with `"\n"`, so passing `env + "\n" + topic` as its env gives the
/// gateway's exact bytes. This is the one place that knows the topic preimage.
#[doc(hidden)]
pub fn sign_register_envelope(
    ops: &AlgoOps,
    iss: &str,
    token: &str,
    env: &str,
    topic: Option<&str>,
    nonce: &str,
    exp: i64,
) -> Result<String, BingleError> {
    let env_and_topic = match topic {
        Some(topic) => format!("{env}\n{topic}"),
        None => env.to_string(),
    };
    ops.sign_notify_envelope("register", iss, "", token, &env_and_topic, nonce, exp)
        .map_err(BingleError::from_anyhow)
}

/// Build and sign the `/register` envelope for `token` under the local handle `iss`.
///
/// `token` is the lowercase-hex device token; `env` is `"sandbox"`/`"production"`; `topic` is the
/// app's bundle ID, or `None` for the gateway's default. Signed by [`sign_register_envelope`], so
/// the bytes are byte-for-byte identical to the gateway's `verify.ts`. No new crypto.
#[doc(hidden)]
pub fn build_register_request(
    ops: &AlgoOps,
    iss: &str,
    token: &str,
    env: &str,
    topic: Option<&str>,
    nonce: &str,
    exp: i64,
) -> Result<RegisterRequest, BingleError> {
    let sig = sign_register_envelope(ops, iss, token, env, topic, nonce, exp)?;
    Ok(RegisterRequest {
        iss: iss.to_string(),
        token: token.to_string(),
        env: env.to_string(),
        topic: topic.map(str::to_string),
        nonce: nonce.to_string(),
        exp,
        sig,
    })
}
