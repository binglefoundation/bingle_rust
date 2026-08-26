//! End-to-end post-on-delivery-fail for store-and-forward (epic #200, story #214): a send whose
//! direct delivery fails posts the sealed message to the recipient's Sidewinder Mailbox exactly
//! once, and a retry does not double-post.
//!
//! Like the other Sidewinder integration tests, this carries no infrastructure and **skips cleanly**
//! (passes without running) when the node/accounts are not configured, so the default test run stays
//! green without a Sidewinder node and registered accounts. The full offline→post→reconnect→read
//! path across two clients is the dedicated end-to-end story (#216); this test covers the #214 slice:
//! the sender posts once on give-up, and the recipient's Mailbox then holds exactly one message.
//!
//! Required environment (all must be set to run):
//!
//! | Variable | Meaning |
//! |---|---|
//! | `SIDEWINDER_NODE_URL` | Base URL of a Sidewinder node. |
//! | `SIDEWINDER_TOKEN` | Bearer token the node accepts. |
//! | `SANDF_SENDER_MNEMONIC` | 25-word mnemonic of the (enrolled) sender account. |
//! | `SANDF_RECIPIENT_MNEMONIC` | 25-word mnemonic of the recipient account (to drain its Mailbox). |
//! | `SANDF_RECIPIENT_HANDLE` | The recipient's on-chain registered handle (resolved to their address). |
//! | `SANDF_APP_ID` | The Bingle application id the recipient handle is registered under. |
//!
//! The Algorand node defaults to the localnet `AlgoChainConfig::default()`; override the app id via
//! `SANDF_APP_ID`. Run, for example:
//!
//! `SIDEWINDER_NODE_URL=... SIDEWINDER_TOKEN=... SANDF_SENDER_MNEMONIC="..." SANDF_RECIPIENT_MNEMONIC="..." SANDF_RECIPIENT_HANDLE=bob SANDF_APP_ID=1234 cargo test -p bingle_local --test post_on_delivery_fail_e2e -- --nocapture`

use algo_ops::AlgoOps;
use bingle_local::api::sidewinder::{Mailbox, MailboxConfig};
use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig};

struct E2eEnv {
    node_url: String,
    token: String,
    sender_mnemonic: String,
    recipient_mnemonic: String,
    recipient_handle: String,
    app_id: u64,
}

fn env() -> Option<E2eEnv> {
    Some(E2eEnv {
        node_url: non_empty("SIDEWINDER_NODE_URL")?,
        token: non_empty("SIDEWINDER_TOKEN")?,
        sender_mnemonic: non_empty("SANDF_SENDER_MNEMONIC")?,
        recipient_mnemonic: non_empty("SANDF_RECIPIENT_MNEMONIC")?,
        recipient_handle: non_empty("SANDF_RECIPIENT_HANDLE")?,
        app_id: non_empty("SANDF_APP_ID")?.parse().ok()?,
    })
}

fn non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.trim().is_empty())
}

#[test]
fn give_up_posts_exactly_once_to_the_recipient_mailbox() {
    let Some(env) = env() else {
        eprintln!(
            "skipping give_up_posts_exactly_once_to_the_recipient_mailbox: set SIDEWINDER_NODE_URL, \
             SIDEWINDER_TOKEN, SANDF_SENDER_MNEMONIC, SANDF_RECIPIENT_MNEMONIC, \
             SANDF_RECIPIENT_HANDLE and SANDF_APP_ID to run (see the module docs)"
        );
        return;
    };

    // Drain the recipient's Mailbox first, so the assertion counts only what this test posts.
    let recipient_algo =
        AlgoOps::new_for_algorand(Some(env.recipient_mnemonic.clone()), None, None);
    let recipient_box = Mailbox::new(
        recipient_algo,
        MailboxConfig::new(env.node_url.clone(), env.token.clone()),
    )
    .expect("recipient mailbox");
    match drain(&recipient_box) {
        Ok(_) => {}
        Err(reason) => {
            eprintln!("skipping: recipient Mailbox not reachable/ready: {reason}");
            return;
        }
    }

    // A sender configured for store-and-forward send against the same node.
    let config = LocalApiConfig {
        app_id: env.app_id,
        sidewinder: Some(MailboxConfig::new(env.node_url.clone(), env.token.clone())),
        store_and_forward_send: true,
        ..LocalApiConfig::default()
    };
    let mut sender = BingleApiLocalImpl::new(config);
    sender
        .import_keypair(env.sender_mnemonic.clone())
        .expect("import sender keypair");

    // A message to the recipient that fails direct delivery. The timestamp is its stable id.
    let ts = 1_726_000_000_000;
    sender
        .add_message(
            "sender".to_string(),
            vec![env.recipient_handle.clone()],
            ts,
            "store-and-forward post-on-fail #214".to_string(),
            None,
        )
        .expect("add message");

    // Two failed status updates (as retries would drive): the post must happen exactly once.
    let fail = || Some("Recipient unreachable — will keep retrying".to_string());
    sender
        .update_message_status(ts, 0.5, fail(), None)
        .expect("first failure");
    sender
        .update_message_status(ts, 0.5, fail(), None)
        .expect("second failure (retry)");

    // The recipient's Mailbox now holds exactly one message.
    let first = recipient_box
        .pop()
        .expect("pop")
        .expect("one message forwarded");
    assert!(!first.is_empty(), "the forwarded message is non-empty");
    let second = recipient_box.pop().expect("pop again");
    assert!(
        second.is_none(),
        "exactly one message was posted despite two delivery failures (no double-post)"
    );
}

/// Pop until the Mailbox is empty, returning how many messages were drained, or the failure reason
/// (for a clean skip) if the node is not reachable.
fn drain(mailbox: &Mailbox) -> Result<usize, String> {
    let mut count = 0;
    loop {
        match mailbox.pop() {
            Ok(Some(_)) => count += 1,
            Ok(None) => return Ok(count),
            Err(e) => return Err(e.to_string()),
        }
    }
}
