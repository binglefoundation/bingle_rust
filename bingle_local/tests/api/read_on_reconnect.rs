//! Tests for store-and-forward read-on-reconnect (epic #200, story #215).
//!
//! The decrypt-and-store drain against a live Sidewinder node is exercised by the skip-clean
//! `read_on_reconnect_e2e` target; here we cover what needs no node: `poll_mailbox` is a no-op that
//! reads nothing when the receive gate is off, when no Sidewinder node is configured, or when there
//! is no keypair.

use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig, MailboxConfig};

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
