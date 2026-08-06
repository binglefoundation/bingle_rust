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

use bingle_local::api::send_retry::{
    RETRY_BACKOFF, is_transient_send_failure, pending_failure_reason, select_sendable_message,
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
/// treated as failures (and classified transient vs permanent).
pub trait MessageSender {
    fn send_text(&self, target: &SendTarget, message: &Value) -> Result<bool, String>;
}

/// Outcome of an attempt (from [`send_once`] or one [`retry_pending`] step).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendOutcome {
    /// Delivered and marked complete.
    Delivered,
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

/// The error string used when a send returns `Ok(false)`. Phrased so the shared classifier treats it
/// as transient (the peer simply did not accept it this time).
const SEND_RETURNED_FALSE: &str = "Send returned false";

/// Classify a failed send and persist the message accordingly, returning the outcome.
///
/// A transient failure (per [`is_transient_send_failure`]) stays pending (`progress 0.0`) to be
/// retried; a permanent one is marked terminal (`progress 1.0`). When `retries_enabled` is false
/// (`--no-retries`) every failure is treated as permanent so nothing lingers pending.
fn classify_and_persist(
    state: &mut ChatState,
    timestamp: i64,
    err: &str,
    retries_enabled: bool,
) -> SendOutcome {
    let transient = retries_enabled && is_transient_send_failure(err);
    let reason = pending_failure_reason(err, transient);
    if transient {
        let _ = state.mark_send_failed(timestamp, &reason, false);
        SendOutcome::Retrying(reason)
    } else {
        let _ = state.mark_send_failed(timestamp, &reason, true);
        SendOutcome::Failed(reason)
    }
}

/// Extract the error string from a non-delivered send result (`Ok(false)` or `Err`).
fn failure_reason(result: Result<bool, String>) -> String {
    match result {
        Ok(false) => SEND_RETURNED_FALSE.to_string(),
        Err(e) => e,
        Ok(true) => unreachable!("delivered results are handled before this"),
    }
}

/// Persist an outbound message as pending, make one send attempt, and record the result.
///
/// On success the message is marked delivered. On a transient failure it stays pending for
/// [`retry_pending`] to keep retrying; on a permanent failure (or when `retries_enabled` is false) it
/// is marked failed. Does not echo the sent text — the terminal already echoed it.
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
    classify_and_persist(state, ts, &failure_reason(result), retries_enabled)
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
        let err = failure_reason(result);
        let transient = is_transient_send_failure(&err);
        let reason = pending_failure_reason(&err, transient);
        if transient {
            let _ = state.mark_send_failed(msg.timestamp, &reason, false);
            retry_after.insert(msg.timestamp, now + RETRY_BACKOFF);
            SendOutcome::Retrying(reason)
        } else {
            let _ = state.mark_send_failed(msg.timestamp, &reason, true);
            retry_after.remove(&msg.timestamp);
            SendOutcome::Failed(reason)
        }
    };
    Some(RetryOutcome {
        timestamp: msg.timestamp,
        recipient,
        outcome,
    })
}
