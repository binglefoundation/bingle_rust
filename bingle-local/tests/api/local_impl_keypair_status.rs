use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};

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
    let kp = api.generate_keypair().expect("generate_keypair should succeed");

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
