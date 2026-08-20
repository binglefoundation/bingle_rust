use bingle_core::blockchain::algo_ops::AlgoChainConfig;
use serial_test::serial;

use crate::setup_localnet;
use crate::util::test_util;

use test_util::{ADDRESS_SPEND, PASSPHRASE_SPEND, localnet_config, ops_from_mnemonic};

// This test validates that register ensures the caller is opted-in to the app local state.
// It uses localnet and will be skipped when localnet is unavailable.
#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn register_ensures_sender_opted_in_to_app() {
    test_util::assert_localnet_available();

    let cfg: AlgoChainConfig = localnet_config();
    // Ensure our test account is funded
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[ADDRESS_SPEND]).expect("ensure funded");

    // Creator ops
    let creator = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());

    // Deploy minimal app + asset for register flow using common helper
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE$", 10_000);

    let ab =
        bingle_core::blockchain::algo_bingle::AlgoBingle::new(creator.clone(), app_id, asset_id);

    // 4) Ensure test sender local state is cleared w.r.t app (if previously opted in) by closing it.
    // It's okay if clear/close fails due to not opted in; we ignore errors to keep test resilient.
    let _ = creator.clear_state_app(app_id);
    let _ = creator.close_out_app(app_id);

    // Buy 1 unit to cover the registration fee (also opts sender into the ASA)
    ab.buy_bingle(app_id, asset_id, 1)
        .expect("buy Bingle$ for registration fee");

    // 5) Now call register which should internally ensure opt-in to app before app call
    let txid = ab
        .register(app_id, asset_id, "handle_optin_test", 1)
        .expect("register call succeeds after implicit opt-in");
    assert!(!txid.is_empty(), "tx id should be non-empty");

    // 6) Verify local state exists for the account under this app
    let _ = creator
        .local_state_for_account(app_id, ADDRESS_SPEND)
        .expect("local state query call")
        .expect("local state should exist after register");
}
