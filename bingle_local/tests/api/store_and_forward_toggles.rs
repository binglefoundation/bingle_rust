//! Tests for the store-and-forward send / receive config toggles (epic #200, story #212).
//!
//! The toggles gate the post-on-fail (#214) and read-on-reconnect (#215) paths, which do not exist
//! yet; what is testable now is that both gates default off, that the builder the JSI/webserver call
//! sites use maps them correctly, that a config without the fields still works (backward
//! compatibility), and that the resolved value is observable on the implementation for #214/#215.

use algo_ops::AlgoChainConfig;
use bingle_local::api::{BingleApiLocalImpl, LocalApiConfig};

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
