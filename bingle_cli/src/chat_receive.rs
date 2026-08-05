//! Receive path for the `chat` command: turn an inbound engine message into a printed line and a
//! persisted state change.
//!
//! The engine (`bingle_core::BingleApiImpl`) delivers messages through an `on_message` callback;
//! `cmd_chat` installs a handler that locks the shared [`ChatState`] and calls [`receive_message`].
//! The parsing + persistence is factored out here so it can be unit-tested without a live engine,
//! feeding a JSON value to a local-file-backed [`ChatState`] and asserting the stored message and
//! contact.

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::chat_state::ChatState;

/// A plaintext message received from a peer, ready to display as `display_handle: text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedMessage {
    /// The sender's handle, or their id when the handle is unknown.
    pub display_handle: String,
    /// The message text.
    pub text: String,
}

/// Handle an inbound engine message.
///
/// When `message` is a plaintext `{ "text": ... }` message (the same shape `run --echo` uses — no
/// non-null `app`/`type`), this records the sender as a `Received` contact if it is new, appends the
/// message to the history, saves the state file, and returns the line to print. Non-plaintext or
/// text-less messages are ignored (`None`).
///
/// Persistence failures are logged, not propagated: a receive callback must never panic or tear down
/// the connection, so a save hiccup only costs durability of that one message.
pub fn receive_message(
    state: &mut ChatState,
    sender_id: &str,
    sender_handle: &str,
    message: &Value,
) -> Option<ReceivedMessage> {
    let text = message.get("text").and_then(|v| v.as_str())?;
    // Only plaintext messages: an app/type field marks a protocol message (ping, etc.), not chat.
    let is_plain = message.get("app").is_none_or(|v| v.is_null())
        && message.get("type").is_none_or(|v| v.is_null());
    if !is_plain {
        return None;
    }

    // Prefer the resolved handle; fall back to the raw id when the sender is not yet known.
    let display_handle = if sender_handle.trim().is_empty() {
        sender_id
    } else {
        sender_handle
    };

    // Record an unknown sender as a Received contact so later `--to <handle>` resolves them.
    if !state.knows_id(sender_id)
        && let Err(e) = state.add_received_contact(display_handle, sender_id)
    {
        tracing::warn!(
            "chat: could not record contact for '{}': {}",
            display_handle,
            e
        );
    }

    let recipient = state.opts.handle.clone();
    if let Err(e) = state.record_message(display_handle, vec![recipient], now_millis(), text, None)
    {
        tracing::warn!("chat: could not persist incoming message: {}", e);
    }
    if let Err(e) = state.save_state() {
        tracing::warn!("chat: could not save state after message: {}", e);
    }

    Some(ReceivedMessage {
        display_handle: display_handle.to_string(),
        text: text.to_string(),
    })
}

/// Milliseconds since the Unix epoch, for message timestamps. Clamps a pre-epoch clock to 0.
fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
