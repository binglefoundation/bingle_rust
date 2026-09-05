//! Unit tests for the Sidewinder Mailbox wrapper (store-and-forward epic #200, story #213).
//!
//! These exercise the parts that need no live node: the config defaults and `from_parts` mapping,
//! the clean-error behaviour when the endpoint or token is missing, and the transaction the two
//! Mailbox operations build. The append-then-remove-head round-trip against a running node lives in
//! the separate, skip-clean `sidewinder_mailbox_e2e` target.

use algo_ops::AlgoOps;
use bingle_local::api::sidewinder::{
    MAILBOX_POP_TYPE, MAILBOX_POST_TYPE, Mailbox, MailboxConfig, build_pop_request,
    build_post_request,
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
