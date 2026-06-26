use rust_comms::blockchain::algo_bingle::AlgoBingle;
use rust_comms::algo_ops::AlgoChainConfig;
use serial_test::serial;

const MIN_FEE: u64 = 1_000;

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
pub fn bingle_buy_register() {
    test_util::assert_localnet_available();
    fund_test_accounts_or_panic();
    let cfg: AlgoChainConfig = localnet_config();

    // Creator/sender and receiver accounts
    let creator = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());
    let receiver = ops_from_mnemonic(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, cfg.clone());

    // Deploy the dapp and create the Bingle$ ASA (app is seeded with 10 units via initial_hot_bingle)
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE", 1_000_000);

    let app_addr = creator.contract_address(app_id).expect("app address");

    // Wrap receiver in AlgoBingle
    let ab = AlgoBingle::new(receiver.clone(), app_id, asset_id);

    let app_asa_before   = creator.asset_holding(&app_addr, asset_id).expect("app asa before");
    let app_algo_before  = creator.microalgos_at(&app_addr).expect("app algo before");

    // buy_bingle: receiver pays 1 µAlgo to app; app sends 1 Bingle$ to receiver
    ab.buy_bingle(app_id, asset_id, 1).expect("buy_bingle group call");

    let receiver_asa_after_buy = creator.asset_holding(ADDRESS_RECEIVE, asset_id).expect("receiver asa after buy");
    let app_asa_after_buy      = creator.asset_holding(&app_addr, asset_id).expect("app asa after buy");
    let app_algo_after_buy     = creator.microalgos_at(&app_addr).expect("app algo after buy");

    assert_eq!(receiver_asa_after_buy, 1, "receiver should hold 1 Bingle$ after buy");
    assert_eq!(app_asa_after_buy, app_asa_before - 1, "app should have 1 fewer Bingle$ after buy");
    assert_eq!(app_algo_after_buy, app_algo_before + 1 - MIN_FEE, "app should net -999 µAlgo after buy (received 1, paid 1000 inner fee)");

    // register: receiver pays 1 Bingle$ to app and sets handle
    let handle = "alice";
    ab.register(app_id, asset_id, handle, 1).expect("register group call");

    let receiver_asa_after_reg = creator.asset_holding(ADDRESS_RECEIVE, asset_id).expect("receiver asa after register");
    let app_asa_after_reg      = creator.asset_holding(&app_addr, asset_id).expect("app asa after register");

    assert_eq!(receiver_asa_after_reg, 0, "receiver should have 0 Bingle$ after paying registration fee");
    assert_eq!(app_asa_after_reg, app_asa_after_buy + 1, "app should have 1 more Bingle$ after register");

    // Verify local state contains the handle
    let lstate = creator.local_state_for_account(app_id, ADDRESS_RECEIVE).expect("local state query").expect("some local state");
    let mut found = false;
    for (k, v) in lstate {
        if k == "Handle" {
            assert_eq!(v, handle, "handle should be stored in local state");
            found = true;
        }
    }
    assert!(found, "expected Handle key in local state");
}
