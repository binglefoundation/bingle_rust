//! Store-and-forward end-to-end against a real Sidewinder node on LocalNet (epic #200, story #216).
//!
//! A case in the `integration` suite, gated behind the `sidewinder_localnet` cargo feature so it is
//! **not** compiled or run by the unit/quality gates. The integration workflow turns the feature on
//! (issue #224, see CI-INTEGRATION.md). It stands up a one-node Sidewinder Mailbox network
//! on algokit LocalNet via `scripts/e2e_sidewinder_localnet.sh` (which drives the installed `sw-node`
//! binary), then drives the store-and-forward path against it. The node runs in `on-chain-identity`
//! mode on a freshly deployed Bingle app, and every client reaches it by app id only: discovery +
//! mutual TLS (#246). Per the LocalNet-suite policy it **fails, not skips**, when the prerequisites
//! are missing.
//!
//! Prerequisites (see the script header):
//!   - the pinned `sw-node` on PATH (`cargo install sidewinder-node --version 0.1.0 --locked`),
//!   - algokit LocalNet up (`algokit localnet start`), and `curl`.
//!
//! Run it explicitly (single-threaded — the node binds fixed ports):
//!   `cargo test -p bingle_cli --features sidewinder_localnet --test integration -- --test-threads 1`
//!
//! Stage A (this file) proves the harness + node: a message posted to a recipient's Mailbox is popped
//! back through the [`Mailbox`] client. Stage B (the full two-`bingle_local` offline-send → reconnect
//! flow with handle registration and the epic invariants) builds on this same harness.

use algo_ops::{AlgoChainConfig, AlgoOps};
use bingle_local::api::sidewinder::{Mailbox, MailboxConfig};
use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig};
use bingle_test::localnet::sidewinder::{
    SidewinderAccount, SidewinderAccounts, SidewinderScript, connect_mailbox, enrol_node,
};
use bingle_test::localnet::test_util;
use std::path::Path;
use std::time::Duration;

/// How long to wait for the node's published endpoint to become discoverable (indexer lag).
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(60);

/// A running one-node Sidewinder Mailbox network in `on-chain-identity` mode, on a freshly deployed
/// Bingle app, with the sender and receiver registered (which enrols them as Sidewinder clients).
/// Torn down (`script down`) on drop.
struct Cluster {
    script: SidewinderScript,
    accounts: SidewinderAccounts,
    app_id: u64,
    asset_id: u64,
}

impl Cluster {
    /// Generate the accounts, deploy the Bingle app, register `sender_handle` / `receiver_handle`,
    /// enrol the node, start it, and wait until both clients can reach it by app id. Fails (does not
    /// skip) with the script's diagnostics when a prerequisite is missing.
    fn up(sender_handle: &str, receiver_handle: &str) -> Cluster {
        let script = SidewinderScript::new(Path::new(env!("CARGO_TARGET_TMPDIR")).join("sw-e2e"));
        script.down();
        let accounts = script.accounts().unwrap_or_else(|e| panic!("{e}"));
        let cfg = test_util::localnet_config();

        let sender_ops = ops(&accounts.sender, &cfg);
        let (app_id, asset_id) =
            test_util::deploy_bingle_app_and_asset(&sender_ops, "Bingle$", 1_000_000);
        for (account, handle) in [
            (&accounts.sender, sender_handle),
            (&accounts.receiver, receiver_handle),
        ] {
            test_util::register_client_on_blockchain(
                &account.address,
                &account.mnemonic,
                handle,
                app_id,
                asset_id,
                &sender_ops,
                cfg.clone(),
            );
        }
        enrol_node(&cfg, app_id, &accounts.node).unwrap_or_else(|e| panic!("{e}"));

        let cluster = Cluster {
            script,
            accounts,
            app_id,
            asset_id,
        };
        cluster.script.up(app_id).unwrap_or_else(|e| panic!("{e}"));
        for account in [&cluster.accounts.sender, &cluster.accounts.receiver] {
            connect_mailbox(ops(account, &cfg), app_id, DISCOVERY_TIMEOUT).unwrap_or_else(|e| {
                panic!("{} cannot reach the node by app id: {e}", account.address)
            });
        }
        cluster
    }
}

impl Drop for Cluster {
    fn drop(&mut self) {
        // `down` removes the work dir, so keep the node log of a failing case beside it for the
        // integration workflow to upload (issue #224).
        if std::thread::panicking() {
            let _ = std::fs::copy(
                self.script.work().join("node.log"),
                Path::new(env!("CARGO_TARGET_TMPDIR")).join("sw-e2e-node.log"),
            );
        }
        self.script.down();
    }
}

/// A LocalNet [`AlgoOps`] signing as `account`.
fn ops(account: &SidewinderAccount, cfg: &AlgoChainConfig) -> AlgoOps {
    test_util::ops_from_mnemonic(&account.address, &account.mnemonic, cfg.clone())
}

