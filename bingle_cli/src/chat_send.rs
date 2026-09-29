//! Outbound sending for the `chat` REPL, built on BingleLocal's pending-message model.
//!
//! The REPL persists each typed message as **pending** in the `--state_file` and wakes the shared
//! background sender, [`PendingSender`](bingle_local::api::pending_sender::PendingSender) — the same
//! one the React Native client uses (issue #283). The sender attempts it straight away, keeps a
//! transient failure pending and retries it with per-message backoff, marks a permanent failure
//! terminal, and with the store-and-forward send gate on hands a failed message to the recipient's
//! Mailbox. It runs off the REPL thread and off the session lock, so the prompt returns immediately.
//!
//! This module holds the CLI's side of that: where to send ([`SendTarget`]), how a stored recipient
//! label is delivered ([`ChatDelivery`]), and how outcomes read in the transcript
//! ([`report_line`]).

use std::sync::Arc;

use bingle_core::api::bingle_api::{BingleApi, BingleError, ProgressCallback};
use bingle_local::api::pending_sender::{MessageDelivery, SendOutcome, SendReport};
use serde_json::Value as JsonValue;

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

/// Whether a stored recipient label is an account id (an Algorand address: 58 characters of
/// base32) rather than a handle. `--to-id` stores the id itself as the label.
pub fn is_account_id(label: &str) -> bool {
    label.len() == 58
        && label
            .bytes()
            .all(|b| b.is_ascii_uppercase() || (b'2'..=b'7').contains(&b))
}

/// [`MessageDelivery`] over the live engine: a label that is an account id is sent by id, anything
/// else by handle.
pub struct ChatDelivery<A: BingleApi> {
    api: Arc<A>,
}

impl<A: BingleApi> ChatDelivery<A> {
    /// Deliver through `api`.
    pub fn new(api: Arc<A>) -> Self {
        Self { api }
    }
}

impl<A: BingleApi> MessageDelivery for ChatDelivery<A> {
    fn deliver(
        &self,
        recipient: &str,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        let recipient = recipient.to_string();
        if is_account_id(&recipient) {
            self.api.send_message_to_id(&recipient, message, progress)
        } else {
            self.api
                .send_message_to_handle(&recipient, message, progress)
        }
    }
}

/// The transcript line for a send outcome, or `None` to stay quiet.
///
/// A delivery is silent unless an earlier attempt had failed (the terminal already echoed the typed
/// line); a transient failure is reported once, on the message's first failure, not on every retry.
pub fn report_line(report: &SendReport) -> Option<String> {
    let to = report.recipients.join(", ");
    match (&report.outcome, report.previously_failed) {
        (SendOutcome::Delivered, false) => None,
        (SendOutcome::Delivered, true) => Some(format!("✓ delivered to {to}")),
        (SendOutcome::Forwarded(_), _) => Some(format!(
            "↪ {to} is offline; queued to their mailbox — they'll get it when they reconnect"
        )),
        (SendOutcome::Retrying(reason), false) => Some(format!(
            "! send to {to} not delivered ({reason}); will keep retrying…"
        )),
        (SendOutcome::Retrying(_), true) => None,
        (SendOutcome::Failed(reason), _) => Some(format!("! send to {to} failed: {reason}")),
    }
}
