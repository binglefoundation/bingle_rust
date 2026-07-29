//! Give-up nudge to the notify gateway (bingle_notify #11).
//!
//! When `bingle_local`'s store-and-retry loop gives up delivering a message to a recipient, it
//! POSTs a content-free `/alert` to the notify gateway for that recipient so their device(s) wake
//! and the end-to-end flow can retry while both are online. Putting this in `bingle_local` means
//! every client built on the core gets it for free, including non-RN apps.
//!
//! The envelope is signed with the shared canonical `bingle-notify:v1` signer
//! ([`AlgoOps::sign_notify_envelope`]) so the bytes match the gateway's `verify.ts` and the
//! committed cross-impl vector. The POST is best-effort and must never block or fail message
//! delivery.

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

/// Best-effort delivery of an `/alert` POST to the notify gateway.
///
/// Implementations must **not block** the caller: the nudge fires from the message give-up path and
/// may never delay or fail delivery. The default [`HttpAlertPoster`] fires the request on a
/// detached thread and returns immediately. This is a seam so tests can observe the request without
/// a live gateway.
pub trait AlertPoster: Send + Sync {
    /// Send `body` to `{gateway_url}/alert`. Best-effort: any non-2xx status or transport error is
    /// logged by the implementation and swallowed — never surfaced to the caller.
    fn post_alert(&self, gateway_url: &str, body: AlertRequest);
}

/// Build and sign the canonical content-free `/alert` envelope for `audience`.
///
/// `iss` is the local handle; `audience` is the recipient to wake. Uses the shared signer so the
/// bytes are byte-for-byte identical to the gateway's `verify.ts` and the committed cross-impl
/// vector (`route = "alert"`, `bodyHash = sha256("")`, `token`/`env` unused). No new crypto.
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
pub fn alert_exp(now_secs: i64) -> i64 {
    now_secs + ALERT_EXP_SECS
}

/// Whether a gateway HTTP status counts as an accepted best-effort alert. A `2xx` (delivered/queued)
/// and the gateway's coalescing/rate-limiting `429` are all fine; any other status is treated as a
/// rejection that is logged and ignored — never retried (the nudge fires only once, on give-up).
pub fn alert_status_accepted(status: u16) -> bool {
    (200..300).contains(&status) || status == 429
}

/// Current unix time in seconds.
pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Default [`AlertPoster`]: fires the POST on a detached thread using a blocking HTTP client so the
/// give-up path never blocks on the network. A non-2xx (other than the gateway's coalescing
/// `429`) or a transport error is logged and swallowed.
///
/// The blocking client is built lazily inside the detached thread, never in the constructor: a
/// `reqwest::blocking` client owns a Tokio runtime, and creating or dropping one on a thread that is
/// already inside an async runtime (e.g. the webserver) panics. The detached thread is a plain OS
/// thread with no ambient runtime, so it is always safe there.
pub struct HttpAlertPoster;

impl HttpAlertPoster {
    pub fn new() -> Self {
        Self
    }
}

impl Default for HttpAlertPoster {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertPoster for HttpAlertPoster {
    fn post_alert(&self, gateway_url: &str, body: AlertRequest) {
        let endpoint = format!("{}/alert", gateway_url.trim_end_matches('/'));
        // Detach: the nudge is best-effort and must not delay or block message delivery. The
        // blocking client is built here, on this plain OS thread, so its runtime is never created
        // or dropped inside a caller's async context.
        let spawned = std::thread::Builder::new()
            .name("bingle-notify-alert".to_string())
            .spawn(move || {
                let client = match reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()
                {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(
                            "[notify][alert] could not build HTTP client for '{}': {}; dropping nudge",
                            body.audience,
                            e
                        );
                        return;
                    }
                };
                match client.post(&endpoint).json(&body).send() {
                    Ok(resp) => {
                        let status = resp.status().as_u16();
                        // 200/202 accepted; 429 is the gateway coalescing/rate-limiting per caller
                        // — all fine. Anything else is logged and ignored.
                        if alert_status_accepted(status) {
                            tracing::debug!(
                                "[notify][alert] gateway accepted alert for '{}' (status {})",
                                body.audience,
                                status
                            );
                        } else {
                            tracing::warn!(
                                "[notify][alert] gateway rejected alert for '{}' (status {}); ignoring",
                                body.audience,
                                status
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "[notify][alert] transport error posting alert for '{}': {}; ignoring",
                            body.audience,
                            e
                        );
                    }
                }
            });
        if let Err(e) = spawned {
            // Failing to spawn the detached thread is itself best-effort: log and drop the nudge.
            tracing::warn!(
                "[notify][alert] could not spawn alert thread: {}; dropping nudge",
                e
            );
        }
    }
}
