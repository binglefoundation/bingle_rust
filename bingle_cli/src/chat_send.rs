//! Outbound send + retry for the `chat` REPL, built on BingleLocal's pending-message model.
//!
//! Follows the same retry policy the React Native client uses (`bingle_jsi`): an outbound message is
//! first persisted as **pending** (`progress < 1.0`) so a failed send survives in the `--state_file`;
//! a **transient** (connectivity) failure keeps the message pending and is retried **indefinitely**
//! with a short per-message backoff, so it delivers once the recipient comes back online; only a
//! **non-transient** failure is marked permanently failed. The classifier, human-readable reason and
//! fair-scheduling selection are shared via `bingle_local::api::send_retry` (issue #82).
//!
//! Sending is abstracted behind [`MessageSender`] so the flow is unit-testable without a live engine.

use std::collections::HashMap;
use std::time::Instant;

use bingle_core::api::bingle_api::{BingleError, SendFailureKind};
use bingle_local::api::send_retry::{
    RETRY_BACKOFF, SendFailure, classify_send_error, select_sendable_message,
};
use serde_json::{Value, json};

use crate::chat_state::ChatState;

/// Where to send: a handle (resolved by the engine) or a raw id/address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendTarget {
    Handle(String),
    Id(String),
}

impl SendTarget {
    /// The recipient label stored in the message log and shown in the transcript.
    pub fn label(&self) -> &str {
        match self {
            SendTarget::Handle(h) => h,
            SendTarget::Id(id) => id,
        }
    }
}

/// Abstraction over the engine's send calls, so send/retry logic is testable without a live engine.
/// `Ok(true)` = delivered, `Ok(false)` = not accepted, `Err` = a send error; the last two are both
/// treated as failures (and classified transient vs permanent). Returns the typed [`BingleError`]
/// so the failure cause survives to the classifier (issue #99).
pub trait MessageSender {
    fn send_text(&self, target: &SendTarget, message: &Value) -> Result<bool, BingleError>;
}

/// Outcome of an attempt (from [`send_once`] or one [`retry_pending`] step).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendOutcome {
    /// Delivered and marked complete.
    Delivered,
    /// The direct send failed but, with the store-and-forward send gate on, the sealed message was
    /// posted to the recipient's Sidewinder Mailbox (bingle_local post-on-give-up, #214); the peer
    /// reads it on reconnect. Marked complete — no direct retry (issue #272). Carries the
    /// human-readable reason the direct send failed, for the transcript.
    Forwarded(String),
    /// A transient failure: the message stays pending and will keep being retried. Carries the
    /// human-readable reason.
    Retrying(String),
    /// A permanent failure (non-transient, or retries disabled): marked failed in the state.
    Failed(String),
}

/// A per-message retry outcome from [`retry_pending`], for the transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryOutcome {
    pub timestamp: i64,
    pub recipient: String,
    pub outcome: SendOutcome,
}

/// Classify a non-delivered send result into a typed [`SendFailure`] (issue #99). Should only be
/// called for `Ok(false)`/`Err` results; a delivered `Ok(true)` has no failure, so it falls back to
/// an `Unknown` cause defensively.
fn failure_of(result: &Result<bool, BingleError>) -> SendFailure {
    classify_send_error(result).unwrap_or_else(|| SendFailure {
        kind: SendFailureKind::Unknown,
        reason: "unknown send failure".to_string(),
    })
}

/// Classify a failed send, persist the message accordingly, and return the outcome — routing through
/// store-and-forward when the send gate is on (issue #272).
///
/// A retryable failure (per [`SendFailureKind::is_retryable`]) stays pending (`progress 0.0`) to be
/// retried; a permanent one is marked terminal (`progress 1.0`). When `retries_enabled` is false
/// (`--no-retries`) every failure is treated as permanent so nothing lingers pending.
///
/// Persisting the failure ([`ChatState::mark_send_failed`]) is what drives the bingle_local
/// post-on-give-up path (#214): with the store-and-forward SEND gate on it seals the message to the
/// recipient's Sidewinder Mailbox and, once fully handed off, marks it complete. So after persisting
/// we check whether the message was handed off: if so the forward *is* the fallback — report
/// [`SendOutcome::Forwarded`] and stop retrying; otherwise (gate off, or the post could not
/// complete) fall back to retry/queue exactly as before, so nothing is silently dropped.
fn classify_and_persist(
    state: &mut ChatState,
    timestamp: i64,
    failure: SendFailure,
    retries_enabled: bool,
) -> SendOutcome {
    let transient = retries_enabled && failure.kind.is_retryable();
    let _ = state.mark_send_failed(timestamp, &failure.reason, Some(failure.kind), !transient);

    if state.store_and_forward_send_enabled() && state.is_handed_off(timestamp) {
        SendOutcome::Forwarded(failure.reason)
    } else if transient {
        SendOutcome::Retrying(failure.reason)
    } else {
        SendOutcome::Failed(failure.reason)
    }
}

