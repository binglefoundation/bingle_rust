use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use rust_comms::algo_bingle::AlgoBingle;
use rust_comms::algo_ops::AlgoOps;
use rust_comms::api::bingle_api::{BingleApi, BingleApiBoth, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::{BingleAccessUnsafeForTests, RelayState};
use rust_comms::relay::discovery::indexer_discover_closure;
use rust_comms::relay::relay_finder::{RelayFinderTrait, RelayInfo};
use rust_comms::relay::relay_updater::RelayUpdater;
use crate::util::test_util::init_test_logging;
use crate::util::relay_test_util::wait_for_relays_visible;

#[path = "../setup_localnet.rs"]
mod setup_localnet;
#[path = "../test_util.rs"]
mod test_util;

#[test]
#[cfg(not(target_os = "ios"))]
#[ntest::timeout(1_800_000)]
fn relay_updater_localnet_e2e_matrix() {
    test_util::assert_localnet_available();

    init_test_logging();

    let cfg = test_util::localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE])
        .expect("Failed to fund localnet accounts");

    let creator_ops = test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone());
    let relay1_ops = creator_ops.clone();
    let relay2_ops = test_util::ops_from_mnemonic(test_util::ADDRESS_RECEIVE, test_util::PASSPHRASE_RECEIVE, cfg.clone());

    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator_ops, "BINGLE$", 1_000_000);
    let creator_ab = AlgoBingle::new(creator_ops.clone(), app_id, 0);

    let relay1_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), test_util::find_unused_loopback_port());
    let relay2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), test_util::find_unused_loopback_port());

    register_relay_static_endpoint(
        &relay1_ops,
        &creator_ab,
        &relay1_addr.to_string(),
        test_util::ADDRESS_SPEND,
        app_id,
    );
    let relay1_expected = vec![(test_util::ADDRESS_SPEND.to_string(), relay1_addr)];
    if !wait_for_relays_visible(&creator_ab, app_id, &relay1_expected, Duration::from_secs(60)) {
        panic!("Relay 1 did not become visible via indexer within 60s");
    }

    let relay1_opts = relay_start_options(
        "relay-updater-localnet-relay-1",
        test_util::PASSPHRASE_SPEND.to_string(),
        relay1_addr,
        cfg.clone(),
        app_id,
    );
    let relay2_opts = relay_start_options(
        "relay-updater-localnet-relay-2",
        test_util::PASSPHRASE_RECEIVE.to_string(),
        relay2_addr,
        cfg.clone(),
        app_id,
    );

    let relay1 = BingleApiImpl::new(&relay1_opts);
    test_util::register_client_on_blockchain(
        test_util::ADDRESS_SPEND,
        test_util::PASSPHRASE_SPEND,
        &relay1_opts.handle,
        app_id,
        asset_id,
        &creator_ops,
        cfg.clone(),
    );
    start_relay_and_wait_available(&relay1, &relay1_opts, "relay1");
    let relay1_id = relay1.get_my_id().expect("relay1 id should be available");
    let relay1_root = RelayInfo::root(relay1_id.clone(), relay1_addr);

    let client1_opts = StartOptions {
        handle: "relay-updater-localnet-client-1".into(),
        algo_passphrase: Some(test_util::PASSPHRASE_10MIL.to_string()),
        static_ip: Some(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            test_util::find_unused_loopback_port(),
        )),
        am_relay: false,
        stun_servers: None,
        algo_provider_config: Some(cfg.clone()),
        algo_network: None,
        app_id: Some(app_id),
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: true,
        log_mode: rust_comms::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    };
    let client1 = BingleApiImpl::new(&client1_opts);
    test_util::register_client_on_blockchain(
        test_util::ADDRESS_10MIL,
        test_util::PASSPHRASE_10MIL,
        &client1_opts.handle,
        app_id,
        asset_id,
        &creator_ops,
        cfg.clone(),
    );
    client1
        .access_unsafe_for_tests(|api| api.start(&client1_opts))
        .expect("client1 start should succeed");
    assert!(
        test_util::wait_for_registered(&client1, Duration::from_secs(60)),
        "client1 did not become Registered in time"
    );
    let client1_id = client1.get_my_id().expect("client1 id should be available");

    tracing::info!("[Test] Setup client1_id: {}", client1_id);

    run_scenario(
        "one_registered_one_available",
        client1_id.clone(),
        client1.clone(),
        app_id,
        cfg.clone(),
        vec![relay1_root.clone()],
        true,
        0,
    );

    relay1.access_unsafe_for_tests(|api| api.stop());
    run_scenario(
        "one_registered_zero_available",
        client1_id.clone(),
        client1.clone(),
        app_id,
        cfg.clone(),
        vec![relay1_root.clone()],
        false,
        0,
    );
    start_relay_and_wait_available(&relay1, &relay1_opts, "relay1-restart");

    test_util::register_client_on_blockchain(
        test_util::ADDRESS_RECEIVE,
        test_util::PASSPHRASE_RECEIVE,
        &relay2_opts.handle,
        app_id,
        asset_id,
        &creator_ops,
        cfg.clone(),
    );
    register_relay_static_endpoint(
        &relay2_ops,
        &creator_ab,
        &relay2_addr.to_string(),
        test_util::ADDRESS_RECEIVE,
        app_id,
    );
    let relay12_expected = vec![
        (test_util::ADDRESS_SPEND.to_string(), relay1_addr),
        (test_util::ADDRESS_RECEIVE.to_string(), relay2_addr),
    ];
    if !wait_for_relays_visible(&creator_ab, app_id, &relay12_expected, Duration::from_secs(60)) {
        panic!("Relays did not become visible via indexer within 60s");
    }

    let relay2 = BingleApiImpl::new(&relay2_opts);
    start_relay_and_wait_available(&relay2, &relay2_opts, "relay2");
    let relay2_id = relay2.get_my_id().expect("relay2 id should be available");
    let relay2_root = RelayInfo::root(relay2_id.clone(), relay2_addr);
    let two_roots = vec![relay1_root.clone(), relay2_root.clone()];

    run_scenario(
        "two_registered_two_available",
        client1_id.clone(),
        client1.clone(),
        app_id,
        cfg.clone(),
        two_roots.clone(),
        true,
        0
    );

    // TODO: these need a non-root relay starting
    // left for now as slow

    //
    // run_scenario(
    //     "two_registered_two_available_two_non_root",
    //     client1_id.clone(),
    //     client1.clone(),
    //     two_roots.clone(),
    //     true,
    //     2,
    // );
    //
    // relay2.access_unsafe_for_tests(|api| api.stop());
    // run_scenario(
    //     "two_registered_one_available",
    //     client1_id.clone(),
    //     client1.clone(),
    //     two_roots.clone(),
    //     true,
    //     0,
    // );
    //
    // relay1.access_unsafe_for_tests(|api| api.stop());
    // run_scenario(
    //     "two_registered_zero_available",
    //     client1_id,
    //     client1.clone(),
    //     two_roots,
    //     false,
    //     0,
    // );
    //
    // client2.access_unsafe_for_tests(|api| api.stop());
    client1.access_unsafe_for_tests(|api| api.stop());
}

