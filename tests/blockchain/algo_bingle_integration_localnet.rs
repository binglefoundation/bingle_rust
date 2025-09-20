use rust_comms::blockchain::algo_bingle::AlgoBingle;
use rust_comms::algo_ops::{AlgoProviderConfig, AppArg};

#[path = "../setup_localnet.rs"]
mod setup_localnet;
#[path = "../test_util.rs"]
mod test_util;
use test_util::{localnet_config, ops_from_mnemonic, ADDRESS_SPEND, PASSPHRASE_SPEND, ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, should_run_localnet};

use std::fs;

fn fund_test_accounts_or_panic() {
    let cfg = test_util::localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE])
        .expect("Failed to ensure localnet test accounts funded; install algokit and start localnet");
}

#[test]
fn bingle_end_to_end_calls() {
    if !should_run_localnet() {
        eprintln!("SKIP: localnet not available (set RUST_COMMS_RUN_LOCALNET=true to force)");
        return;
    }
    fund_test_accounts_or_panic();
    let cfg: AlgoProviderConfig = localnet_config();

    // Creator/sender and receiver accounts
    let creator = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());
    let receiver = ops_from_mnemonic(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, cfg.clone());

    // Create an ASA to act as Bingle$
    let total_units = 1_000_000u64;
    let asset_id = creator.create_asset("BINGLE", total_units).expect("asset create").expect("asset id");

    // Deploy the dapp from TEAL artifacts
    // Print current working directory to help diagnose path issues
    match std::env::current_dir() {
        Ok(cwd) => eprintln!("Current working directory: {}", cwd.display()),
        Err(e) => eprintln!("Failed to get current working directory: {}", e),
    }
    let approval_src = fs::read_to_string("dapp/smart_contracts/artifacts/bingle_dapp/BingleDapp.approval.teal").expect("read approval teal");
    let clear_src = fs::read_to_string("dapp/smart_contracts/artifacts/bingle_dapp/BingleDapp.clear.teal").expect("read clear teal");
    let approval_bytes = creator.compile_teal(&approval_src).expect("compile approval teal");
    let clear_bytes = creator.compile_teal(&clear_src).expect("compile clear teal");
    // Pass asset_id to deploy so the app account is auto opted-in to the ASA
    let app_id = creator.deploy_app(&approval_bytes, &clear_bytes, Some(asset_id)).expect("deploy app call").expect("app id");

    // Set price = 1 (microAlgo and unit) using creator (must be app creator)
    let _ = creator.call_app(app_id, ADDRESS_SPEND, None, Some("set_bingle_price(uint64)void"), &[AppArg::Uint(1)]).expect("set_bingle_price call");

    // Receiver opts in to ASA and receives 10 units
    receiver.opt_in_to_asset(asset_id).expect("receiver opt-in ASA");
    creator.send_asset(asset_id, 10, ADDRESS_RECEIVE).expect("transfer ASA to receiver");

    // Receiver opts in to app to allow local state updates
    receiver.opt_in_app(app_id).expect("receiver opt-in app");

    // Wrap receiver in AlgoBingle
    let ab = AlgoBingle::new(receiver.clone());

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
