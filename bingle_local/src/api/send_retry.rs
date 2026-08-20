//! Shared outbound-send retry policy for the pending-message model.
//!
//! Both the React Native client (via `bingle_jsi`) and `bingle_cli chat` drive the same policy:
//! a send that fails for a **transient** (connectivity) reason keeps the message pending and retries
//! it forever with a short per-message backoff; only a **non-transient** failure is marked
//! permanently failed. Keeping the classifier, the human-readable reason, and the fair-scheduling
//! selection here means both clients behave identically (issue #82).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use bingle_core::api::bingle_api::{BingleError, SendFailureKind};

use crate::api::bingle_local_api::Message;

/// A transient-failed message backs off this long before it is eligible to retry again, so a
/// repeatedly-failing recipient does not starve newer messages (head-of-line blocking).
#[doc(hidden)]
pub const RETRY_BACKOFF: Duration = Duration::from_secs(10);

/// Whether a failed pending-message send *string* looks transient (i.e. connectivity-related) and so
/// the message should stay pending to be retried, rather than being marked permanently failed.
///
/// Since issue #99 the primary, reliable classifier is [`classify_send_error`], which reads the
/// typed [`SendFailureKind`] directly. This string-based check remains as the keyword fallback for
/// legacy untyped errors (see [`classify_legacy_error`]) and for existing callers/tests. Recognises
/// retryable errors, undelivered sends, and no-route/no-relay/unreachable conditions; matches
/// `retryable` anywhere so `bingle_core`'s `BingleError::Retryable` display (`"Retryable error: …"`,
/// e.g. a relay `dtls client connect timeout` while the peer is briefly offline) is transient.
#[doc(hidden)]
pub fn is_transient_send_failure(err: &str) -> bool {
    let e = err.to_ascii_lowercase();
    e.contains("retryable")
        || e.contains("send returned false")
        || e.contains("no available relay")
        || e.contains("no relay")
        || e.contains("unreachable")
        || e.contains("no route")
        || e.contains("noconnection")
}

/// Map a raw send error to a concise, human-readable `failure_reason` for a queued message. A
/// transient (connectivity) failure keeps the message pending and retrying, so the reason says so
/// rather than leaking the internal error; a permanent failure surfaces the underlying error. The
/// raw error is still logged for debugging by the caller.
#[doc(hidden)]
pub fn pending_failure_reason(err: &str, transient: bool) -> String {
    if transient {
        "Recipient unreachable — will keep retrying".to_string()
    } else {
        format!("Message failed to send: {err}")
    }
}

/// A classified send failure: the reliable typed cause plus a concise, human-readable reason for
/// display (issue #99). `kind` drives retry and UX decisions ([`SendFailureKind::is_retryable`]);
/// `reason` is safe to surface to the user.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendFailure {
    pub kind: SendFailureKind,
    pub reason: String,
}

/// Classify the result of a single send attempt into a typed failure, or `None` when the message
/// was delivered (`Ok(true)`).
///
/// This is the reliable replacement for string-parsing the error (issue #99): when the send path
/// returns a typed [`BingleError::Send`], its `kind` is used directly. Delivered sends return
/// `None`. An `Ok(false)` (an internal guard rejected the send without an error — e.g. send-to-self
/// or an incomplete endpoint) and any legacy untyped error are mapped to a best-fit kind so callers
/// always get a classification.
#[doc(hidden)]
pub fn classify_send_error(result: &Result<bool, BingleError>) -> Option<SendFailure> {
    match result {
        Ok(true) => None,
        Ok(false) => Some(SendFailure {
            kind: SendFailureKind::Unknown,
            reason: humanize_failure(
                SendFailureKind::Unknown,
                "message not accepted by recipient",
            ),
        }),
        Err(BingleError::Send { kind, detail }) => Some(SendFailure {
            kind: *kind,
            reason: humanize_failure(*kind, detail),
        }),
        Err(other) => Some(classify_legacy_error(other)),
    }
}

/// Map an untyped legacy [`BingleError`] (one not produced through [`BingleError::Send`]) to a
/// best-fit [`SendFailure`]. Kept so a send path that has not yet been converted to typed causes,
/// or an error surfaced from a non-send layer, still classifies sensibly. Prefer typed
/// [`BingleError::Send`] at the source.
fn classify_legacy_error(err: &BingleError) -> SendFailure {
    let detail = err.to_string();
    let kind = match err {
        // A blockchain error during a send is a transient node/connectivity condition.
        BingleError::Algo(_) => SendFailureKind::NotReady,
        // Legacy transient marker.
        BingleError::Retryable(_) => SendFailureKind::PeerUnreachable,
        // Fall back to the keyword classifier for other untyped errors.
        _ if is_transient_send_failure(&detail) => SendFailureKind::PeerUnreachable,
        _ => SendFailureKind::Unknown,
    };
    SendFailure {
        kind,
        reason: humanize_failure(kind, &detail),
    }
}

/// Produce a concise, human-readable reason for a failure `kind`. Retryable causes phrase
/// themselves as "will keep retrying"; permanent causes state the stable condition. `detail` is the
/// underlying message, used only for the catch-all so nothing is silently swallowed.
fn humanize_failure(kind: SendFailureKind, detail: &str) -> String {
    match kind {
        SendFailureKind::HandleNotFound => "No account is registered for that handle".to_string(),
        SendFailureKind::HandleLookupFailed => {
            "Could not reach the network to look up the handle — will keep retrying".to_string()
        }
        SendFailureKind::RecipientNotAdvertised => {
            "Recipient is not connected right now — will keep retrying".to_string()
        }
        SendFailureKind::InvalidRecipientId => "The recipient address is not valid".to_string(),
        SendFailureKind::NoRelayAvailable => {
            "No relay available to route the message — will keep retrying".to_string()
        }
        SendFailureKind::RelayAllocationFailed => {
            "Could not open a relay channel to the recipient — will keep retrying".to_string()
        }
        // Preserves the wording used before typed causes existed.
        SendFailureKind::PeerUnreachable => {
            "Recipient unreachable — will keep retrying".to_string()
        }
        SendFailureKind::NoResponse => {
            "No response from the recipient — will keep retrying".to_string()
        }
        SendFailureKind::MalformedAdvert => {
            "The recipient's connection record is invalid".to_string()
        }
        SendFailureKind::ProtocolError => "Unexpected response from the network".to_string(),
        SendFailureKind::NotReady => "Not ready to send yet — will keep retrying".to_string(),
        SendFailureKind::Unknown => format!("Message failed to send: {detail}"),
    }
}

/// Pick the oldest pending message eligible to send right now.
///
/// A message that recently transient-failed carries a `retry_after` deadline; until that deadline
/// passes it is skipped so one repeatedly-failing recipient (e.g. an offline peer) can't starve
/// newer messages behind it — the scheduler always drains the *oldest* message, so without this a
/// stuck head of queue would block everything (head-of-line blocking). Messages with no recorded
/// deadline are always eligible.
#[doc(hidden)]
pub fn select_sendable_message(
    mut pending: Vec<Message>,
    retry_after: &HashMap<i64, Instant>,
    now: Instant,
) -> Option<Message> {
    pending.sort_by_key(|m| m.timestamp);
    pending.into_iter().find(|m| {
        retry_after
            .get(&m.timestamp)
            .map(|deadline| *deadline <= now)
            .unwrap_or(true)
    })
}