fn run_scenario(
    scenario_name: &str,
    my_id: String,
    api: Arc<BingleApiImpl>,
    app_id: u64,
    cfg: rust_comms::algo_ops::AlgoChainConfig,
    registered_roots: Vec<RelayInfo>,
    expect_selected: bool,
    min_expected_non_root_count: usize,
) {
    tracing::info!("[Test] Running scenario: {}", scenario_name);
    let updater = make_updater(my_id.clone(), api, app_id, cfg);
    updater.init_from_blockchain();

    let init_cache = updater.relay_info_cache().list_all_relays(my_id.as_str(), false);
    assert_eq!(
        init_cache.len(),
        registered_roots.len(),
        "{scenario_name}: init cache size should match registered roots"
    );

    let expected_root_ids: BTreeSet<String> = registered_roots.iter().map(|relay| relay.id.clone()).collect();
    let init_root_ids: BTreeSet<String> = init_cache.iter().map(|relay| relay.id.clone()).collect();
    tracing::info!("[Test] init_root_ids: {:?}", init_root_ids);
    assert_eq!(
        init_root_ids, expected_root_ids,
        "{scenario_name}: init cache root ids mismatch"
    );
    for relay in &init_cache {
        assert_eq!(
            relay.state,
            Some(RelayState::Unknown),
            "{scenario_name}: expected Unknown after init for {}",
            relay.id
        );
    }

    // The following should return a consistent result immediately
    // Assumes test nodes are stable by here
    let selected = updater.relay_select_and_query(&[]);
    if !expect_selected {
        assert!(selected.is_none(), "{scenario_name}: expected no relay selected");
        return;
    }

    assert!(selected.is_some(), "{scenario_name}: expected selected relay");
    let selected_relay = selected.expect("selected relay should exist after is_some check");
    assert!(
        expected_root_ids.contains(&selected_relay.id),
        "{scenario_name}: selected relay should be one of registered roots"
    );
    assert_eq!(
        selected_relay.state,
        Some(RelayState::Available),
        "{scenario_name}: selected relay should be Available"
    );

    let all_relays = updater.relay_info_cache().list_all_relays(my_id.as_str(), true);
    tracing::info!("[Test] {} all_relays: {:?}", scenario_name, all_relays);
    let non_root_count = all_relays
        .iter()
        .filter(|relay| !expected_root_ids.contains(&relay.id))
        .count();

    assert!(non_root_count >= min_expected_non_root_count, "{scenario_name}: expected at least {min_expected_non_root_count} non-root relays");

    let selected_in_cache = all_relays.iter().find(|relay| relay.id == selected_relay.id);
    assert!(
        selected_in_cache.is_some(),
        "{scenario_name}: selected relay should be present in cache"
    );
}

