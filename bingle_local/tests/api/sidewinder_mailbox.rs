//! Unit tests for the Sidewinder Mailbox wrapper (store-and-forward epic #200, story #213).
//!
//! These exercise the parts that need no live node: the config defaults and `from_parts` mapping,
//! the clean-error behaviour when the endpoint or token is missing, and the transaction the two
//! Mailbox operations build. The append-then-remove-head round-trip against a running node lives in
//! the separate, skip-clean `sidewinder_mailbox_e2e` target.

use algo_ops::AlgoOps;
use bingle_local::api::sidewinder::{
    MAILBOX_POP_TYPE, MAILBOX_POST_TYPE, Mailbox, MailboxConfig, MailboxConnection,
    build_pop_request, build_post_request, next_node_index,
};
use sidewinder_ops::SuggestedParams;

/// A keyless handle: `Mailbox::new` validates the config before it ever signs, so no key is needed
/// to test the validation and construction path.
fn keyless_algo() -> AlgoOps {
    AlgoOps::new_for_algorand(None, None, None)
}

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
fn mailbox_config_new_uses_default_operation_types() {
    let config = MailboxConfig::new("http://localhost:9101", "tok");
    assert_eq!(config.post_type, MAILBOX_POST_TYPE);
    assert_eq!(config.pop_type, MAILBOX_POP_TYPE);
    assert_eq!(config.post_type, 1, "tier-1 Mailbox binds post to type 1");
    assert_eq!(config.pop_type, 2, "tier-1 Mailbox binds pop to type 2");
    assert!(
        matches!(config.connection, MailboxConnection::Bearer { .. }),
        "new() builds a bearer connection"
    );
}

#[test]
fn mailbox_config_discovered_carries_app_id_and_defaults() {
    let config = MailboxConfig::discovered(1234);
    assert_eq!(config.post_type, MAILBOX_POST_TYPE);
    assert_eq!(config.pop_type, MAILBOX_POP_TYPE);
    assert_eq!(
        config.connection,
        MailboxConnection::Discovered { app_id: 1234 }
    );
}

// --- Connection selection (story #244, deliverable 2): url+token override vs app-id discovery ---

#[test]
fn select_prefers_bearer_override_even_when_app_id_present() {
    // The Bingle app id is always present in normal operation, so an explicit URL+token must win.
    let conn =
        MailboxConnection::select(Some("http://n:9101".into()), Some("tok".into()), Some(555));
    assert_eq!(
        conn,
        Some(MailboxConnection::Bearer {
            base_url: "http://n:9101".into(),
            token: "tok".into(),
        }),
        "url + token override discovery"
    );
}

#[test]
fn select_uses_discovery_when_only_app_id_available() {
    assert_eq!(
        MailboxConnection::select(None, None, Some(555)),
        Some(MailboxConnection::Discovered { app_id: 555 }),
        "no override + app id -> discovery + mTLS"
    );
    // A half-set bearer (url or token alone) is not an override, so it falls back to discovery.
    assert_eq!(
        MailboxConnection::select(Some("http://n:9101".into()), None, Some(555)),
        Some(MailboxConnection::Discovered { app_id: 555 }),
        "url alone is not a bearer override; discovery is used"
    );
    assert_eq!(
        MailboxConnection::select(None, Some("tok".into()), Some(555)),
        Some(MailboxConnection::Discovered { app_id: 555 }),
        "token alone is not a bearer override; discovery is used"
    );
}

#[test]
fn select_is_none_when_neither_override_nor_app_id() {
    assert_eq!(
        MailboxConnection::select(None, None, None),
        None,
        "no override and no app id leaves store-and-forward unconfigured"
    );
    assert_eq!(
        MailboxConnection::select(None, None, Some(0)),
        None,
        "a 0 app id is treated as unset"
    );
}

#[test]
fn select_treats_blank_override_values_as_absent() {
    // A blank URL/token must not half-configure the bearer path; with an app id, discovery is used.
    assert_eq!(
        MailboxConnection::select(Some("   ".into()), Some("tok".into()), Some(9)),
        Some(MailboxConnection::Discovered { app_id: 9 }),
    );
    assert_eq!(
        MailboxConnection::select(Some("http://n:9101".into()), Some("".into()), Some(9)),
        Some(MailboxConnection::Discovered { app_id: 9 }),
    );
    // Blank override and no app id -> unconfigured.
    assert_eq!(
        MailboxConnection::select(Some("   ".into()), Some("   ".into()), None),
        None,
    );
}

#[test]
fn mailbox_config_select_wraps_the_selected_connection() {
    let bearer = MailboxConfig::select(Some("http://n:9101".into()), Some("tok".into()), Some(1));
    assert!(matches!(
        bearer.map(|c| c.connection),
        Some(MailboxConnection::Bearer { .. })
    ));
    let discovered = MailboxConfig::select(None, None, Some(77));
    assert_eq!(
        discovered.map(|c| c.connection),
        Some(MailboxConnection::Discovered { app_id: 77 })
    );
    assert!(MailboxConfig::select(None, None, None).is_none());
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
    // account key AND (on the discovery transport) derives its mutual-TLS client identity from the
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
fn from_parts_requires_both_url_and_token() {
    assert!(
        MailboxConfig::from_parts(Some("http://n:9101".into()), Some("tok".into())).is_some(),
        "both present configures the mailbox"
    );
    assert!(
        MailboxConfig::from_parts(Some("http://n:9101".into()), None).is_none(),
        "url without token is unconfigured"
    );
    assert!(
        MailboxConfig::from_parts(None, Some("tok".into())).is_none(),
        "token without url is unconfigured"
    );
    assert!(
        MailboxConfig::from_parts(None, None).is_none(),
        "neither is unconfigured"
    );
}

#[test]
fn from_parts_treats_blank_values_as_absent() {
    assert!(
        MailboxConfig::from_parts(Some("   ".into()), Some("tok".into())).is_none(),
        "a blank url does not half-configure the mailbox"
    );
    assert!(
        MailboxConfig::from_parts(Some("http://n:9101".into()), Some("".into())).is_none(),
        "a blank token does not half-configure the mailbox"
    );
}

#[test]
fn mailbox_new_rejects_empty_endpoint() {
    // Mailbox is not Debug (it holds a client), so match rather than unwrap the Result.
    match Mailbox::new(keyless_algo(), MailboxConfig::new("", "tok")) {
        Ok(_) => panic!("empty endpoint must be rejected"),
        Err(err) => assert!(
            err.to_string().to_lowercase().contains("url"),
            "error names the missing endpoint: {err}"
        ),
    }
}

#[test]
fn mailbox_new_rejects_empty_token() {
    match Mailbox::new(
        keyless_algo(),
        MailboxConfig::new("http://localhost:9101", ""),
    ) {
        Ok(_) => panic!("empty token must be rejected"),
        Err(err) => assert!(
            err.to_string().to_lowercase().contains("token"),
            "error names the missing token: {err}"
        ),
    }
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
