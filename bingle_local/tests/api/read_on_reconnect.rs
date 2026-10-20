//! Tests for store-and-forward read-on-reconnect (epic #200, story #215).
//!
//! The decrypt-and-store drain against a live Sidewinder node is exercised by the skip-clean
//! `read_on_reconnect_e2e` target; here we cover what needs no node: `poll_mailbox` is a no-op that
//! reads nothing when the receive gate is off, when no Sidewinder node is configured, or when there
//! is no keypair, and the poll outcome tells those apart from an unreachable node (issue #358).

use bingle_local::api::{
    BingleApiLocalImpl, BingleLocalApi, LocalApiConfig, MailboxConfig, MailboxPollStatus,
};

const TEST_MNEMONIC: &str = "square flat curtain negative three april hobby culture unit fit drip bronze cactus stage vault pluck captain nation pond pizza grief domain coin abstract path";

#[test]
fn poll_is_a_no_op_when_receive_gate_off() {
    // Node configured and a keypair present, but the receive gate is off (the default): no read.
    let config = LocalApiConfig {
        sidewinder: Some(MailboxConfig::new("http://localhost:9", "tok")),
        store_and_forward_receive: false,
        ..LocalApiConfig::default()
    };
    let mut api = BingleApiLocalImpl::new(config);
    api.import_keypair(TEST_MNEMONIC.to_string())
        .expect("import test keypair");

    let read = api
        .poll_mailbox()
        .expect("poll is infallible when gated off");
    assert!(
        read.is_empty(),
        "no messages are read when the receive gate is off"
    );
    assert!(
        api.get_messages().expect("messages").is_empty(),
        "nothing is stored"
    );
}

#[test]
fn poll_is_a_no_op_when_no_node_configured() {
    // Receive gate on but no Sidewinder node configured: nothing to poll.
    let config = LocalApiConfig {
        sidewinder: None,
        store_and_forward_receive: true,
        ..LocalApiConfig::default()
    };
    let mut api = BingleApiLocalImpl::new(config);
    api.import_keypair(TEST_MNEMONIC.to_string())
        .expect("import test keypair");

    let read = api.poll_mailbox().expect("poll is infallible with no node");
    assert!(read.is_empty());
}

#[test]
fn poll_is_a_no_op_without_a_keypair() {
    // Gate on and a node configured, but no keypair to open envelopes with: no read, no panic.
    let config = LocalApiConfig {
        sidewinder: Some(MailboxConfig::new("http://localhost:9", "tok")),
        store_and_forward_receive: true,
        ..LocalApiConfig::default()
    };
    let api = BingleApiLocalImpl::new(config);

    let read = api
        .poll_mailbox()
        .expect("poll is infallible without a keypair");
    assert!(read.is_empty());
}

/// A poll that does not run reports `Skipped`, so a cycling caller does not retry it early
/// (issue #358).
#[test]
fn poll_outcome_is_skipped_when_the_poll_does_not_run() {
    let gated_off = BingleApiLocalImpl::new(LocalApiConfig {
        sidewinder: Some(MailboxConfig::new("http://localhost:9", "tok")),
        store_and_forward_receive: false,
        ..LocalApiConfig::default()
    });
    let no_node = BingleApiLocalImpl::new(LocalApiConfig {
        sidewinder: None,
        store_and_forward_receive: true,
        ..LocalApiConfig::default()
    });
    let no_keypair = BingleApiLocalImpl::new(LocalApiConfig {
        sidewinder: Some(MailboxConfig::new("http://localhost:9", "tok")),
        store_and_forward_receive: true,
        ..LocalApiConfig::default()
    });

    for api in [gated_off, no_node, no_keypair] {
        let poll = api.poll_mailbox_outcome().expect("poll is infallible");
        assert_eq!(poll.status, MailboxPollStatus::Skipped);
        assert!(poll.messages.is_empty());
    }
}

/// A poll that cannot reach its node reports `Unreachable` rather than an empty Mailbox, so a
/// cycling caller retries soon (issue #358).
#[test]
fn poll_outcome_is_unreachable_when_the_node_cannot_be_reached() {
    let config = LocalApiConfig {
        // Nothing listens on the discard port.
        sidewinder: Some(MailboxConfig::new("http://localhost:9", "tok")),
        store_and_forward_receive: true,
        ..LocalApiConfig::default()
    };
    let mut api = BingleApiLocalImpl::new(config);
    api.import_keypair(TEST_MNEMONIC.to_string())
        .expect("import test keypair");

    let poll = api.poll_mailbox_outcome().expect("poll is infallible");
    assert!(
        matches!(poll.status, MailboxPollStatus::Unreachable(_)),
        "an unreachable node is not reported as an empty Mailbox: {:?}",
        poll.status
    );
    assert!(poll.messages.is_empty());
}
