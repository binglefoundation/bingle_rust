//! Unit tests for offline send resilience (issue #18).
//!
//! A1: queuing a message resolves the sender handle from cache, so it never needs a live
//! blockchain read once the account is registered.

use bingle_core::api::bingle_api::BingleError;
use bingle_core::blockchain::error::{AlgoError, AlgoErrorKind};
use bingle_local::api::KeypairStatus;
use bingle_local::api::bingle_local_api_impl::{resolve_sender_handle, status_or_last_known};

fn no_blockchain_err() -> BingleError {
    BingleError::Algo(AlgoError::unreachable(
        "account_information",
        "error sending request",
    ))
}

fn active_status(handle: &str) -> KeypairStatus {
    KeypairStatus {
        status: "ACTIVE".to_string(),
        id: Some("ID".to_string()),
        handle: Some(handle.to_string()),
        required_algo: None,
    }
}

#[test]
fn cached_handle_is_used_without_a_status_read() {
    // The cache is warm: the fetch closure must not be called at all (it would fail offline).
    let mut fetched = false;
    let handle = resolve_sender_handle(Some("alice".to_string()), || {
        fetched = true;
        Err(no_blockchain_err())
    })
    .expect("cached handle should resolve");
    assert_eq!(handle, "alice");
    assert!(!fetched, "keypair_status must not be consulted when the handle is cached");
}

#[test]
fn falls_back_to_status_when_cache_empty() {
    let handle = resolve_sender_handle(None, || Ok(active_status("bob")))
        .expect("should resolve from a fresh status");
    assert_eq!(handle, "bob");
}

#[test]
fn propagates_no_blockchain_on_first_run_with_empty_cache() {
    // Genuine first run, offline: nothing cached and the chain is unreachable -> surface it.
    let err = resolve_sender_handle(None, || Err(no_blockchain_err())).expect_err("should fail");
    match err {
        BingleError::Algo(ae) => assert_eq!(ae.kind, AlgoErrorKind::HostUnreachable),
        other => panic!("expected Algo/HostUnreachable, got {:?}", other),
    }
}

#[test]
fn errors_when_status_has_no_handle_and_cache_empty() {
    // Registered account but status carries no handle (not yet ACTIVE): a clear error.
    let unfunded = KeypairStatus {
        status: "UNFUNDED".to_string(),
        id: Some("ID".to_string()),
        handle: None,
        required_algo: Some(1.5),
    };
    let err = resolve_sender_handle(None, || Ok(unfunded)).expect_err("should fail");
    assert!(err.to_string().contains("No handle registered"));
}

// ── A2: keypair_status tolerates a transient outage ──────────────────────────────

#[test]
fn returns_last_known_status_when_unreachable_and_cached() {
    // Already-known account: a host-unreachable read falls back to the cached ACTIVE status
    // rather than surfacing NoBlockchain, keeping the running app usable.
    let out = status_or_last_known(no_blockchain_err(), Some(active_status("alice")))
        .expect("should fall back to cached");
    assert_eq!(out.status, "ACTIVE");
    assert_eq!(out.handle.as_deref(), Some("alice"));
}

#[test]
fn propagates_when_unreachable_but_nothing_cached() {
    // Genuine first run, offline: no cache -> surface the outage so the UI shows NoBlockchain.
    let err = status_or_last_known(no_blockchain_err(), None).expect_err("should propagate");
    match err {
        BingleError::Algo(ae) => assert_eq!(ae.kind, AlgoErrorKind::HostUnreachable),
        other => panic!("expected Algo/HostUnreachable, got {:?}", other),
    }
}

#[test]
fn non_outage_error_is_not_masked_by_cache() {
    // A real (non-network) error must propagate even if a cached status exists, so genuine
    // failures are not hidden.
    let err = status_or_last_known(
        BingleError::Other("bad config".to_string()),
        Some(active_status("alice")),
    )
    .expect_err("non-outage error should propagate");
    assert!(err.to_string().contains("bad config"));
}

#[test]
fn network_available_is_false_without_a_keypair() {
    use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig};
    // With no keypair there is nothing to send or register, so network_available reports false
    // (rather than erroring) and issues no blockchain probe (issue #31).
    let api = BingleApiLocalImpl::new(LocalApiConfig::default());
    assert!(!api.network_available(true).expect("should not error"));
}
