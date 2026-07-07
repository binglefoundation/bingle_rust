/// Integration tests for sender authentication using a real localnet blockchain.
///
/// These tests exercise `BingleApiImpl::handle_lookup_by_id` against a deployed
/// Bingle smart contract on algokit localnet.  The unit counterpart
/// (`tests/engine/sender_auth.rs`) uses mock APIs; here we drive the actual
/// blockchain code path so that any change to the on-chain local-state schema or
/// the blockchain query logic is caught by a realistic test.
///
/// Run with:
///   cargo test --test integration sender_auth_localnet
///
/// Prerequisites: `algokit localnet start` must be running.
use bingle_core::api::bingle_api::StartOptions;
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use serial_test::serial;

use crate::setup_localnet;
use crate::util::test_util;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Build a minimal `BingleApiImpl` that is configured to query the given
/// localnet `app_id` but does **not** start any background threads.
/// This is sufficient for `handle_lookup_by_id` which only needs
/// `app_id` and `algo_provider_config` from `StartOptions`.
fn make_read_only_api(passphrase: &str, app_id: u64) -> std::sync::Arc<BingleApiImpl> {
    let opts = StartOptions {
        handle: "test".into(),
        algo_passphrase: Some(passphrase.to_string()),
        static_ip: None,
        am_relay: false,
        stun_servers: None,
        algo_provider_config: Some(test_util::localnet_config()),
        algo_network: None,
        app_id: Some(app_id),
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: false,
        log_mode: bingle_core::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    };
    BingleApiImpl::new(&opts)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// A user who has registered on-chain should have their handle returned by
/// `handle_lookup_by_id`.
#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn registered_user_handle_retrieved_from_blockchain() {
    test_util::init_test_logging();
    test_util::assert_localnet_available();

    let cfg = test_util::localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(
        &cfg,
        &[test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE],
    )
    .expect("fund test accounts");

    let creator = test_util::ops_from_mnemonic(
        test_util::ADDRESS_SPEND,
        test_util::PASSPHRASE_SPEND,
        cfg.clone(),
    );
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE$", 1_000_000);

    // Register a user on the blockchain with a known handle.
    let handle = "alice_auth_test";
    test_util::register_client_on_blockchain(
        test_util::ADDRESS_RECEIVE,
        test_util::PASSPHRASE_RECEIVE,
        handle,
        app_id,
        asset_id,
        &creator,
        cfg.clone(),
    );

    // Build a read-only API pointed at the deployed app and query the blockchain.
    let api = make_read_only_api(test_util::PASSPHRASE_SPEND, app_id);
    let user_id = test_util::ADDRESS_RECEIVE.to_string();
    let result = api.handle_lookup_by_id(&user_id);

    assert_eq!(
        result,
        Some(handle.to_string()),
        "handle_lookup_by_id should return '{}' for a registered user, got {:?}",
        handle,
        result
    );
}

/// A user who has never registered on-chain should cause `handle_lookup_by_id`
/// to return `None` rather than panic or error.
#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn unregistered_user_handle_not_found_on_blockchain() {
    test_util::init_test_logging();
    test_util::assert_localnet_available();

    let cfg = test_util::localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[test_util::ADDRESS_SPEND])
        .expect("fund test accounts");

    let creator = test_util::ops_from_mnemonic(
        test_util::ADDRESS_SPEND,
        test_util::PASSPHRASE_SPEND,
        cfg.clone(),
    );
    // Deploy a fresh app so no accounts have registered against it.
    let (app_id, _asset_id) =
        test_util::deploy_bingle_app_and_asset(&creator, "BINGLE$", 1_000_000);

    let api = make_read_only_api(test_util::PASSPHRASE_SPEND, app_id);

    // ADDRESS_RECEIVE has never opted in or registered against this fresh app.
    let user_id = test_util::ADDRESS_RECEIVE.to_string();
    let result = api.handle_lookup_by_id(&user_id);

    assert_eq!(
        result, None,
        "handle_lookup_by_id should return None for an unregistered user, got {:?}",
        result
    );
}
