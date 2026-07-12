//! Unit tests for offline send resilience (issue #18).
//!
//! A1: queuing a message resolves the sender handle from cache, so it never needs a live
//! blockchain read once the account is registered.

use bingle_core::api::bingle_api::BingleError;
use bingle_core::blockchain::error::{AlgoError, AlgoErrorKind};
use bingle_local::api::KeypairStatus;
use bingle_local::api::bingle_local_api_impl::resolve_sender_handle;

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
