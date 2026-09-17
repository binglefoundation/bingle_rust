//! Receive-side store-and-forward poll for the `chat` session (epic #200, issue #274).
//!
//! `bingle_local`'s [`poll_mailbox`](bingle_local::api::bingle_local_api::BingleLocalApi::poll_mailbox)
//! is caller-driven: it drains this account's Sidewinder Mailbox, decrypts and stores each held
//! message, but decides nothing about *when* to run. `bingle_jsi` drives it from the app foreground
//! lifecycle; `bingle_cli chat` has no such lifecycle, so this module supplies the CLI analogue — a
//! single poll the session runs on connect and again on a backstop cycle while it is up.
//!
//! The network drain itself (and its at-most-once / sorted-by-sent-time guarantees) is proven by the
//! `bingle_local` gated read-on-reconnect tests (#215); this module is only the *when*, kept small and
//! pure so the interval resolution and the receive-gate no-op are unit-testable without a live node.

use bingle_local::api::bingle_local_api::Message;
use std::time::Duration;

use crate::chat_state::ChatState;

/// Default period of the receive-side backstop Mailbox poll when `--poll-interval` is not given: 20s.
/// A backstop, not the primary path — real-time delivery normally beats it — so a short, responsive
/// cadence is fine for the interactive CLI (the JSI client uses a much longer production period).
pub const DEFAULT_MAILBOX_POLL_SECS: u64 = 20;

/// Resolve the backstop poll period from the optional `--poll-interval <secs>` (issue #274), falling
/// back to [`DEFAULT_MAILBOX_POLL_SECS`]. A zero value is clamped to the default: a 0s cycle would
/// busy-spin the poller thread.
pub fn resolve_poll_interval(poll_interval_secs: Option<u64>) -> Duration {
    match poll_interval_secs {
        Some(secs) if secs > 0 => Duration::from_secs(secs),
        _ => Duration::from_secs(DEFAULT_MAILBOX_POLL_SECS),
    }
}

/// Poll this account's Sidewinder Mailbox once, persist anything read, and return the batch read this
/// poll (sorted by sent time) for the caller to display like a real-time message.
///
/// A no-op returning an empty vector when the receive gate is off, so it is safe to call
/// unconditionally. Best-effort: a node/keypair problem surfaces as an empty batch (bingle_local logs
/// the detail); it never errors, so a poll failure cannot tear down the chat session. Read messages
/// are also recorded on the local history by `poll_mailbox`; on a non-empty read we save the state
/// file so they survive a restart. Real-time-delivered messages are not duplicated: the sender only
/// posts to the Mailbox on give-up (so a directly-delivered message was never posted), and the FIFO
/// read-and-drop is at-most-once, so a polled message is not re-read on the next cycle.
pub fn poll_once(state: &ChatState) -> Vec<Message> {
    if !state.store_and_forward_receive_enabled() {
        return Vec::new();
    }
    let read = match state.poll_mailbox() {
        Ok(read) => read,
        Err(e) => {
            tracing::warn!("chat: mailbox poll failed: {e}");
            return Vec::new();
        }
    };
    if !read.is_empty()
        && let Err(e) = state.save_state()
    {
        tracing::warn!("chat: could not save state after mailbox poll: {e}");
    }
    read
}
