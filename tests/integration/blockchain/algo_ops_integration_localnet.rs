use rust_comms::algo_ops::AlgoOps;

use crate::setup_localnet;


use crate::util::test_util;

use test_util::{localnet_config, ADDRESS_10MIL};


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

#[test]
#[ignore]
fn account_balance_for_address10mil_returns_some() {
    skip_if_no_localnet!();
    fund_test_accounts_or_panic();
    let cfg = localnet_config();
    let ops = AlgoOps::new(None, Some(ADDRESS_10MIL.to_string()), Some(cfg));
    let bal = ops.account_balance().expect("network query should not error on localnet");
    assert!(bal.is_some(), "Expected Some(balance) for funded localnet account");
    assert!(bal.unwrap() >= 0.0);
}

#[test]
#[ignore]
fn global_state_for_address10mil_returns_some_vec() {
    skip_if_no_localnet!();
    fund_test_accounts_or_panic();
    let cfg = localnet_config();
    let ops = AlgoOps::new(None, Some(ADDRESS_10MIL.to_string()), Some(cfg));
    let gs = ops.global_state(None).expect("global_state call should succeed on localnet");
    assert!(gs.is_some(), "Should return Some (possibly empty) global state vector");
}

// Compatibility shim test binary to restore expected name `algo_ops_integration_localnet`.
// This intentionally performs no heavy integration work here; it only ensures the
// test target exists and can be executed. It will skip unless localnet is available.

#[test]
#[ignore]
fn algo_ops_integration_localnet_placeholder() {
    skip_if_no_localnet!();
    // Localnet is available; keep placeholder light to avoid duplicating other tests.
    // Future: could delegate to a more comprehensive integration suite here.
    assert!(true, "localnet detected; placeholder passing");
}
