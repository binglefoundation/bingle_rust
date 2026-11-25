//! Unit tests for the Sidewinder Mailbox wrapper (store-and-forward epic #200, story #213).
//!
//! These exercise the parts that need no live node: the config defaults and the app-id rule, and
//! the transaction the two Mailbox operations build. The append-then-remove-head round-trip against a
//! running node lives in the separate, skip-clean `sidewinder_mailbox_e2e` target.
//!
//! The failover tests (#497) post through a real Mailbox to loopback mutual-TLS nodes
//! ([`crate::sidewinder_tls_node`]): one node's dead IPv4 address next to its live `[::1]` address is
//! handled inside that node's client, and only a node none of whose addresses answer is failed over.

use crate::sidewinder_tls_node::{TlsSidewinderNode, dead_addr, identity_of, node_record};
use algo_ops::AlgoOps;
use bingle_core::api::bingle_api::BingleError;
use bingle_local::api::sidewinder::{
    IDENTITY_REFUSED_BACKOFF, MAILBOX_POP_TYPE, MAILBOX_POST_TYPE, MAILBOX_TAKE_BATCH,
    MAILBOX_TAKE_TYPE, Mailbox, MailboxConfig, WatchProgress, build_pop_request,
    build_post_request, build_take_request, classify_watch, decode_take_batch,
    identity_refusal_holds, identity_refusal_message, is_routine_read, map_error, next_node_index,
};
use ed25519_dalek::SigningKey;
use sidewinder_ops::nat64::Nat64Resolver;
use sidewinder_ops::{
    Disposition, PendingTransaction, SidewinderError, Stage, SuggestedParams, TxnError,
};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Suggested params with recognisable values, so a built request can be asserted against them.
fn sample_params() -> SuggestedParams {
    SuggestedParams {
        instance_id: vec![7u8; 32],
        last_round: 100,
        min_fee: 1000,
        max_validity_window: 1000,
    }
}

#[test]
fn mailbox_config_discovered_carries_app_id_and_defaults() {
    let config = MailboxConfig::discovered(1234);
    assert_eq!(config.app_id, 1234);
    assert_eq!(config.post_type, MAILBOX_POST_TYPE);
    assert_eq!(config.pop_type, MAILBOX_POP_TYPE);
    assert_eq!(config.post_type, 1, "tier-1 Mailbox binds post to type 1");
    assert_eq!(config.pop_type, 2, "tier-1 Mailbox binds pop to type 2");
    assert_eq!(config.take_type, MAILBOX_TAKE_TYPE);
    assert_eq!(config.take_type, 4, "Sidewinder #626 binds take to type 4");
}

// --- Configuration from the Bingle app id only (#246) ---

#[test]
fn from_app_id_configures_the_mailbox_only_from_a_non_zero_app_id() {
    assert_eq!(
        MailboxConfig::from_app_id(Some(555)),
        Some(MailboxConfig::discovered(555)),
        "an app id -> discovery + mTLS from that app"
    );
    assert_eq!(
        MailboxConfig::from_app_id(None),
        None,
        "no app id leaves store-and-forward unconfigured"
    );
    assert_eq!(
        MailboxConfig::from_app_id(Some(0)),
        None,
        "a 0 app id is treated as unset"
    );
}

#[test]
fn the_removed_bearer_env_vars_have_no_effect() {
    // A stale environment from the removed bearer path (#246) must not change the transport or
    // configure a Mailbox on its own. Nothing else in this test binary reads these variables.
    // SAFETY: the variables are set and removed only here, and no other thread reads them.
    unsafe {
        std::env::set_var("SIDEWINDER_NODE_URL", "http://localhost:9101");
        std::env::set_var("SIDEWINDER_TOKEN", "stale-token");
    }
    let with_app_id = MailboxConfig::from_app_id(Some(77));
    let without_app_id = MailboxConfig::from_app_id(None);
    unsafe {
        std::env::remove_var("SIDEWINDER_NODE_URL");
        std::env::remove_var("SIDEWINDER_TOKEN");
    }
    assert_eq!(
        with_app_id,
        Some(MailboxConfig::discovered(77)),
        "the app id alone decides the connection"
    );
    assert_eq!(
        without_app_id, None,
        "the old env vars do not configure a Mailbox without an app id"
    );
}

