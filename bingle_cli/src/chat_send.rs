//! Outbound send + retry for the `chat` REPL, built on BingleLocal's pending-message model.
//!
//! Sending is abstracted behind [`MessageSender`] so the flow is unit-testable with a mock (no live
//! engine). Each outbound message is first persisted as **pending** (`progress < 1.0`) so a failed
//! send survives in the `--state_file` and can be retried later; [`send_once`] makes the first
//! attempt and [`retry_pending`] (driven periodically by `cmd_chat`) re-attempts anything still
//! pending until it delivers or a bounded number of attempts is exhausted.

use std::collections::HashMap;

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
/// treated as failures to retry.
pub trait MessageSender {
    fn send_text(&self, target: &SendTarget, message: &Value) -> Result<bool, String>;
}

/// Outcome of a single [`send_once`] attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendReport {
    Delivered,
    /// The attempt failed; the message is left pending for [`retry_pending`]. Carries the reason.
    Failed(String),
}

/// Result of re-attempting one pending message in [`retry_pending`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryResult {
    Delivered,
    /// Still failing but under the attempt cap — left pending. Carries the latest reason.
    Retrying(String),
    /// Attempt cap reached — marked permanently failed in the state. Carries the last reason.
    GaveUp(String),
}

/// A per-message retry outcome, for the transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryOutcome {
    pub timestamp: i64,
    pub recipient: String,
    pub result: RetryResult,
}

/// The reason string for a send that returned `Ok(false)` (no error, but not accepted).
const NOT_ACCEPTED: &str = "peer did not accept the message";

/// Persist an outbound message as pending, make one send attempt, and record the result.
///
/// On success the message is marked delivered; on failure it stays pending (with the reason) for
/// [`retry_pending`]. Returns the report so the caller can print an inline transcript line — note it
/// deliberately does not print or echo the sent text (the terminal already echoed it).
pub fn send_once(
    sender: &dyn MessageSender,
    state: &mut ChatState,
    target: &SendTarget,
    text: &str,
) -> SendReport {
    let ts = match state.queue_outbound(target.label(), text) {
        Ok(ts) => ts,
        Err(e) => return SendReport::Failed(format!("could not queue message: {e}")),
    };
    match sender.send_text(target, &json!({ "text": text })) {
        Ok(true) => {
            let _ = state.mark_delivered(ts);
            SendReport::Delivered
        }
        Ok(false) => {
            let _ = state.mark_send_failed(ts, NOT_ACCEPTED, false);
            SendReport::Failed(NOT_ACCEPTED.to_string())
        }
        Err(e) => {
            let _ = state.mark_send_failed(ts, &e, false);
            SendReport::Failed(e)
        }
    }
}

/// Re-attempt every still-pending outbound message once. `attempts` tracks tries per message
/// (keyed by timestamp) across calls; a message that reaches `max_attempts` failures is marked
/// permanently failed. Returns one [`RetryOutcome`] per message that changed state this call.
pub fn retry_pending(
    sender: &dyn MessageSender,
    state: &mut ChatState,
    attempts: &mut HashMap<i64, u32>,
    max_attempts: u32,
) -> Vec<RetryOutcome> {
    let pending = match state.pending_outbound() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };

    let mut outcomes = Vec::new();
    for msg in pending {
        let recipient = msg.recipient_handles.first().cloned().unwrap_or_default();
        // We stored the recipient label; retry by handle (the common case). An id-only recipient
        // whose label is not a registered handle simply keeps failing until it gives up.
        let target = SendTarget::Handle(recipient.clone());

        let result = match sender.send_text(&target, &json!({ "text": msg.text })) {
            Ok(true) => {
                let _ = state.mark_delivered(msg.timestamp);
                attempts.remove(&msg.timestamp);
                RetryResult::Delivered
            }
            other => {
                let reason = match other {
                    Ok(false) => NOT_ACCEPTED.to_string(),
                    Err(e) => e,
                    Ok(true) => unreachable!("handled above"),
                };
                let tries = attempts.entry(msg.timestamp).or_insert(0);
                *tries += 1;
                if *tries >= max_attempts {
                    let _ = state.mark_send_failed(msg.timestamp, &reason, true);
                    attempts.remove(&msg.timestamp);
                    RetryResult::GaveUp(reason)
                } else {
                    let _ = state.mark_send_failed(msg.timestamp, &reason, false);
                    RetryResult::Retrying(reason)
                }
            }
        };
        outcomes.push(RetryOutcome {
            timestamp: msg.timestamp,
            recipient,
            result,
        });
    }
    outcomes
}