/// Persist an outbound message as pending, make one send attempt, and record the result.
///
/// On success the message is marked delivered. On a failure the outcome is decided by
/// `classify_and_persist`: with the store-and-forward send gate on and the message handed off to
/// the recipient's Mailbox it is [`SendOutcome::Forwarded`]; otherwise a transient failure stays
/// pending for [`retry_pending`] to keep retrying, and a permanent failure (or any failure under
/// `--no-retries`) is marked failed. Does not echo the sent text — the terminal already echoed it.
pub fn send_once(
    sender: &dyn MessageSender,
    state: &mut ChatState,
    target: &SendTarget,
    text: &str,
    retries_enabled: bool,
) -> SendOutcome {
    let ts = match state.queue_outbound(target.label(), text) {
        Ok(ts) => ts,
        Err(e) => return SendOutcome::Failed(format!("could not queue message: {e}")),
    };
    let result = sender.send_text(target, &json!({ "text": text }));
    if matches!(result, Ok(true)) {
        let _ = state.mark_delivered(ts);
        return SendOutcome::Delivered;
    }
    classify_and_persist(state, ts, failure_of(&result), retries_enabled)
}

/// Re-attempt the oldest *eligible* pending outbound message (respecting per-message backoff), one
/// per call. `retry_after` holds backoff deadlines keyed by timestamp and is maintained across
/// calls. A transient failure keeps the message pending and backs it off by [`RETRY_BACKOFF`] (so it
/// keeps retrying without starving newer messages); a permanent failure marks it failed. Returns the
/// outcome for the message attempted, or `None` when nothing is eligible right now.
pub fn retry_pending(
    sender: &dyn MessageSender,
    state: &mut ChatState,
    retry_after: &mut HashMap<i64, Instant>,
    now: Instant,
) -> Option<RetryOutcome> {
    let pending = state.pending_outbound().ok()?;
    // Drop backoff deadlines for messages that are no longer pending, keeping the map bounded.
    retry_after.retain(|ts, _| pending.iter().any(|m| m.timestamp == *ts));

    let msg = select_sendable_message(pending, retry_after, now)?;
    let recipient = msg.recipient_handles.first().cloned().unwrap_or_default();
    // We stored the recipient label; retry by handle (the common case).
    let target = SendTarget::Handle(recipient.clone());

    let result = sender.send_text(&target, &json!({ "text": msg.text }));
    let outcome = if matches!(result, Ok(true)) {
        let _ = state.mark_delivered(msg.timestamp);
        retry_after.remove(&msg.timestamp);
        SendOutcome::Delivered
    } else {
        // The retry worker only runs when retries are enabled, so a retryable failure is transient
        // here. `classify_and_persist` records the failure — which drives the store-and-forward
        // post-on-give-up (#214) — and reports [`SendOutcome::Forwarded`] when the message was handed
        // off to the Mailbox (issue #272).
        let outcome = classify_and_persist(state, msg.timestamp, failure_of(&result), true);
        // Back a still-transient message off so it keeps retrying without starving newer messages;
        // a forwarded, delivered or permanently-failed message needs no further attempts.
        match outcome {
            SendOutcome::Retrying(_) => {
                retry_after.insert(msg.timestamp, now + RETRY_BACKOFF);
            }
            _ => {
                retry_after.remove(&msg.timestamp);
            }
        }
        outcome
    };
    Some(RetryOutcome {
        timestamp: msg.timestamp,
        recipient,
        outcome,
    })
}
