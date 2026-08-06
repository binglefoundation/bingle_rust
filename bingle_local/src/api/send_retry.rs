//! Shared outbound-send retry policy for the pending-message model.
//!
//! Both the React Native client (via `bingle_jsi`) and `bingle_cli chat` drive the same policy:
//! a send that fails for a **transient** (connectivity) reason keeps the message pending and retries
//! it forever with a short per-message backoff; only a **non-transient** failure is marked
//! permanently failed. Keeping the classifier, the human-readable reason, and the fair-scheduling
//! selection here means both clients behave identically (issue #82).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::api::bingle_local_api::Message;

/// A transient-failed message backs off this long before it is eligible to retry again, so a
/// repeatedly-failing recipient does not starve newer messages (head-of-line blocking).
pub const RETRY_BACKOFF: Duration = Duration::from_secs(10);

/// Whether a failed pending-message send looks transient (i.e. connectivity-related) and so the
/// message should stay pending to be retried, rather than being marked permanently failed.
///
/// Recognises retryable errors, undelivered sends, and no-route/no-relay/unreachable conditions.
/// Matches `retryable` anywhere (not just as a prefix) so both the JSI worker's `"Retryable: …"`
/// and `bingle_core`'s `BingleError::Retryable` display (`"Retryable error: …"`, e.g. a relay
/// `dtls client connect timeout` while the peer is briefly offline) are treated as transient.
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
pub fn pending_failure_reason(err: &str, transient: bool) -> String {
    if transient {
        "Recipient unreachable — will keep retrying".to_string()
    } else {
        format!("Message failed to send: {err}")
    }
}

/// Pick the oldest pending message eligible to send right now.
///
/// A message that recently transient-failed carries a `retry_after` deadline; until that deadline
/// passes it is skipped so one repeatedly-failing recipient (e.g. an offline peer) can't starve
/// newer messages behind it — the scheduler always drains the *oldest* message, so without this a
/// stuck head of queue would block everything (head-of-line blocking). Messages with no recorded
/// deadline are always eligible.
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
