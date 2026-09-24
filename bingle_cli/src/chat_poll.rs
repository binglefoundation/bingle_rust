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

use bingle_local::api::bingle_local_api::{BingleLocalApi, Message};
use bingle_local::api::bingle_local_api_impl::BingleApiLocalImpl;
use std::time::Duration;

use crate::chat_state::ChatState;

/// Default period of the receive-side backstop Mailbox poll when `--poll-interval` is not given: 20s.
/// A backstop, not the primary path — real-time delivery normally beats it — so a short, responsive
/// cadence is fine for the interactive CLI (the JSI client uses a much longer production period).
pub const DEFAULT_MAILBOX_POLL_SECS: u64 = 20;

/// Persist a shared local store handle to `state_file`, for the shutdown path (issue #276).
///
/// A no-op when no state file is configured. Runs on the shared `Arc<BingleApiLocalImpl>` so the
/// Ctrl-C/SIGTERM handler saves without taking the session lock, which a background poll can hold for
/// the length of its network wait; blocking the handler there was why the session would not exit.
pub fn save_shared(local: &BingleApiLocalImpl, state_file: Option<&str>) {
    if let Some(path) = state_file
        && let Err(e) = local.save(path)
    {
        tracing::warn!("chat: final save on shutdown failed: {e}");
    }
}

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
    let local = state.local_handle();
    poll_shared(&local, state.state_file_path().as_deref())
}

/// Poll a shared local store handle once, as [`poll_once`] does, but taking the store directly rather
/// than through a [`ChatState`].
///
/// This is the form the background poller (issue #274) uses: it holds a clone of the session's
/// `Arc<BingleApiLocalImpl>` (via [`ChatState::local_handle`](crate::chat_state::ChatState::local_handle))
/// and polls off the session lock. A poll's network wait can be up to 120s; running it on the shared
/// handle instead of under the session mutex keeps the REPL responsive and, crucially, lets
/// Ctrl-C/SIGTERM shut the session down promptly instead of blocking behind an in-flight poll
/// (issue #276). The store is fully interior-mutable, so a poll and a concurrent interactive send do
/// not corrupt each other.
pub fn poll_shared(local: &BingleApiLocalImpl, state_file: Option<&str>) -> Vec<Message> {
    if !local.store_and_forward_receive() {
        return Vec::new();
    }
    tracing::info!("chat: polling mailbox");
    let read = match local.poll_mailbox() {
        Ok(read) => read,
        Err(e) => {
            tracing::warn!("chat: mailbox poll failed: {e}");
            return Vec::new();
        }
    };
    if !read.is_empty()
        && let Some(path) = state_file
        && let Err(e) = local.save(path)
    {
        tracing::warn!("chat: could not save state after mailbox poll: {e}");
    }
    read
}
