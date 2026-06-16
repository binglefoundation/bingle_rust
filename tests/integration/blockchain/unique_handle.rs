use rust_comms::blockchain::algo_bingle::AlgoBingle;
use rust_comms::algo_ops::AlgoChainConfig;
use serial_test::serial;

use crate::setup_localnet;
use crate::util::test_util;

use test_util::{localnet_config, ops_from_mnemonic, ADDRESS_SPEND, PASSPHRASE_SPEND, ADDRESS_RECEIVE, PASSPHRASE_RECEIVE};

fn fund_test_accounts_or_panic() {
    let cfg = test_util::localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE])
        .expect("Failed to ensure localnet test accounts funded; install algokit and start localnet");
}

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn test_unique_handle_registration() {
    test_util::init_test_logging();
    test_util::assert_localnet_available();
    fund_test_accounts_or_panic();
    let cfg: AlgoChainConfig = localnet_config();

    // Account A (First user)
    let ops_a = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());
    // Account B (Second user)
    let ops_b = ops_from_mnemonic(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, cfg.clone());

    // 1. Deploy app and asset using Account A
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&ops_a, "BINGLE", 1_000_000);
    tracing::info!("Deployed app_id={}, asset_id={}", app_id, asset_id);

    // Give some assets to Account B so it can attempt registration (fee is 1 unit)
    ops_b.opt_in_to_asset(asset_id).expect("Account B opt-in ASA");
    ops_a.send_asset(asset_id, 10, ADDRESS_RECEIVE).expect("fund Account B with ASA");

    let ab_a = AlgoBingle::new(ops_a.clone(), app_id, asset_id);
    let ab_b = AlgoBingle::new(ops_b.clone(), app_id, asset_id);

    let handle = "unique_handle_test";

    // 2. Register handle with Account A
    tracing::info!("Registering handle '{}' with Account A ({})", handle, ADDRESS_SPEND);
    ab_a.register(app_id, asset_id, handle, 1).expect("Account A should be able to register handle");

    // Wait a moment for indexer to catch up
    std::thread::sleep(std::time::Duration::from_secs(2));

    // 3. Attempt to register same handle with Account B
    tracing::info!("Attempting to register handle '{}' with Account B ({}) - SHOULD FAIL", handle, ADDRESS_RECEIVE);
    let result_b = ab_b.register(app_id, asset_id, handle, 1);
    
    assert!(result_b.is_err(), "Account B should NOT be able to register the same handle");
    let err_msg = result_b.err().unwrap().to_string();
    tracing::info!("Registration failed as expected: {}", err_msg);
    assert!(err_msg.contains("Handle already in use"), "Error message should indicate handle is in use, got: {}", err_msg);

    // 4. Verify handle lookup finds Account A
    tracing::info!("Looking up handle '{}'", handle);
    let owner = ab_a.handle_lookup(handle).expect("lookup should succeed").expect("handle should be found");
    assert_eq!(owner, ADDRESS_SPEND, "Handle lookup should return Account A's address");
    tracing::info!("Handle lookup verified owner: {}", owner);
}
