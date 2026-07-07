use bingle_core::algo_ops::AlgoChainConfig;
use bingle_core::blockchain::algo_bingle::AlgoBingle;
use serial_test::serial;

use crate::setup_localnet;
use crate::util::test_util;

use test_util::{ADDRESS_SPEND, PASSPHRASE_SPEND, localnet_config, ops_from_mnemonic};

fn fund_test_accounts_or_panic() {
    let cfg = test_util::localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[test_util::ADDRESS_SPEND])
        .expect("Failed to ensure localnet test accounts funded; install algokit and start localnet");
}

/// End-to-end partial (prefix) handle lookup against localnet.
///
/// Registers a mixed-case handle with punctuation, then verifies that a normalised
/// prefix resolves to the owner's id and the canonical handle exactly as written.
#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn test_handle_lookup_partial_returns_canonical() {
    test_util::init_test_logging();
    test_util::assert_localnet_available();
    fund_test_accounts_or_panic();
    let cfg: AlgoChainConfig = localnet_config();

    let ops_a = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());

    // Deploy app and asset using Account A.
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&ops_a, "BINGLE", 1_000_000);
    tracing::info!("Deployed app_id={}, asset_id={}", app_id, asset_id);

    let ab_a = AlgoBingle::new(ops_a.clone(), app_id, asset_id);

    // A needs 1 unit to pay the registration fee; buy from the app.
    ab_a.buy_bingle(app_id, asset_id, 1)
        .expect("Account A buy Bingle$ for registration fee");

    // Canonical handle as written: mixed case with punctuation.
    let canonical = "Partial_Lookup.Alice";
    ab_a.register(app_id, asset_id, canonical, 1)
        .expect("Account A should be able to register handle");

    // Wait for the indexer to catch up.
    std::thread::sleep(std::time::Duration::from_secs(2));

    // A normalised prefix ("partiallook") should match the start of "partiallookupalice".
    let hit = ab_a
        .handle_lookup_partial("partiallook")
        .expect("partial lookup should succeed")
        .expect("prefix should match a registered handle");

    assert_eq!(hit.0, ADDRESS_SPEND, "should resolve to Account A's address");
    assert_eq!(
        hit.1, canonical,
        "should return the canonical handle exactly as written on-chain"
    );

    // A prefix that does not start any handle returns None.
    let miss = ab_a
        .handle_lookup_partial("zzz_nomatch")
        .expect("partial lookup should succeed");
    assert!(miss.is_none(), "non-matching prefix should return None");
}
