use rust_comms::engine::BingleAccessUnsafeForTests;
use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

// Ensure tests for src/api are under tests/api per project guidelines.

#[cfg_attr(not(target_os = "ios"), test)]
pub fn start_returns_err_on_invalid_passphrase() {
    // Passphrase that is not in the expected format (missing b64: prefix)
    let bad_pass = "this-is-not-a-valid-secret".to_string();

    // Provide a static endpoint so Engine would choose static path if we got that far; we expect early Err instead.
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

    let api = BingleApiImpl::new(&StartOptions::default());
    let opts = StartOptions {
        handle: "tester".into(),
        algo_passphrase: Some(bad_pass),
        static_ip: Some(addr),
        am_relay: false,
        stun_servers: None,
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None, handle_cache_expiry: None, dangerous_debug: true,
    };

    let err = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts)).expect_err("start should fail for invalid passphrase");
    // Check that the error message includes context from private_key_bytes failure mapping
    assert!(err.contains("Failed to get private key bytes"), "Unexpected error: {err}");
}