/// Whether `popped` carries `message`, tolerating an ARC-4 `byte[]` length prefix around the payload
/// (the wire may length-prefix the declared `byte[]` return; the FIFO primitive stores it raw).
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

/// Stage A: a message posted to the receiver's Mailbox is popped back — the harness stands up a real
/// node and the [`Mailbox`] client round-trips a message through it.
#[serial_test::serial(sidewinder_e2e)]
#[test]
fn mailbox_round_trips_against_a_real_localnet_node() {
    let cluster = Cluster::up("rt-sender", "rt-receiver");
    let cfg = test_util::localnet_config();
    let accounts = &cluster.accounts;

    // Sender posts to the receiver's Mailbox (keyed by the receiver's address), reaching the node by
    // app id only.
    let mut sender = Mailbox::new(
        ops(&accounts.sender, &cfg),
        MailboxConfig::discovered(cluster.app_id),
    )
    .expect("sender mailbox");
    let message = b"store-and-forward end-to-end #216 (stage A)".to_vec();
    sender
        .post(&accounts.receiver.address, &message)
        .expect("post to the receiver's mailbox");

    // Receiver pops its own Mailbox and reads the message back.
    let mut receiver = Mailbox::new(
        ops(&accounts.receiver, &cfg),
        MailboxConfig::discovered(cluster.app_id),
    )
    .expect("receiver mailbox");
    let popped = receiver
        .pop()
        .expect("pop succeeds")
        .expect("a message is queued for the receiver");
    assert!(
        payload_matches(&popped, &message),
        "the receiver reads the posted message back: got {popped:?}"
    );

    // Read-and-drop: a second pop finds nothing.
    let drained = receiver.pop().expect("second pop succeeds");
    assert!(
        drained.is_none(),
        "the message is dropped from the sidechain and not re-read"
    );
}

/// Build a bingle_local config for LocalNet + this cluster: the localnet algod (with the deployed
/// app/asset ids) and the cluster's Sidewinder Mailbox, discovered from the app id, with
/// `send`/`receive` set as given.
fn local_config(app_id: u64, asset_id: u64, send: bool, receive: bool) -> LocalApiConfig {
    let algo_config = AlgoChainConfig {
        app_id: Some(app_id),
        asset_id: Some(asset_id),
        ..test_util::localnet_config()
    };
    LocalApiConfig {
        algo_config,
        app_id,
        asset_id,
        sidewinder: Some(MailboxConfig::discovered(app_id)),
        store_and_forward_send: send,
        store_and_forward_receive: receive,
        ..LocalApiConfig::default()
    }
}

/// Drive a failed direct delivery for message `timestamp` twice, as the retry loop would; the second
/// failure must not double-post (idempotency).
fn fail_delivery_twice(sender: &mut BingleApiLocalImpl, timestamp: i64) {
    let reason = || Some("Recipient unreachable — will keep retrying".to_string());
    let id = sender.id_of_timestamp_for_tests(timestamp);
    sender
        .update_message_status(&id, 0.5, reason(), None)
        .expect("first failure");
    sender
        .update_message_status(&id, 0.5, reason(), None)
        .expect("second failure (retry) — must not double-post");
}

