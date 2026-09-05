//! End-to-end round-trip for the Sidewinder Mailbox wrapper (store-and-forward epic #200, story
//! #213): post a message to a Mailbox and pop it back through the [`Mailbox`] client, against a
//! running Sidewinder node.
//!
//! Like the `sidewinder_ops` integration harness, this test carries no infrastructure and **skips
//! cleanly** — it returns without failing — when the node is not configured or not reachable, so the
//! default test run stays green without a Sidewinder node. The node/localnet harness is still
//! evolving upstream, so this is the shape the real round-trip runs in until a node is routinely
//! available in continuous integration.
//!
//! Configure it from the environment:
//!
//! | Variable | Required | Meaning |
//! |---|---|---|
//! | `SIDEWINDER_NODE_URL` | yes | Base URL of a Sidewinder node, e.g. `http://localhost:9101`. |
//! | `SIDEWINDER_TOKEN` | yes | Bearer token the node's client endpoints accept. |
//! | `SIDEWINDER_ACCOUNT_MNEMONIC` | yes | 25-word Algorand mnemonic for an enrolled caller; its key signs the transactions. |
//! | `SIDEWINDER_POST_TYPE` | no | Transaction type bound to Mailbox `post`. Defaults to the tier-1 binding (1). |
//! | `SIDEWINDER_POP_TYPE` | no | Transaction type bound to Mailbox `pop`. Defaults to the tier-1 binding (2). |
//!
//! Run it with, for example:
//!
//! `SIDEWINDER_NODE_URL=http://localhost:9101 SIDEWINDER_TOKEN=dev-token SIDEWINDER_ACCOUNT_MNEMONIC="word1 ... word25" cargo test -p bingle_local --test sidewinder_mailbox_e2e -- --nocapture`

use algo_ops::AlgoOps;
use bingle_core::api::bingle_api::BingleError;
use bingle_local::api::sidewinder::{Mailbox, MailboxConfig};

/// The environment inputs for a configured run, or `None` when the required ones are unset.
struct NodeEnv {
    node_url: String,
    token: String,
    mnemonic: String,
    post_type: Option<u32>,
    pop_type: Option<u32>,
}

fn env_from_process() -> Option<NodeEnv> {
    let node_url = non_empty("SIDEWINDER_NODE_URL")?;
    let token = non_empty("SIDEWINDER_TOKEN")?;
    let mnemonic = non_empty("SIDEWINDER_ACCOUNT_MNEMONIC")?;
    Some(NodeEnv {
        node_url,
        token,
        mnemonic,
        post_type: parse_type("SIDEWINDER_POST_TYPE"),
        pop_type: parse_type("SIDEWINDER_POP_TYPE"),
    })
}

fn non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.trim().is_empty())
}

fn parse_type(key: &str) -> Option<u32> {
    non_empty(key).and_then(|s| s.parse().ok())
}

#[test]
fn post_then_pop_round_trips_a_message() {
    let Some(env) = env_from_process() else {
        eprintln!(
            "skipping post_then_pop_round_trips_a_message: set SIDEWINDER_NODE_URL, \
             SIDEWINDER_TOKEN and SIDEWINDER_ACCOUNT_MNEMONIC to run (see the module docs)"
        );
        return;
    };

    let algo = AlgoOps::new_for_algorand(Some(env.mnemonic.clone()), None, None);
    let own_address =
        AlgoOps::address_from_passphrase(&env.mnemonic).expect("derive address from mnemonic");

    let mut config = MailboxConfig::new(env.node_url, env.token);
    if let Some(t) = env.post_type {
        config.post_type = t;
    }
    if let Some(t) = env.pop_type {
        config.pop_type = t;
    }
    let mailbox = Mailbox::new(algo, config).expect("configured mailbox");

    // Post to our own mailbox (a caller pops its own queue), then read it back. A distinctive
    // payload so a pop of some other queued message would be caught.
    let message = b"bingle store-and-forward round-trip #213".to_vec();
    match mailbox.post(&own_address, &message) {
        Ok(()) => {}
        Err(BingleError::Retryable(reason)) => {
            eprintln!("skipping: sidewinder node not reachable/ready: {reason}");
            return;
        }
        Err(other) => panic!("post failed: {other}"),
    }

    let popped = mailbox
        .pop()
        .expect("pop succeeds")
        .expect("a message is queued");
    assert!(
        payload_matches(&popped, &message),
        "pop returns the posted message (raw or ARC-4 wrapped): got {popped:?}"
    );

    // The queue is now drained: a second pop returns nothing.
    let drained = mailbox.pop().expect("second pop succeeds");
    assert!(
        drained.is_none(),
        "the mailbox is empty after the message is read"
    );
}

/// Whether `popped` carries `message`, tolerating an Algorand Request for Comments 4 (ARC-4)
/// `byte[]` wrapper (a 2-byte big-endian length prefix) around the payload. The FIFO primitive
/// returns the stored value directly, but the declared `byte[]` return type may be length-prefixed
/// on the wire; pinning which is the read-on-reconnect story (#215), so this round-trip accepts
/// either and the actual envelope decoding lands there.
fn payload_matches(popped: &[u8], message: &[u8]) -> bool {
    if popped == message {
        return true;
    }
    if popped.len() == message.len() + 2 {
        let len = u16::from_be_bytes([popped[0], popped[1]]) as usize;
        return len == message.len() && &popped[2..] == message;
    }
    false
}