// --- Log level of a submitted transaction (issues #402, #482) ---

#[test]
fn reads_are_routine_and_posts_are_not() {
    // pop and take run on every receive poll, so they log at trace; a post is a message sent.
    assert!(is_routine_read("pop"));
    assert!(is_routine_read("take"));
    assert!(!is_routine_read("post"));
}

// --- Failover cursor (story #244, deliverable 3) ---

#[test]
fn next_node_index_advances_then_exhausts() {
    // Three reachable nodes: 0 -> 1 -> 2 -> exhausted.
    assert_eq!(next_node_index(0, 3), Some(1));
    assert_eq!(next_node_index(1, 3), Some(2));
    assert_eq!(
        next_node_index(2, 3),
        None,
        "last node -> exhausted (triggers re-resolve)"
    );
    // A single node has no next.
    assert_eq!(next_node_index(0, 1), None);
    // An empty set never advances.
    assert_eq!(next_node_index(0, 0), None);
}

// --- Identity-matches-signer (story #244): one enrolled account serves both roles ---

#[test]
fn mtls_identity_is_the_same_account_as_the_transaction_signer() {
    // The Mailbox uses one AlgoOps handle for everything: it signs Mailbox transactions with the
    // account key AND (on the discovered transport) derives its mutual-TLS client identity from the
    // same account seed (sidewinder_ops::connect -> identity_key). So the on-chain identity a node
    // pins for this client is exactly the address that signs its transactions. `identity_key` is
    // pub(crate) in sidewinder_ops, so we assert the invariant through the public account surface:
    // the seed-derived identity address equals the signer's own address.
    let (id, mnemonic) = AlgoOps::generate_keypair();
    let algo = AlgoOps::new_for_algorand(Some(mnemonic.clone()), None, None);
    let signer_address = algo.address_str().expect("signer address");
    let identity_address =
        AlgoOps::address_from_passphrase(&mnemonic).expect("identity address from seed");
    assert_eq!(
        signer_address, id,
        "the transaction signer is the generated account"
    );
    assert_eq!(
        identity_address, signer_address,
        "the mutual-TLS identity (seed-derived) and the transaction signer are the same account"
    );
}

#[test]
fn build_post_request_packs_recipient_key_and_message() {
    let params = sample_params();
    let recipient = "BINGLERECIPIENTADDRESS";
    let message = b"sealed-envelope-bytes";
    let request = build_post_request(MAILBOX_POST_TYPE, recipient, message, &params);

    assert_eq!(request.txn_type, MAILBOX_POST_TYPE);
    assert_eq!(
        request.args.len(),
        2,
        "post carries the key and the message"
    );
    assert_eq!(
        request.args[0].to_bytes(),
        recipient.as_bytes(),
        "arg[0] is the recipient address string bytes (the queue key)"
    );
    assert_eq!(
        request.args[1].to_bytes(),
        message,
        "arg[1] is the message payload"
    );
    // Header comes straight from the suggested params.
    assert_eq!(request.max_fee, params.min_fee);
    assert_eq!(request.first_valid, params.last_round);
    assert_eq!(
        request.last_valid,
        params.last_round + params.max_validity_window
    );
    assert_eq!(request.instance, params.instance_id);
    assert!(
        request.note.is_some(),
        "a unique note keeps repeated posts from colliding on the content address"
    );
    assert!(request.group.is_none());
}

#[test]
fn build_pop_request_takes_no_args() {
    let params = sample_params();
    let request = build_pop_request(MAILBOX_POP_TYPE, &params);

    assert_eq!(request.txn_type, MAILBOX_POP_TYPE);
    assert!(
        request.args.is_empty(),
        "pop keys off the authenticated sender, so it carries no arguments"
    );
    assert_eq!(request.max_fee, params.min_fee);
    assert_eq!(request.first_valid, params.last_round);
    assert_eq!(
        request.last_valid,
        params.last_round + params.max_validity_window
    );
    assert_eq!(request.instance, params.instance_id);
    assert!(request.note.is_some());
}

