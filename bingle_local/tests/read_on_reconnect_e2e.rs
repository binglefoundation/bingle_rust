//! End-to-end read-on-reconnect for store-and-forward (epic #200, story #215): a sealed message
//! sitting in a user's Sidewinder Mailbox is read, decrypted, and stored by `poll_mailbox`, and is
//! then gone from the Mailbox (read-and-drop).
//!
//! Like the other Sidewinder integration tests, this carries no infrastructure and **skips cleanly**
//! (passes without running) when the node is not configured, so the default test run stays green
//! without a Sidewinder node. One account plays both sender and recipient: it seals a message to its
//! own identity, posts it to its own Mailbox, then polls and reads it back.
//!
//! Required environment (all must be set to run):
//!
//! | Variable | Meaning |
//! |---|---|
//! | `SIDEWINDER_NODE_URL` | Base URL of a Sidewinder node. |
//! | `SIDEWINDER_TOKEN` | Bearer token the node accepts. |
//! | `SIDEWINDER_ACCOUNT_MNEMONIC` | 25-word mnemonic of the (enrolled) account. |
//! | `SANDF_APP_ID` | Optional Bingle app id for sender-handle resolution; defaults to 0 (falls back to the address). |
//!
//! Run, for example:
//!
//! `SIDEWINDER_NODE_URL=... SIDEWINDER_TOKEN=... SIDEWINDER_ACCOUNT_MNEMONIC="..." cargo test -p bingle_local --test read_on_reconnect_e2e -- --nocapture`

use algo_ops::AlgoOps;
use bingle_local::api::sidewinder::{Mailbox, MailboxConfig};
use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig};

fn non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.trim().is_empty())
}

#[test]
fn poll_reads_decrypts_and_drops_a_mailbox_message() {
    let (Some(node_url), Some(token), Some(mnemonic)) = (
        non_empty("SIDEWINDER_NODE_URL"),
        non_empty("SIDEWINDER_TOKEN"),
        non_empty("SIDEWINDER_ACCOUNT_MNEMONIC"),
    ) else {
        eprintln!(
            "skipping poll_reads_decrypts_and_drops_a_mailbox_message: set SIDEWINDER_NODE_URL, \
             SIDEWINDER_TOKEN and SIDEWINDER_ACCOUNT_MNEMONIC to run (see the module docs)"
        );
        return;
    };
    let app_id: u64 = non_empty("SANDF_APP_ID")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let address = AlgoOps::address_from_passphrase(&mnemonic).expect("address from mnemonic");
    let private_key = AlgoOps::seed_from_passphrase(&mnemonic).expect("private key from mnemonic");
    let recipient_pub = algo_ops::address_to_byte_key(&address).expect("address to key");

    // A Mailbox client for posting (and for draining first, so we count only what we post).
    let poster_algo = AlgoOps::new_for_algorand(Some(mnemonic.clone()), None, None);
    let poster = Mailbox::new(
        poster_algo,
        MailboxConfig::new(node_url.clone(), token.clone()),
    )
    .expect("poster mailbox");
    loop {
        match poster.pop() {
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(e) => {
                eprintln!("skipping: node not reachable/ready: {e}");
                return;
            }
        }
    }

    // Seal a message to our own identity and post it to our own Mailbox.
    let text = "read-on-reconnect #215";
    let sent_time = 1_726_500_000_000;
    let sealed = bingle_core::crypto::sealed_envelope::seal_from_private_key(
        private_key,
        recipient_pub,
        sent_time,
        text,
    )
    .expect("seal");
    poster.post(&address, &sealed).expect("post to own mailbox");

    // A recipient client configured to read store-and-forward messages.
    let config = LocalApiConfig {
        app_id,
        sidewinder: Some(MailboxConfig::new(node_url, token)),
        store_and_forward_receive: true,
        ..LocalApiConfig::default()
    };
    let mut recipient = BingleApiLocalImpl::new(config);
    recipient
        .import_keypair(mnemonic)
        .expect("import recipient keypair");

    // Poll: the held message is read, decrypted, and stored.
    let read = recipient.poll_mailbox().expect("poll");
    assert_eq!(read.len(), 1, "exactly the one posted message is read");
    assert_eq!(read[0].text, text, "the decrypted text matches");
    assert_eq!(
        read[0].sent_time,
        Some(sent_time),
        "sender-stamped sent time is carried"
    );
    assert!(
        read[0].delivered_time.is_some(),
        "a delivered time is stamped"
    );
    assert!(
        recipient
            .get_messages()
            .expect("messages")
            .iter()
            .any(|m| m.text == text),
        "the message is on the local store"
    );

    // Read-and-drop: a second poll finds nothing (the message is gone from the Mailbox).
    let again = recipient.poll_mailbox().expect("second poll");
    assert!(
        again.is_empty(),
        "the message is not re-read on the next poll"
    );
}
