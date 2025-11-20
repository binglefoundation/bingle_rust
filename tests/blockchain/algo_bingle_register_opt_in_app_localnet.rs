use rust_comms::algo_ops::AlgoProviderConfig;

#[path = "../setup_localnet.rs"]
mod setup_localnet;
#[path = "../test_util.rs"]
mod test_util;
use test_util::{localnet_config, ops_from_mnemonic, ADDRESS_SPEND, PASSPHRASE_SPEND, should_run_localnet};

// This test validates that register ensures the caller is opted-in to the app local state.
// It uses localnet and will be skipped when localnet is unavailable.
#[test]
fn register_ensures_sender_opted_in_to_app() {
    if !should_run_localnet() {
        eprintln!("SKIP: localnet not available (set RUST_COMMS_RUN_LOCALNET=true to force)");
        return;
    }

    let cfg: AlgoProviderConfig = localnet_config();
    // Ensure our test account is funded
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[ADDRESS_SPEND])
        .expect("ensure funded");

    // Creator ops
    let creator = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());

    // Deploy minimal app + asset for register flow using existing helper test harness
    // We reuse the integration test helper: create asset, deploy app, set price, etc.
    // For brevity, we inline a minimal path similar to other localnet tests.
    // 1) Compile and deploy contract with no asset foreign arrays yet
    let approval = include_bytes!("../../generated/BingleDapp.approval.teal");
    let clear = include_bytes!("../../generated/BingleDapp.clear.teal");
    let app_id = creator
        .deploy_app(approval, clear, None)
        .expect("deploy app")
        .expect("app id");

    // 2) Create ASA with reserve/clawback to app so register can transfer fees
    let asset_id = creator
        .create_asset_with_reserve_app("BINGLE$,REGAP", 10_000, app_id)
        .expect("create asset")
        .expect("asset id");

    // 3) Fund app with ASA so it can receive registration fees (opt-in app to asset via wrapper)
    let ab = rust_comms::blockchain::algo_bingle::AlgoBingle::new(creator.clone());
    let _ = ab.opt_in_app_to_asset(app_id, asset_id).expect("opt in app to asset");

    // 4) Ensure test sender local state is cleared w.r.t app (if previously opted in) by closing it.
    // It's okay if clear/close fails due to not opted in; we ignore errors to keep test resilient.
    let _ = creator.clear_state_app(app_id);
    let _ = creator.close_out_app(app_id);

    // 5) Now call register which should internally ensure opt-in to app before app call
    let txid = ab
        .register(app_id, asset_id, "handle_optin_test", 1)
        .expect("register call succeeds after implicit opt-in");
    assert!(!txid.is_empty(), "tx id should be non-empty");

    // 6) Verify local state exists for the account under this app
    let ls = creator
        .local_state_for_account(app_id, ADDRESS_SPEND)
        .expect("local state query call")
        .expect("local state should exist after register");
    assert!(ls.len() >= 0, "local state entries vector should be present");
}
