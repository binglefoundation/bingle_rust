use bingle_core::blockchain::algo_bingle::AlgoBingle;
use bingle_core::blockchain::algo_ops::AlgoChainConfig;
use serial_test::serial;

use crate::setup_localnet;
use crate::util::test_util;

use test_util::{
    ADDRESS_RECEIVE, ADDRESS_SPEND, PASSPHRASE_RECEIVE, PASSPHRASE_SPEND, localnet_config,
    ops_from_mnemonic,
};

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn buy_bingle_transfers_from_reserve_inner_tx() {
    test_util::assert_localnet_available();

    // Ensure accounts are funded
    let cfg: AlgoChainConfig = localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[ADDRESS_SPEND, ADDRESS_RECEIVE])
        .expect(
            "Failed to ensure localnet test accounts funded; install algokit and start localnet",
        );

    // Creator (holds reserve) and buyer accounts
    let creator = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());
    let buyer = ops_from_mnemonic(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, cfg.clone());

    // Deploy app and ASA (app is seeded with 10 units via initial_hot_bingle)
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE", 1_000_000);

    // Assert buyer holds 0 before purchase
    let before = creator
        .asset_holding(ADDRESS_RECEIVE, asset_id)
        .expect("buyer balance before");
    assert_eq!(before, 0, "buyer should hold 0 units before buying");

    // Execute buy_bingle: buyer pays 1 µAlgo; app transfers 1 unit to buyer (opt-in handled internally)
    let ab_buyer = AlgoBingle::new(buyer.clone(), app_id, asset_id);
    ab_buyer
        .buy_bingle(app_id, asset_id, 1)
        .expect("buy_bingle call");

    // Validate buyer now holds 1 unit
    let after = creator
        .asset_holding(ADDRESS_RECEIVE, asset_id)
        .expect("buyer balance after");
    assert_eq!(after, 1, "buyer should hold 1 unit after buy_bingle");
}
