use bingle_core::algo_ops::AlgoChainConfig;
use bingle_core::blockchain::algo_bingle::AlgoBingle;

use crate::setup_localnet;
use crate::util::test_util;

use std::net::SocketAddr;
use std::thread;
use std::time::{Duration, Instant};
use test_util::{
    ADDRESS_RECEIVE, ADDRESS_SPEND, PASSPHRASE_RECEIVE, PASSPHRASE_SPEND, localnet_config,
    ops_from_mnemonic,
};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn set_allow_and_register_endpoint_then_list_and_clear() {
    test_util::assert_localnet_available();
    // Ensure test accounts are funded
    let cfg: AlgoChainConfig = localnet_config();

    // Extra guard: ensure Indexer is reachable; otherwise skip to avoid false negatives when only algod is up.
    {
        use bingle_core::blockchain::algo_ops::AlgoOps;
        let tmp_ops = AlgoOps::new(
            None,
            Some(test_util::ADDRESS_SPEND.to_string()),
            Some(cfg.clone()),
        );
        let indexer = tmp_ops
            .indexer_client()
            .expect("failed to build indexer client");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio rt");
        match rt.block_on(indexer.health()) {
            Ok(_) => { /* ok */ }
            Err(e) => {
                eprintln!("SKIP: indexer not reachable: {}", e);
                return;
            }
        }
    }

    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[ADDRESS_SPEND, ADDRESS_RECEIVE])
        .expect(
            "Failed to ensure localnet test accounts funded; install algokit and start localnet",
        );

    // Creator and user ops
    let creator = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());
    let user = ops_from_mnemonic(ADDRESS_RECEIVE, PASSPHRASE_RECEIVE, cfg.clone());

    // Deploy the dapp from TEAL artifacts using common helper
    let app_id = test_util::deploy_bingle_app(&creator);

    // User & creator opts in to the app to enable local state
    user.opt_in_app(app_id).expect("user opt-in app");
    creator.opt_in_app(app_id).expect("user opt-in app");

    // Grant static permission for the user via the app creator, then user registers a static endpoint
    let ab_creator = AlgoBingle::new(creator.clone(), app_id, 0);
    let _ = ab_creator
        .set_allow_static(app_id, ADDRESS_RECEIVE, true)
        .expect("set_allow_static call");
    let ab_user = AlgoBingle::new(user.clone(), app_id, 0);
    let addr: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let endpoint = test_util::get_compact_advert_record(&user, addr, false);
    let _ = ab_user
        .register_endpoint(app_id, &endpoint)
        .expect("register_endpoint call");

    // Query via Indexer and validate our account appears with the endpoint.
    // Indexer is eventually consistent; poll for up to ~10 seconds.
    let start = Instant::now();
    let mut list: Vec<(String, String)>;
    let ab = AlgoBingle::new(user.clone(), app_id, 0);
    loop {
        list = ab
            .list_static_endpoints_via_indexer(app_id)
            .expect("indexer list");
        if list
            .iter()
            .any(|(addr, ep)| addr == ADDRESS_RECEIVE && ep == &endpoint)
        {
            break;
        }
        if start.elapsed() > Duration::from_secs(45) {
            break;
        }
        thread::sleep(Duration::from_millis(250));
    }
    let mut found = false;
    for (addr, ep) in &list {
        if addr == ADDRESS_RECEIVE {
            assert_eq!(ep, &endpoint, "endpoint mismatch for user");
            found = true;
        }
    }
    assert!(
        found,
        "user ADDRESS_RECEIVE not found in static_endpoint indexer list: {:?}",
        list
    );

    // Clear the endpoint and verify removal (also with polling to account for indexer lag)
    let _ = ab_user
        .register_endpoint(app_id, "")
        .expect("clear endpoint call");
    let start2 = Instant::now();
    let mut list2;
    loop {
        list2 = ab
            .list_static_endpoints_via_indexer(app_id)
            .expect("indexer list after clear");
        if !list2.iter().any(|(addr, _)| addr == ADDRESS_RECEIVE) {
            break;
        }
        if start2.elapsed() > Duration::from_secs(10) {
            break;
        }
        thread::sleep(Duration::from_millis(250));
    }
    let still_present = list2.iter().any(|(addr, _)| addr == ADDRESS_RECEIVE);
    assert!(
        !still_present,
        "user ADDRESS_RECEIVE should not be present after clearing endpoint; got {:?}",
        list2
    );
}