/// Stage B: the full store-and-forward path across two `bingle_local` instances against a real node.
/// An offline send gives up and is posted to the recipient's Mailbox; the recipient reconnects,
/// polls, reads, decrypts, and the messages land on its store sorted by sent time and gone from the
/// sidechain — asserting the epic invariants (no double-post, no re-read, sorted by sent time).
#[serial_test::serial(sidewinder_e2e)]
#[test]
fn offline_send_survives_and_is_read_on_reconnect() {
    // The cluster registers a handle for the sender and the receiver, so the sender's give-up post
    // can resolve the recipient handle → address (and the reader can attribute the sender);
    // registering is also what enrols them as Sidewinder clients.
    let cluster = Cluster::up("sf-sender", "sf-receiver");
    let (app_id, asset_id) = (cluster.app_id, cluster.asset_id);
    let accounts = &cluster.accounts;

    // ── sender: two offline sends give up and are posted to the receiver's Mailbox ──────────────
    let mut sender = BingleApiLocalImpl::new(local_config(app_id, asset_id, true, false));
    sender
        .import_keypair(accounts.sender.mnemonic.clone())
        .expect("import sender keypair");

    // The earlier-sent message is added second and posted first, so a correct read must reorder by
    // sent time, not by Mailbox (FIFO) arrival order.
    let earlier_ts = 1_726_600_000_000;
    let later_ts = 1_726_600_005_000;
    sender
        .add_message(
            "sf-sender".to_string(),
            vec!["sf-receiver".to_string()],
            earlier_ts,
            "first, by sent time".to_string(),
            None,
        )
        .expect("add earlier message");
    sender
        .add_message(
            "sf-sender".to_string(),
            vec!["sf-receiver".to_string()],
            later_ts,
            "second, by sent time".to_string(),
            None,
        )
        .expect("add later message");

    fail_delivery_twice(&mut sender, later_ts);
    fail_delivery_twice(&mut sender, earlier_ts);

    // ── receiver: reconnect, poll, read, decrypt ─────────────────────────────────────────────────
    let mut receiver = BingleApiLocalImpl::new(local_config(app_id, asset_id, false, true));
    receiver
        .import_keypair(accounts.receiver.mnemonic.clone())
        .expect("import receiver keypair");

    let read = receiver.poll_mailbox().expect("poll");

    // No double-post: exactly two messages despite each delivery failing twice.
    assert_eq!(
        read.len(),
        2,
        "exactly the two posted messages are read (no double-post)"
    );
    // Sorted by sent time: the earlier-sent message comes first, though it was posted second.
    assert_eq!(read[0].text, "first, by sent time");
    assert_eq!(read[1].text, "second, by sent time");
    assert_eq!(read[0].sent_time, Some(earlier_ts));
    assert_eq!(read[1].sent_time, Some(later_ts));
    assert!(read[0].sent_time < read[1].sent_time);
    // The sender is attributed by its registered handle (reverse-resolved from the signed address).
    assert_eq!(read[0].sender_handle, "sf-sender");
    assert!(read.iter().all(|m| m.delivered_time.is_some()));

    // The decrypted messages are on the receiver's local store.
    let stored = receiver.get_messages().expect("messages");
    assert!(
        stored.iter().any(|m| m.text == "first, by sent time")
            && stored.iter().any(|m| m.text == "second, by sent time")
    );

    // No re-read: the delivered messages are gone from the sidechain, so a second poll is empty.
    let again = receiver.poll_mailbox().expect("second poll");
    assert!(
        again.is_empty(),
        "delivered messages are dropped from the sidechain and not re-read"
    );
}

/// A delivery that always finds the peer unreachable. The exit-flush test never makes a direct
/// attempt (the sender is not ready), so this only guards against one.
struct UnreachablePeer;

impl bingle_local::api::pending_sender::MessageDelivery for UnreachablePeer {
    fn deliver(
        &self,
        _recipient: &str,
        _message: serde_json::Value,
        _progress: Option<std::sync::Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
        Err(bingle_core::api::bingle_api::BingleError::Send {
            kind: bingle_core::api::bingle_api::SendFailureKind::PeerUnreachable,
            detail: "peer offline".to_string(),
        })
    }
}

/// Issue #282: a message queued just before `chat` exits, with the peer offline, is handed to the
/// peer's Mailbox by the exit flush (`PendingSender::shutdown`) and read on the peer's next start.
#[test]
fn exit_flush_posts_a_queued_message_to_the_offline_peers_mailbox() {
    use bingle_local::api::pending_sender::{
        LocalOutboundStore, PendingSender, PendingSenderOptions,
    };
    use std::sync::Arc;

    let cluster = Cluster::up("exit-sender", "exit-receiver");
    let (app_id, asset_id) = (cluster.app_id, cluster.asset_id);
    let accounts = &cluster.accounts;

    // ── sender: queue a message, then exit before any direct attempt ─────────────────────────────
    let mut sender = BingleApiLocalImpl::new(local_config(app_id, asset_id, true, false));
    sender
        .import_keypair(accounts.sender.mnemonic.clone())
        .expect("import sender keypair");
    let ts = 1_726_700_000_000;
    sender
        .add_message(
            "exit-sender".to_string(),
            vec!["exit-receiver".to_string()],
            ts,
            "queued just before exit".to_string(),
            None,
        )
        .expect("add message");
    sender
        .update_message_status(&sender.id_of_timestamp_for_tests(ts), 0.0, None, None)
        .expect("mark pending");

    let pending_sender = PendingSender::start(
        Arc::new(LocalOutboundStore::new(sender.clone(), None)),
        Arc::new(UnreachablePeer),
        PendingSenderOptions::new(),
        Arc::new(|| false),
        Arc::new(|_| {}),
    );
    let report = pending_sender.shutdown(std::time::Duration::from_secs(60));
    assert_eq!(
        report.forwarded.len(),
        1,
        "the queued message is handed off: {report:?}"
    );
    assert!(
        report.not_sent.is_empty(),
        "nothing left behind: {report:?}"
    );
    assert!(
        sender.get_pending_messages().expect("pending").is_empty(),
        "no longer pending once in the Mailbox"
    );

    // ── receiver: next start reads it from the Mailbox ───────────────────────────────────────────
    let mut receiver = BingleApiLocalImpl::new(local_config(app_id, asset_id, false, true));
    receiver
        .import_keypair(accounts.receiver.mnemonic.clone())
        .expect("import receiver keypair");
    let read = receiver.poll_mailbox().expect("poll");
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].text, "queued just before exit");
    assert_eq!(read[0].sender_handle, "exit-sender");
}
