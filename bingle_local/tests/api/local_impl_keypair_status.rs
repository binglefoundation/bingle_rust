use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{
    BingleApiLocalImpl, LocalApiConfig, keypair_status_from_facts,
};

#[test]
fn test_keypair_status_none_when_no_keypair() {
    let api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let status = api.keypair_status().expect("keypair_status should succeed");
    assert_eq!(status.status, "None");
    assert!(status.id.is_none());
    assert!(status.handle.is_none());
    assert!(status.required_algo.is_none());
}

#[test]
fn test_keypair_status_has_id_after_generate() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let kp = api
        .generate_keypair()
        .expect("generate_keypair should succeed");

    // With default config (no real blockchain), get_algo_ops will succeed but
    // balance/asset checks will fail. keypair_status should still return a status
    // with the algorand id set. The exact status depends on blockchain availability;
    // since we have no real node, we expect an error or UNFUNDED.
    let result = api.keypair_status();
    match result {
        Ok(status) => {
            // Should have the id set
            let id = status.id.expect("id should be present when keypair exists");
            assert_eq!(id, kp.id);
            // Status should not be "None" since keypair exists
            assert_ne!(status.status, "None");
        }
        Err(_) => {
            // Blockchain query failure is acceptable in unit tests without a real node
        }
    }
}

#[test]
fn test_status_active_when_asset_and_handle() {
    let status = keypair_status_from_facts(
        "ID_ACTIVE".to_string(),
        true,
        Some("alice".to_string()),
        // balance is irrelevant when ACTIVE
        0.0,
    );
    assert_eq!(status.status, "ACTIVE");
    assert_eq!(status.id.as_deref(), Some("ID_ACTIVE"));
    assert_eq!(status.handle.as_deref(), Some("alice"));
    assert!(status.required_algo.is_none());
}

#[test]
fn test_status_falls_through_to_balance_when_asset_but_no_handle() {
    // Holds the Bingle$ asset but has no Handle entry: must not be ACTIVE.
    // With a funded balance we expect FUNDED, not ACTIVE.
    let funded = keypair_status_from_facts("ID_NO_HANDLE".to_string(), true, None, 2.0);
    assert_eq!(funded.status, "FUNDED");
    assert_eq!(funded.id.as_deref(), Some("ID_NO_HANDLE"));
    assert!(funded.handle.is_none());
    assert!(funded.required_algo.is_none());

    // With an insufficient balance we expect UNFUNDED.
    let unfunded = keypair_status_from_facts("ID_NO_HANDLE".to_string(), true, None, 0.0);
    assert_eq!(unfunded.status, "UNFUNDED");
    assert_eq!(unfunded.id.as_deref(), Some("ID_NO_HANDLE"));
    assert!(unfunded.handle.is_none());
    assert!(unfunded.required_algo.is_some());
}

#[test]
fn test_status_unfunded_when_no_asset_and_low_balance() {
    let status = keypair_status_from_facts("ID_POOR".to_string(), false, None, 0.5);
    assert_eq!(status.status, "UNFUNDED");
    assert!(status.handle.is_none());
    assert!(status.required_algo.is_some());
}
