use rust_comms::blockchain::algo_bingle::AlgoBingle;
use rust_comms::algo_ops::{AlgoProviderConfig};

#[path = "../setup_localnet.rs"]
mod setup_localnet;
#[path = "../test_util.rs"]
mod test_util;
use test_util::{localnet_config, ops_from_mnemonic, ADDRESS_SPEND, PASSPHRASE_SPEND, ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, should_run_localnet};

use std::fs;
use std::time::{Duration, Instant};
use std::thread;

#[test]
fn set_allow_and_register_endpoint_then_list_and_clear() {
    if !should_run_localnet() {
        eprintln!("SKIP: localnet not available (set RUST_COMMS_RUN_LOCALNET=true to force)");
        return;
    }
    // Ensure test accounts are funded
    let cfg: AlgoProviderConfig = localnet_config();

    // Extra guard: ensure Indexer is reachable; otherwise skip to avoid false negatives when only algod is up.
    let health_url = format!("{}:{}/health", cfg.indexer_api_url, cfg.indexer_api_port);
    match reqwest::blocking::get(&health_url) {
        Ok(resp) if resp.status().is_success() => { /* ok */ }
        _ => {
            eprintln!("SKIP: indexer not reachable at {}", health_url);
            return;
        }
    }

    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[ADDRESS_SPEND, ADDRESS_RECEIVE])
        .expect("Failed to ensure localnet test accounts funded; install algokit and start localnet");

    // Creator and user ops
    let creator = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());
    let user = ops_from_mnemonic(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, cfg.clone());

    // Deploy the dapp from TEAL artifacts
    match std::env::current_dir() {
        Ok(cwd) => eprintln!("Current working directory: {}", cwd.display()),
        Err(e) => eprintln!("Failed to get current working directory: {}", e),
    }
    let approval_src = fs::read_to_string("dapp/projects/dapp/smart_contracts/artifacts/bingle_dapp/BingleDapp.approval.teal").expect("read approval teal");
    let clear_src = fs::read_to_string("dapp/projects/dapp/smart_contracts/artifacts/bingle_dapp/BingleDapp.clear.teal").expect("read clear teal");
    let approval_bytes = creator.compile_teal(&approval_src).expect("compile approval teal");
    let clear_bytes = creator.compile_teal(&clear_src).expect("compile clear teal");
    let app_id = creator.deploy_app(&approval_bytes, &clear_bytes, None).expect("deploy app call").expect("app id");

    // User opts in to the app to enable local state
    user.opt_in_app(app_id).expect("user opt-in app");

    // Grant static permission for the user via the app creator, then user registers a static endpoint
    let ab_creator = AlgoBingle::new(creator.clone());
    let _ = ab_creator.set_allow_static(app_id, ADDRESS_RECEIVE, true).expect("set_allow_static call");
    let ab_user = AlgoBingle::new(user.clone());
    let endpoint = "127.0.0.1:54321";
    let _ = ab_user.register_endpoint(app_id, endpoint).expect("register_endpoint call");

    // Query via Indexer and validate our account appears with the endpoint.
    // Indexer is eventually consistent; poll for up to ~10 seconds.
    let start = Instant::now();
    let mut list: Vec<(String, String)>;
    let ab = AlgoBingle::new(user.clone());
    loop {
        list = ab.list_static_endpoints_via_indexer(app_id).expect("indexer list");
        if list.iter().any(|(addr, ep)| addr == ADDRESS_RECEIVE && ep == endpoint) { break; }
        if start.elapsed() > Duration::from_secs(45) { break; }
        thread::sleep(Duration::from_millis(250));
    }
    let mut found = false;
    for (addr, ep) in &list {
        if addr == ADDRESS_RECEIVE {
            assert_eq!(ep, endpoint, "endpoint mismatch for user");
            found = true;
        }
    }
    assert!(found, "user ADDRESS_RECEIVE not found in static_endpoint indexer list: {:?}", list);

    // Clear the endpoint and verify removal (also with polling to account for indexer lag)
    let _ = ab_user.register_endpoint(app_id, "").expect("clear endpoint call");
    let start2 = Instant::now();
    let mut list2;
    loop {
        list2 = ab.list_static_endpoints_via_indexer(app_id).expect("indexer list after clear");
        if !list2.iter().any(|(addr, _)| addr == ADDRESS_RECEIVE) { break; }
        if start2.elapsed() > Duration::from_secs(10) { break; }
        thread::sleep(Duration::from_millis(250));
    }
    let still_present = list2.iter().any(|(addr, _)| addr == ADDRESS_RECEIVE);
    assert!(!still_present, "user ADDRESS_RECEIVE should not be present after clearing endpoint; got {:?}", list2);
}
