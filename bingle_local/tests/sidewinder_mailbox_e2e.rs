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
//! The round-trip resolves the node endpoint on-chain from the Bingle DApp app id and pins the node
//! identity over mutual TLS (story #244) — the only way to reach a node (#246). Configure it from the
//! environment:
//!
//! | Variable | Required | Meaning |
//! |---|---|---|
//! | `SIDEWINDER_DISCOVERY_APP_ID` | yes | Bingle DApp application id whose opted-in cluster nodes publish endpoints. |
//! | `SIDEWINDER_ACCOUNT_MNEMONIC` | yes | 25-word Algorand mnemonic for a permitted client; its key signs and is the mutual-TLS identity. |
//!
//! Run it with, for example:
//!
//! `SIDEWINDER_DISCOVERY_APP_ID=1234 SIDEWINDER_ACCOUNT_MNEMONIC="word1 ... word25" cargo test -p bingle_local --test sidewinder_mailbox_e2e -- --nocapture`

use algo_ops::AlgoOps;
use bingle_core::api::bingle_api::BingleError;
use bingle_local::api::sidewinder::{Mailbox, MailboxConfig};

fn non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.trim().is_empty())
}

/// Discovery + identity-pinned mutual TLS round-trip (story #244): resolve the node endpoint on-chain
/// from the Bingle DApp app id and connect over mutual TLS (no bearer token), then post to our own
/// Mailbox and read it back.
///
/// Skips cleanly unless `SIDEWINDER_DISCOVERY_APP_ID` and `SIDEWINDER_ACCOUNT_MNEMONIC` are set (and
/// the account is enrolled/opted-in, with the process able to reach the app's chain — an algod/indexer
/// the default `AlgoOps` config points at). A discovery scan or mutual-TLS connect that cannot reach a
/// node surfaces as [`BingleError::Retryable`] and is treated as a clean skip.
#[test]
fn discovery_mtls_post_then_pop_round_trips_a_message() {
    let (Some(app_id), Some(mnemonic)) = (
        non_empty("SIDEWINDER_DISCOVERY_APP_ID").and_then(|s| s.parse::<u64>().ok()),
        non_empty("SIDEWINDER_ACCOUNT_MNEMONIC"),
    ) else {
        eprintln!(
            "skipping discovery_mtls_post_then_pop_round_trips_a_message: set \
             SIDEWINDER_DISCOVERY_APP_ID and SIDEWINDER_ACCOUNT_MNEMONIC to run (see the module docs)"
        );
        return;
    };

    let algo = AlgoOps::new_for_algorand(Some(mnemonic.clone()), None, None);
    let own_address =
        AlgoOps::address_from_passphrase(&mnemonic).expect("derive address from mnemonic");

    // No node URL/token: the discovery connection resolves the endpoint on-chain and pins the node's
    // identity over mutual TLS. A discovery/connect failure is a clean skip (node/chain not ready).
    let mut mailbox = match Mailbox::new(algo, MailboxConfig::discovered(app_id)) {
        Ok(m) => m,
        Err(BingleError::Retryable(reason)) => {
            eprintln!("skipping: sidewinder discovery/connect not ready: {reason}");
            return;
        }
        Err(other) => panic!("discovery mailbox build failed: {other}"),
    };

    let message = b"bingle store-and-forward discovery + mTLS round-trip #244".to_vec();
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
