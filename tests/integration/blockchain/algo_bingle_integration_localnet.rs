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

#[cfg_attr(not(target_os = "ios"), test)]
#[ignore]
#[serial]
pub fn bingle_end_to_end_calls() {
    skip_if_no_localnet!();
    fund_test_accounts_or_panic();
    let cfg: AlgoChainConfig = localnet_config();

    // Creator/sender and receiver accounts
    let creator = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());
    let receiver = ops_from_mnemonic(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, cfg.clone());

    // Deploy the dapp and create the Bingle$ ASA
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE", 1_000_000);

    // Stock the app with some units to sell
    let app_addr = creator.contract_address(app_id).expect("app address");
    creator.send_asset(asset_id, 100, &app_addr).expect("fund app with ASA");

    // Receiver opts in to ASA and receives 10 units
    receiver.opt_in_to_asset(asset_id).expect("receiver opt-in ASA");
    creator.send_asset(asset_id, 10, ADDRESS_RECEIVE).expect("transfer ASA to receiver");

    // Receiver opts in to app to allow local state updates
    // receiver.opt_in_app(app_id).expect("receiver opt-in app");

    // Wrap receiver in AlgoBingle
    let ab = AlgoBingle::new(receiver.clone(), app_id, asset_id);

    // buy_bingle: pay 1 microAlgo to app and do self->self ASA xfer of 1 to satisfy checks
    ab.buy_bingle(app_id, asset_id, 1).expect("buy_bingle group call");

    // sell_bingle: send 2 units to app and self-pay 2 microAlgos to satisfy payout
    ab.sell_bingle(app_id, asset_id, 2, 1).expect("sell_bingle group call");

    // register: pay 1 unit of ASA to app and set handle
    let handle = "alice";
    ab.register(app_id, asset_id, handle, 1).expect("register group call");

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
