//! Tests for the store-and-forward send / receive config toggles (epic #200, story #212).
//!
//! The toggles gate the post-on-fail (#214) and read-on-reconnect (#215) paths, which do not exist
//! yet; what is testable now is that both gates default off, that the builder the JSI/webserver call
//! sites use maps them correctly, that a config without the fields still works (backward
//! compatibility), and that the resolved value is observable on the implementation for #214/#215.

use algo_ops::AlgoChainConfig;
use bingle_local::api::{BingleApiLocalImpl, LocalApiConfig, MailboxConfig};

#[test]
fn both_gates_default_off() {
    let cfg = LocalApiConfig::default();
    assert!(!cfg.store_and_forward_send, "send gate defaults off");
    assert!(!cfg.store_and_forward_receive, "receive gate defaults off");
}

#[test]
fn with_notify_leaves_gates_off_for_backward_compatibility() {
    // A call site (or persisted config) that predates the toggles still builds a valid, gates-off
    // config — nothing opts into store-and-forward implicitly.
    let cfg = LocalApiConfig::with_notify(AlgoChainConfig::default(), 0, 0, None, None);
    assert!(!cfg.store_and_forward_send);
    assert!(!cfg.store_and_forward_receive);
}

#[test]
fn with_store_and_forward_maps_each_side_independently() {
    let base = || LocalApiConfig::with_notify(AlgoChainConfig::default(), 0, 0, None, None);

    // Send on, receive off.
    let send_only = base().with_store_and_forward(Some(true), Some(false));
    assert!(send_only.store_and_forward_send);
    assert!(!send_only.store_and_forward_receive);

    // Receive on, send off — the reverse a single flag could not express.
    let receive_only = base().with_store_and_forward(Some(false), Some(true));
    assert!(!receive_only.store_and_forward_send);
    assert!(receive_only.store_and_forward_receive);

    // Both on.
    let both = base().with_store_and_forward(Some(true), Some(true));
    assert!(both.store_and_forward_send);
    assert!(both.store_and_forward_receive);
}

#[test]
fn with_store_and_forward_none_defaults_off() {
    let cfg = LocalApiConfig::with_notify(AlgoChainConfig::default(), 0, 0, None, None)
        .with_store_and_forward(None, None);
    assert!(!cfg.store_and_forward_send, "None send defaults off");
    assert!(!cfg.store_and_forward_receive, "None receive defaults off");
}

// --- Startup validation: gate on but no Mailbox configured fails loudly (story #244) ---

#[test]
fn validate_passes_when_gates_off_regardless_of_mailbox() {
    // Gates off (today's default) never requires a Mailbox.
    let cfg = LocalApiConfig::with_notify(AlgoChainConfig::default(), 0, 0, None, None);
    assert!(cfg.validate_store_and_forward().is_ok());
}

#[test]
fn validate_fails_when_a_gate_is_on_but_no_mailbox_configured() {
    for (send, receive) in [(true, false), (false, true), (true, true)] {
        let cfg = LocalApiConfig::with_notify(AlgoChainConfig::default(), 0, 0, None, None)
            .with_store_and_forward(Some(send), Some(receive));
        // No .with_sidewinder(...): the Mailbox is unconfigured.
        let err = cfg
            .validate_store_and_forward()
            .expect_err("a gate on with no Mailbox must fail loudly");
        assert!(
            err.contains("SIDEWINDER_NODE_URL")
                && err.contains("SIDEWINDER_TOKEN")
                && err.contains("app id"),
            "error names both the app-id and the bearer inputs; got: {err}"
        );
    }
}

#[test]
fn validate_passes_when_a_gate_is_on_and_a_mailbox_is_configured() {
    // Either a bearer override or the discovery (app-id) connection satisfies the check.
    let bearer = LocalApiConfig::with_notify(AlgoChainConfig::default(), 0, 0, None, None)
        .with_sidewinder(Some(MailboxConfig::new("http://n:9101", "tok")))
        .with_store_and_forward(Some(true), Some(true));
    assert!(bearer.validate_store_and_forward().is_ok());

    let discovered = LocalApiConfig::with_notify(AlgoChainConfig::default(), 42, 0, None, None)
        .with_sidewinder(Some(MailboxConfig::discovered(42)))
        .with_store_and_forward(Some(false), Some(true));
    assert!(discovered.validate_store_and_forward().is_ok());
}

#[test]
fn gates_are_observable_on_the_implementation() {
    // The value each side's path (#214 / #215) reads is surfaced from the built implementation.
    let cfg = LocalApiConfig::with_notify(AlgoChainConfig::default(), 0, 0, None, None)
        .with_store_and_forward(Some(true), Some(false));
    let api = BingleApiLocalImpl::new(cfg);
    assert!(api.store_and_forward_send(), "send gate observable and on");
    assert!(
        !api.store_and_forward_receive(),
        "receive gate observable and off"
    );
}