fn make_updater(my_id: String, api: Arc<BingleApiImpl>, app_id: u64, cfg: rust_comms::algo_ops::AlgoChainConfig) -> RelayUpdater {
    let api_both: Arc<dyn BingleApiBoth> = api;
    RelayUpdater::new_with_api(
        my_id,
        Arc::downgrade(&api_both),
        indexer_discover_closure(app_id, Some(cfg)),
    )
}

fn relay_start_options(
    handle: &str,
    passphrase: String,
    static_ip: SocketAddr,
    cfg: rust_comms::algo_ops::AlgoChainConfig,
    app_id: u64,
) -> StartOptions {
    StartOptions {
        handle: handle.into(),
        algo_passphrase: Some(passphrase),
        static_ip: Some(static_ip),
        am_relay: true,
        stun_servers: None,
        algo_provider_config: Some(cfg),
        algo_network: None,
        app_id: Some(app_id),
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: true,
        log_mode: rust_comms::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    }
}
fn register_relay_static_endpoint(
    relay_ops: &AlgoOps,
    creator_ab: &AlgoBingle,
    relay_address: &str,
    relay_account: &str,
    app_id: u64,
) {
    let relay_ab = AlgoBingle::new(relay_ops.clone(), app_id, 0);
    relay_ops.opt_in_app(app_id).expect("relay opt-in should succeed");
    creator_ab
        .set_allow_static(app_id, relay_account, true)
        .expect("set_allow_static should succeed");
    relay_ab
        .register_endpoint(app_id, relay_address)
        .expect("register_endpoint should succeed");
}


fn start_relay_and_wait_available(relay: &Arc<BingleApiImpl>, opts: &StartOptions, relay_name: &str) {
    relay
        .access_unsafe_for_tests(|api| api.start(opts))
        .unwrap_or_else(|err| panic!("{relay_name} start should succeed: {err}"));

    assert!(
        test_util::wait_for_relay_available(relay, Duration::from_secs(60)),
        "{relay_name} did not become Available in time"
    );
}