// --- take: a batch per transaction (Sidewinder #626) ---

#[test]
fn build_take_request_carries_the_batch_size_as_a_uint64() {
    let params = sample_params();
    let request = build_take_request(MAILBOX_TAKE_TYPE, MAILBOX_TAKE_BATCH, &params);

    assert_eq!(request.txn_type, MAILBOX_TAKE_TYPE);
    assert_eq!(
        request.args,
        vec![algo_ops::AppArg::Uint(MAILBOX_TAKE_BATCH)],
        "take(max_entries) keys off the sender, so its one argument is the batch size"
    );
    assert_eq!(request.first_valid, params.last_round);
    assert_eq!(
        request.last_valid,
        params.last_round + params.max_validity_window
    );
    assert!(request.note.is_some());
}

#[test]
fn decode_take_batch_reads_each_message_in_order_with_the_more_flag() {
    // more = 1, then "hi" and an empty message, each behind a 4-byte big-endian length.
    let result = [1, 0, 0, 0, 2, b'h', b'i', 0, 0, 0, 0];
    assert_eq!(
        decode_take_batch(&result),
        Some((true, vec![b"hi".to_vec(), Vec::new()]))
    );
    assert_eq!(
        decode_take_batch(&[0, 0, 0, 0, 1, b'x']),
        Some((false, vec![b"x".to_vec()]))
    );
}

#[test]
fn decode_take_batch_reads_an_empty_result_as_an_empty_mailbox() {
    assert_eq!(decode_take_batch(&[]), Some((false, Vec::new())));
}

#[test]
fn decode_take_batch_rejects_a_malformed_result() {
    assert_eq!(decode_take_batch(&[2]), None, "more is 0 or 1");
    assert_eq!(decode_take_batch(&[0, 0, 0]), None, "truncated length");
    assert_eq!(
        decode_take_batch(&[0, 0, 0, 0, 5, b'a']),
        None,
        "message runs past the end"
    );
}

#[test]
fn successive_builds_get_distinct_notes() {
    let params = sample_params();
    let a = build_pop_request(MAILBOX_POP_TYPE, &params);
    let b = build_pop_request(MAILBOX_POP_TYPE, &params);
    assert_ne!(
        a.note, b.note,
        "each built request gets a unique note so repeated pops have distinct content addresses"
    );
}

/// A `watch` read at `stage`, with an error body carrying `reason` when given.
fn pending_at(stage: Stage, reason: Option<&str>) -> PendingTransaction {
    PendingTransaction {
        tx_id: "TX".to_string(),
        stage,
        result: None,
        logs: Vec::new(),
        events: Vec::new(),
        certificate: None,
        proof: None,
        error: reason.map(|message| TxnError {
            message: message.to_string(),
            disposition: Some(Disposition::Rejected),
        }),
    }
}

#[test]
fn classify_watch_keeps_polling_while_in_flight() {
    assert_eq!(
        classify_watch(&pending_at(Stage::Pending, None)),
        WatchProgress::InFlight
    );
    assert_eq!(
        classify_watch(&pending_at(Stage::Provisional, None)),
        WatchProgress::InFlight
    );
}

#[test]
fn classify_watch_final_with_and_without_error() {
    assert_eq!(
        classify_watch(&pending_at(Stage::Final, None)),
        WatchProgress::Final
    );
    assert_eq!(
        classify_watch(&pending_at(Stage::Final, Some("op error"))),
        WatchProgress::FinalWithError("op error".to_string())
    );
}

#[test]
fn classify_watch_refuses_terminal_stages_with_the_node_reason() {
    // #305: rejected / expired / failed are terminal refusals carrying the node's reason.
    let reason = "sender is not enrolled or its parent-chain account is not funded";
    for stage in [Stage::Rejected, Stage::Expired, Stage::Failed] {
        assert_eq!(
            classify_watch(&pending_at(stage, Some(reason))),
            WatchProgress::Refused {
                stage,
                reason: reason.to_string()
            },
            "{stage:?} is a terminal refusal"
        );
    }
}

