use algo_ops::AlgoChainConfig;
use bingle_core::blockchain::algo_bingle::AlgoBingle;
use serial_test::serial;

use crate::setup_localnet;
use crate::util::test_util;

use test_util::{
    ADDRESS_RECEIVE, ADDRESS_SPEND, PASSPHRASE_RECEIVE, PASSPHRASE_SPEND, localnet_config,
    ops_from_mnemonic,
};

/// End-to-end check of `AlgoBingle::sell_bingle` on localnet. `sell_bingle` builds an atomic group
/// — an asset transfer of `amount` units (seller -> app) plus a self-payment of `price * amount`
/// that the contract validates — via the `algo_ops` transaction-group builder (issue #191). The
/// seller first acquires a unit through `buy_bingle`, then sells it back; the ASA holding must drop
/// to zero.
#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn sell_bingle_returns_units_to_app() {
    test_util::assert_localnet_available();

    // Ensure accounts are funded
    let cfg: AlgoChainConfig = localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[ADDRESS_SPEND, ADDRESS_RECEIVE])
        .expect(
            "Failed to ensure localnet test accounts funded; install algokit and start localnet",
        );

    // Creator (holds reserve) and seller accounts
    let creator = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());
    let seller = ops_from_mnemonic(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, cfg.clone());

    // Deploy app and ASA (app is seeded with units; on-chain Bingle$ price is 1 µAlgo)
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE", 1_000_000);

    // Seller acquires 1 unit via buy_bingle (opt-in handled internally by the flow)
    let ab_seller = AlgoBingle::new(seller.clone(), app_id, asset_id);
    ab_seller
        .buy_bingle(app_id, asset_id, 1)
        .expect("buy_bingle call");
    let held = creator
        .asset_holding(ADDRESS_RECEIVE, asset_id)
        .expect("seller balance after buy");
    assert_eq!(held, 1, "seller should hold 1 unit before selling");

    // Sell the unit back: amount = 1 at the on-chain price of 1 µAlgo
    ab_seller
        .sell_bingle(app_id, asset_id, 1, 1)
        .expect("sell_bingle call");

    // Validate the seller now holds 0 units (the unit was transferred to the app)
    let after = creator
        .asset_holding(ADDRESS_RECEIVE, asset_id)
        .expect("seller balance after sell");
    assert_eq!(after, 0, "seller should hold 0 units after sell_bingle");
}
