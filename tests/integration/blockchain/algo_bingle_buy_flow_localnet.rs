use rust_comms::blockchain::algo_bingle::AlgoBingle;
use rust_comms::algo_ops::AlgoChainConfig;
use serial_test::serial;

use crate::setup_localnet;
use crate::util::test_util;

use test_util::{localnet_config, ops_from_mnemonic, ADDRESS_SPEND, PASSPHRASE_SPEND, ADDRESS_RECEIVE, PASSPHRASE_RECEIVE};

#[test]
#[ignore]
#[serial]
fn buy_bingle_transfers_from_reserve_inner_tx() {
    skip_if_no_localnet!();

    // Ensure accounts are funded
    let cfg: AlgoChainConfig = localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[ADDRESS_SPEND, ADDRESS_RECEIVE])
        .expect("Failed to ensure localnet test accounts funded; install algokit and start localnet");

    // Creator (holds reserve) and buyer accounts
    let creator = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());
    let buyer = ops_from_mnemonic(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, cfg.clone());

    // Deploy app and ASA for the buy flow test using common helper
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE", 1_000_000);

    // Stock the app account with supply to sell
    let app_addr = creator.contract_address(app_id).expect("app address");
    creator.send_asset(asset_id, 100, &app_addr).expect("fund app with ASA");

    // Buyer: ensure opted-in to ASA (required to receive inner tx)
    buyer.opt_in_to_asset(asset_id).expect("buyer opt-in to ASA");

    // Assert buyer holds 0 before purchase
    let zero_before = buyer.is_account_opted_in_to_asset(ADDRESS_RECEIVE, asset_id).expect("check opt-in");
    assert!(zero_before, "buyer should be opted-in to ASA before buying");

    // To check holdings, read account info through helper that lists assets
    // We'll send 0 and ensure no transfer has happened yet (remains zero units)
    // There is no direct balance getter; do a simple send of 0 (already done by opt-in) and rely on contract behavior

    // Execute buy_bingle: pay 1 microAlgo; app should transfer 1 unit from reserve to buyer via inner tx
    let ab_buyer = AlgoBingle::new(buyer.clone(), app_id, asset_id);
    ab_buyer.buy_bingle(app_id, asset_id, 1).expect("buy_bingle call");

    // Validate buyer now holds the asset: query again and ensure opted-in still, and creator can transfer back 1 unit to verify balance > 0
    // Attempt to send 1 unit back to creator; this should succeed if buyer holds >=1
    creator.send_asset(asset_id, 0, ADDRESS_RECEIVE).ok(); // no-op ensure ledger updated
    // Now transfer back 1 unit from buyer to creator; if buyer didn't receive, this will fail
    let res = buyer.send_asset(asset_id, 1, ADDRESS_SPEND);
    assert!(res.is_ok(), "buyer should be able to send 1 unit after buy_bingle; got {:?}", res.err());
}