#[test]
fn classify_watch_refusal_without_a_reason_has_a_placeholder() {
    assert_eq!(
        classify_watch(&pending_at(Stage::Rejected, None)),
        WatchProgress::Refused {
            stage: Stage::Rejected,
            reason: "no reason given".to_string()
        }
    );
}

/// The message sidewinder_ops 0.8.7 gives an identity refusal: it names the node and the address.
const REFUSAL: &str = "the node at https://node.example:1080 refused this client's identity \
    TESTADDRESS: it is not a permitted client there.";

#[test]
fn identity_refusal_message_is_some_only_for_identity_refused() {
    // #309: the node was reached and refused this identity — not a network fault.
    let refused: anyhow::Error = SidewinderError::identity_refused("params", REFUSAL).into();
    let message = identity_refusal_message(&refused).expect("an identity refusal");
    assert!(message.contains("refused this client's identity"));
    assert!(
        message.contains("TESTADDRESS"),
        "the message names the address"
    );

    let unreachable: anyhow::Error =
        SidewinderError::unreachable("params", "connection refused").into();
    assert!(identity_refusal_message(&unreachable).is_none());
    assert!(identity_refusal_message(&anyhow::anyhow!("something else")).is_none());
}

#[test]
fn map_error_does_not_retry_an_identity_refusal() {
    let refused: anyhow::Error = SidewinderError::identity_refused("params", REFUSAL).into();
    let mapped = map_error("params", refused);
    assert!(
        !matches!(mapped, BingleError::Retryable(_)),
        "an identity refusal is not retryable: {mapped}"
    );
    assert!(
        mapped.to_string().contains("TESTADDRESS"),
        "the surfaced error names the address"
    );
}

#[test]
fn map_error_still_retries_an_unreachable_or_transient_node() {
    let unreachable: anyhow::Error =
        SidewinderError::unreachable("params", "connection refused").into();
    assert!(matches!(
        map_error("params", unreachable),
        BingleError::Retryable(_)
    ));
    let transient: anyhow::Error = SidewinderError::transient("params", "timed out").into();
    assert!(matches!(
        map_error("params", transient),
        BingleError::Retryable(_)
    ));
}

#[test]
fn map_error_retries_a_family_or_route_the_network_lacks() {
    // #497: an IPv4 address dialled on an IPv6-only network (or the reverse) fails with one of these;
    // sidewinder_ops reports it as unreachable, so the Mailbox fails over rather than giving up.
    for cause in [
        "tcp connect error: Network is unreachable (os error 51)",
        "tcp connect error: No route to host (os error 65)",
        "tcp connect error: Can't assign requested address: address not available",
        "tcp connect error: Address family not supported by protocol family (os error 47)",
    ] {
        assert!(
            SidewinderError::looks_unreachable(cause),
            "sidewinder_ops classifies {cause:?} as unreachable"
        );
        let error: anyhow::Error = SidewinderError::unreachable("params", cause).into();
        assert!(
            matches!(map_error("params", error), BingleError::Retryable(_)),
            "{cause:?} is retryable"
        );
    }
}

#[test]
fn identity_refusal_holds_for_the_backoff_window_only() {
    let refused_at = Instant::now();
    assert!(!identity_refusal_holds(None, refused_at), "no refusal seen");
    assert!(identity_refusal_holds(Some(refused_at), refused_at));
    assert!(identity_refusal_holds(
        Some(refused_at),
        refused_at + IDENTITY_REFUSED_BACKOFF - Duration::from_secs(1)
    ));
    assert!(
        !identity_refusal_holds(Some(refused_at), refused_at + IDENTITY_REFUSED_BACKOFF),
        "the hold-off ends after the back-off window"
    );
}

// --- Every address of a node before the next node (#497) ---

