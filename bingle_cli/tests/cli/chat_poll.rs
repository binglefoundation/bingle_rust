// Unit tests for the receive-side store-and-forward poll (bingle_cli::chat_poll), issue #274.
// The network drain itself is proven by the bingle_local gated `read_on_reconnect` tests; here we
// cover the interval resolution and the receive-gate no-op, which need no live node.
use std::time::Duration;

use bingle_cli::chat_poll::{DEFAULT_MAILBOX_POLL_SECS, poll_once, resolve_poll_interval};
use bingle_cli::chat_state::ChatState;
use bingle_core::api::bingle_api::StartOptions;
use bingle_local::api::MailboxConfig;
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};

/// A `ChatState` registered as "alice" whose local store has the store-and-forward RECEIVE gate set
/// as given and a Sidewinder Mailbox configured. Built from parts (bypassing the state-file bridge's
/// validation) so a test can exercise the poll path with an arbitrary config. Store is in-memory.
fn alice_state_receive(receive_gate: bool) -> ChatState {
    let mut local = BingleApiLocalImpl::new(LocalApiConfig {
        sidewinder: Some(MailboxConfig::new("http://localhost:9", "tok")),
        store_and_forward_receive: receive_gate,
        ..LocalApiConfig::default()
    });
    local.generate_keypair().expect("keypair");
    local.seed_own_handle_for_tests("alice".to_string());
    ChatState::from_parts_for_tests(local, StartOptions::new("alice".to_string()))
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn resolve_poll_interval_defaults_when_unset() {
    assert_eq!(
        resolve_poll_interval(None),
        Duration::from_secs(DEFAULT_MAILBOX_POLL_SECS)
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn resolve_poll_interval_honors_flag() {
    assert_eq!(resolve_poll_interval(Some(45)), Duration::from_secs(45));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn resolve_poll_interval_clamps_zero_to_default() {
    // A zero-second cycle would busy-spin; fall back to the default instead.
    assert_eq!(
        resolve_poll_interval(Some(0)),
        Duration::from_secs(DEFAULT_MAILBOX_POLL_SECS)
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn poll_once_is_noop_when_receive_gate_off() {
    // Receive gate off (e.g. `--store-forward send|none`): the poll does nothing and reaches no node,
    // so it returns an empty batch deterministically.
    let state = alice_state_receive(false);
    assert!(
        !state.store_and_forward_receive_enabled(),
        "gate should be off in this fixture"
    );
    assert!(
        poll_once(&state).is_empty(),
        "with the receive gate off, a poll reads nothing"
    );
}
