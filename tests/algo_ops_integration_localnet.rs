use rust_comms::algo_ops::{AlgoOps, AlgoProviderConfig};

#[path = "setup_localnet.rs"]
mod setup_localnet;
#[path = "test_util.rs"]
mod test_util;
use test_util::{localnet_config, ops_from_mnemonic, ADDRESS_10MIL, ADDRESS_SPEND, PASSPHRASE_SPEND, ADDRESS_RECEIVE, PASSPHRASE_RECEIVE};




fn fund_test_accounts_or_panic() {
    let cfg = test_util::localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[test_util::ADDRESS_10MIL, test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE])
        .expect("Failed to ensure localnet test accounts funded; install algokit and start localnet");
}

// How to run these integration tests from RustRover:
// - Ensure Algokit localnet (or another local Algorand node) is running at http://localhost:4001
//   with token aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa; or
// - Set the environment variable RUST_COMMS_RUN_LOCALNET=true in your Run Configuration.
// The tests will auto-skip if localnet isn’t available and the env var isn’t set.
fn should_run_localnet() -> bool { test_util::should_run_localnet() }

#[test]
fn account_balance_for_address10mil_returns_some() {
    if !should_run_localnet() {
        eprintln!("SKIP: localnet not available (set RUST_COMMS_RUN_LOCALNET=true to force)");
        return;
    }
    fund_test_accounts_or_panic();
    let cfg = localnet_config();
    let ops = AlgoOps::new(None, Some(ADDRESS_10MIL.to_string()), Some(cfg));
    let bal = ops.account_balance().expect("network query should not error on localnet");
    assert!(bal.is_some(), "Expected Some(balance) for funded localnet account");
    assert!(bal.unwrap() >= 0.0);
}

#[test]
fn global_state_for_address10mil_returns_some_vec() {
    if !should_run_localnet() {
        eprintln!("SKIP: localnet not available (set RUST_COMMS_RUN_LOCALNET=true to force)");
        return;
    }
    fund_test_accounts_or_panic();
    let cfg = localnet_config();
    let ops = AlgoOps::new(None, Some(ADDRESS_10MIL.to_string()), Some(cfg));
    let gs = ops.global_state(None).expect("global_state call should succeed on localnet");
    assert!(gs.is_some(), "Should return Some (possibly empty) global state vector");
}

#[test]
fn account_balance_for_spend_and_receive_addresses_returns_some() {
    if !should_run_localnet() {
        eprintln!("SKIP: localnet not available (set RUST_COMMS_RUN_LOCALNET=true to force)");
        return;
    }
    fund_test_accounts_or_panic();
    let cfg = localnet_config();
    let ops_spend = AlgoOps::new(None, Some(ADDRESS_SPEND.to_string()), Some(cfg.clone()));
    let ops_recv = AlgoOps::new(None, Some(ADDRESS_RECEIVE.to_string()), Some(cfg.clone()));

    let b1 = ops_spend.account_balance().expect("balance query");
    let b2 = ops_recv.account_balance().expect("balance query");
    assert!(b1.is_some());
    assert!(b2.is_some());
}

#[test]
fn send_algo_transfers_and_updates_balances() {
    if !should_run_localnet() {
        eprintln!("SKIP: localnet not available (set RUST_COMMS_RUN_LOCALNET=true to force)");
        return;
    }
    fund_test_accounts_or_panic();
    let cfg = localnet_config();
    let sender = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());
    let receiver = AlgoOps::new(None, Some(ADDRESS_RECEIVE.to_string()), Some(cfg.clone()));

    let sb_before = sender.account_balance().expect("sender bal query").expect("some sender bal");
    let rb_before = receiver.account_balance().expect("receiver bal query").expect("some receiver bal");

    let amount = 0.1234f64;
    sender.send_algo(ADDRESS_RECEIVE, amount).expect("send algo");

    // After send_algo returns, it has waited for confirmation
    let sb_after = sender.account_balance().expect("sender bal query after").expect("some sender bal");
    let rb_after = receiver.account_balance().expect("receiver bal query after").expect("some receiver bal");

    let delta_s = sb_before - sb_after;
    let delta_r = rb_after - rb_before;

    // Receiver should increase by ~amount within tolerance
    assert!(delta_r > 0.0, "receiver should increase");
    assert!((delta_r - amount).abs() < 0.005, "receiver delta {} should be close to amount {}", delta_r, amount);

    // Sender should decrease by at least amount; fee typically ~0.001 ALGO on localnet
    assert!(delta_s >= amount, "sender delta {} should be >= amount {}", delta_s, amount);
    assert!(delta_s - amount < 0.02, "fee seems unexpectedly large: {}", delta_s - amount);
}


fn get_asset_holding_amount(cfg: &AlgoProviderConfig, addr_str: &str, asset_id: u64) -> Option<u64> {
    use std::str::FromStr;
    let url = format!("{}:{}", cfg.client_api_url, cfg.client_api_port);
    let token = cfg.token.clone().unwrap_or_default();
    let client = algonaut::algod::v2::Algod::new(&url, &token).ok()?;
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().ok()?;
    let addr = algonaut::core::Address::from_str(addr_str).ok()?;
    let info = match rt.block_on(client.account_information(&addr)) { Ok(i) => i, _ => return None };
    let v = serde_json::to_value(&info).ok()?;
    let arr = v.get("assets").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    for a in arr {
        let id = a.get("asset-id").or_else(|| a.get("asset_id")).and_then(|x| x.as_u64());
        if id == Some(asset_id) {
            if let Some(amt) = a.get("amount").and_then(|x| x.as_u64()) { return Some(amt); }
        }
    }
    None
}

#[test]
fn asset_create_optin_and_transfer_updates_holdings() {
    if !should_run_localnet() {
        eprintln!("SKIP: localnet not available (set RUST_COMMS_RUN_LOCALNET=true to force)");
        return;
    }
    fund_test_accounts_or_panic();
    let cfg = localnet_config();

    let creator = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());
    let receiver = ops_from_mnemonic(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, cfg.clone());

    let total = 1000u64;
    let asset_id = creator.create_asset("TCOIN", total)
        .expect("asset create call")
        .expect("created asset id");

    // Receiver opts in
    receiver.opt_in_to_asset(asset_id).expect("opt in asset");

    // Transfer 10 units to receiver
    let send_amt = 10u64;
    creator.send_asset(asset_id, send_amt, ADDRESS_RECEIVE).expect("send asset");

    // Verify holdings via algod account info
    let c_amt = get_asset_holding_amount(&cfg, ADDRESS_SPEND, asset_id).expect("creator holding query");
    let r_amt = get_asset_holding_amount(&cfg, ADDRESS_RECEIVE, asset_id).expect("receiver holding query");

    assert_eq!(c_amt, total - send_amt, "creator holdings should decrease by send amount");
    assert_eq!(r_amt, send_amt, "receiver holdings should equal sent amount");
}