/// A NAT64 resolver that reports no NAT64, so the failover tests never query the network's DNS.
struct NoNat64;

impl Nat64Resolver for NoNat64 {
    fn lookup_ipv4only(&self) -> std::io::Result<Vec<IpAddr>> {
        Ok(Vec::new())
    }
}

/// A fresh client account and its Algorand address (also its mutual-TLS identity).
fn client_account() -> (AlgoOps, String) {
    let (address, mnemonic) = AlgoOps::generate_keypair();
    (
        AlgoOps::new_for_algorand(Some(mnemonic), None, None),
        address,
    )
}

/// A Mailbox over the given resolved nodes, with a short finality timeout (the mock finalises at once).
fn mailbox_over(algo: AlgoOps, nodes: Vec<sidewinder_ops::DiscoveredNode>) -> Mailbox {
    let mut config = MailboxConfig::discovered(1234);
    config.finality_timeout = Duration::from_secs(10);
    Mailbox::with_nodes(algo, config, nodes, Arc::new(NoNat64)).expect("connect the first node")
}

#[test]
fn a_dead_ipv4_and_live_ipv6_endpoint_delivers_without_cross_node_failover() {
    let (algo, client) = client_account();
    let dual =
        TlsSidewinderNode::start_v6(&SigningKey::from_bytes(&[210u8; 32]), vec![client.clone()]);
    let spare = TlsSidewinderNode::start_v4(&SigningKey::from_bytes(&[211u8; 32]), vec![client]);
    let nodes = vec![
        node_record(
            dual.identity(),
            Some(dead_addr("127.0.0.1")),
            Some(dual.addr()),
        ),
        node_record(spare.identity(), Some(spare.addr()), None),
    ];

    let mut mailbox = mailbox_over(algo, nodes);
    mailbox
        .post("RECIPIENT", b"sealed")
        .expect("the live IPv6 address of the first node takes the post");

    assert_eq!(dual.submits(), 1, "the post landed on the dual-stack node");
    assert_eq!(
        spare.connections(),
        0,
        "the dead IPv4 address is handled inside the node's client, not by failing over"
    );
}

#[test]
fn a_node_with_only_a_dead_ipv4_endpoint_fails_over_to_the_next_node() {
    let (algo, client) = client_account();
    let next = TlsSidewinderNode::start_v4(&SigningKey::from_bytes(&[212u8; 32]), vec![client]);
    let nodes = vec![
        node_record(
            &identity_of(&SigningKey::from_bytes(&[213u8; 32])),
            Some(dead_addr("127.0.0.1")),
            None,
        ),
        node_record(next.identity(), Some(next.addr()), None),
    ];

    let mut mailbox = mailbox_over(algo, nodes);
    mailbox
        .post("RECIPIENT", b"sealed")
        .expect("the next node takes the post once the first is unreachable");

    assert_eq!(next.submits(), 1, "the post failed over to the next node");
}

#[test]
fn an_identity_refusal_is_final_and_does_not_fail_over() {
    let (algo, client) = client_account();
    // the first node is reachable on IPv6 but does not permit this client.
    let refusing = TlsSidewinderNode::start_v6(&SigningKey::from_bytes(&[214u8; 32]), Vec::new());
    let spare = TlsSidewinderNode::start_v4(&SigningKey::from_bytes(&[215u8; 32]), vec![client]);
    let nodes = vec![
        node_record(
            refusing.identity(),
            Some(dead_addr("127.0.0.1")),
            Some(refusing.addr()),
        ),
        node_record(spare.identity(), Some(spare.addr()), None),
    ];

    let mut mailbox = mailbox_over(algo, nodes);
    let error = mailbox
        .post("RECIPIENT", b"sealed")
        .expect_err("a refused identity fails the post");

    assert!(
        !matches!(error, BingleError::Retryable(_)),
        "an identity refusal is not retryable: {error}"
    );
    assert!(
        mailbox.identity_refusal().is_some(),
        "the refusal is reported for the hold-off"
    );
    assert_eq!(spare.connections(), 0, "a refusal does not fail over");
}
