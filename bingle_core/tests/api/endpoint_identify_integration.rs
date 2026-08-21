use crate::util::relay_test_util::wait_for_relays_visible;
use crate::util::test_util::init_test_logging;
use crate::util::test_util::register_client_on_blockchain;
use algo_ops::AlgoChainConfig;
use bingle_core::api::bingle_api::{BingleApi, StartOptions};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::blockchain::algo_bingle::AlgoBingle;
use bingle_core::engine::BingleAccessUnsafeForTests;
use bingle_core::engine::EngineState;
use bingle_core::stun::{SimpleStunServer, SimpleStunStartOptions};
use serial_test::serial;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

#[path = "../setup_localnet.rs"]
pub mod setup_localnet;
#[path = "../test_util.rs"]
pub mod test_util;

// Integration test: use BingleApiImpl as the entry point, but mock out
// the discovery by forcing STUN consistent on the underlying Engine. We avoid
// a real Algorand localnet and real relays; instead, we start two relay instances
// (static endpoints) and two client instances, then validate that the clients reach
// Registered with the expected public address.
// Note this has morphed as we register with the relays now after EndpointAvailable
#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn bingle_api_register_via_forced_stun() {
    init_test_logging();

    fn register_relay_static_endpoint(
        handle: &str,
        ops_relay: &algo_ops::AlgoOps,
        ab_creator: &AlgoBingle,
        relay_addr: SocketAddr,
        relay_account: &str,
        relay_passphrase: &str,
        app_id: u64,
        asset_id: u64,
        cfg: &AlgoChainConfig,
    ) {
        let ab_relay = AlgoBingle::new(ops_relay.clone(), app_id, 0);
        ops_relay.opt_in_app(app_id).expect("relay opt-in app");
        register_client_on_blockchain(
            relay_account,
            relay_passphrase,
            handle,
            app_id,
            asset_id,
            &ab_creator.ops,
            cfg.clone(),
        );
        ab_creator
            .set_allow_static(app_id, relay_account, true)
            .expect("set_allow_static");
        ab_creator
            .set_allow_relay(app_id, relay_account, true)
            .expect("set_allow_relay");
        let compact = test_util::get_compact_advert_record(ops_relay, relay_addr, true);
        ab_relay
            .register_endpoint(app_id, &compact)
            .expect("register_endpoint");
    }

    test_util::assert_localnet_available();
    // Set up two relay instances with static endpoints (127.0.0.1 with known, unused ports)
    let r1_port = test_util::find_unused_loopback_port();
    let r2_port = test_util::find_unused_loopback_port();
    assert_ne!(r1_port, 0);
    assert_ne!(r2_port, 0);
    let relay1_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r1_port);
    let relay2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r2_port);

    // Print relay addresses for debugging
    tracing::info!("[Test] relay1_addr = {}", relay1_addr);
    tracing::info!("[Test] relay2_addr = {}", relay2_addr);

    let cfg = test_util::localnet_config();
    // Ensure relay accounts are funded
    setup_localnet::ensure_localnet_accounts_funded(
        &cfg,
        &[test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE],
    )
    .expect("Failed to fund localnet accounts");
    // Build AlgoOps: relay accounts use SPEND/RECEIVE, admin calls use ADDRESS_APP_ADMIN
    let ops_admin = test_util::ops_from_mnemonic(
        test_util::ADDRESS_APP_ADMIN,
        test_util::PASSPHRASE_APP_ADMIN,
        cfg.clone(),
    );
    let ops_relay1 = test_util::ops_from_mnemonic(
        test_util::ADDRESS_SPEND,
        test_util::PASSPHRASE_SPEND,
        cfg.clone(),
    );
    let ops_relay2 = test_util::ops_from_mnemonic(
        test_util::ADDRESS_RECEIVE,
        test_util::PASSPHRASE_RECEIVE,
        cfg.clone(),
    );

    // Deploy the BingleDapp from artifacts using common helper
    let (app_id, asset_id) =
        test_util::deploy_bingle_app_and_asset(&ops_relay1, "BINGLE$", 1_000_000);

    // Create helpers bound to this app
    let ab_creator = AlgoBingle::new(ops_admin.clone(), app_id, 0);

    register_relay_static_endpoint(
        "relay1",
        &ops_relay1,
        &ab_creator,
        relay1_addr,
        test_util::ADDRESS_SPEND,
        test_util::PASSPHRASE_SPEND,
        app_id,
        asset_id,
        &cfg,
    );
    let relay1_expected = vec![(test_util::ADDRESS_SPEND.to_string(), relay1_addr)];
    if !wait_for_relays_visible(
        &ab_creator,
        app_id,
        &relay1_expected,
        Duration::from_secs(60),
    ) {
        panic!("Relay 1 did not become visible via indexer within 60s");
    }

    tracing::info!("[Test] Relay 1 is visible via indexer");

    let cfg = test_util::localnet_config();

    let r1_opts = StartOptions {
        handle: "relay1".into(),
        algo_passphrase: Some(test_util::PASSPHRASE_SPEND.parse().unwrap()),
        static_ip: Some(relay1_addr),
        am_relay: true,
        stun_servers: None,
        algo_provider_config: Some(cfg.clone()),
        algo_network: None,
        app_id: Some(app_id),
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: true,
        log_mode: bingle_core::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    };
    let r2_opts = StartOptions {
        handle: "relay2".into(),
        algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.parse().unwrap()),
        static_ip: Some(relay2_addr),
        am_relay: true,
        stun_servers: None,
        algo_provider_config: Some(cfg.clone()),
        algo_network: None,
        app_id: Some(app_id),
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: true,
        log_mode: bingle_core::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    };

    let relay1 = test_util::start_root_relay(
        "relay1",
        relay1_addr,
        test_util::PASSPHRASE_SPEND,
        app_id,
        r1_opts.algo_provider_config.clone().unwrap(),
    );

    register_relay_static_endpoint(
        "relay2",
        &ops_relay2,
        &ab_creator,
        relay2_addr,
        test_util::ADDRESS_RECEIVE,
        test_util::PASSPHRASE_RECEIVE,
        app_id,
        asset_id,
        &cfg,
    );
    let relay12_expected = vec![
        (test_util::ADDRESS_SPEND.to_string(), relay1_addr),
        (test_util::ADDRESS_RECEIVE.to_string(), relay2_addr),
    ];
    if !wait_for_relays_visible(
        &ab_creator,
        app_id,
        &relay12_expected,
        Duration::from_secs(60),
    ) {
        panic!("Relays did not become visible via indexer within 60s");
    }
    tracing::info!("[Test] Relay 2 is visible via indexer");
    let relay2 = test_util::start_root_relay(
        "relay2",
        relay2_addr,
        test_util::PASSPHRASE_RECEIVE,
        app_id,
        r2_opts.algo_provider_config.clone().unwrap(),
    );
    tracing::info!("[Test] Relay 2 started");

    // Start two local STUN servers we will use for consistency resolution
    let p1 = test_util::find_unused_loopback_port();
    let p2 = test_util::find_unused_loopback_port();
    let a1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p1);
    let a2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p2);

    let mut s1 = SimpleStunServer::start(SimpleStunStartOptions {
        bind_addr: a1,
        attach_to: None,
        broken_nat: false,
    })
    .expect("start s1");
    let mut s2 = SimpleStunServer::start(SimpleStunStartOptions {
        bind_addr: a2,
        attach_to: None,
        broken_nat: false,
    })
    .expect("start s2");

    let stun_list = vec![a1, a2];
    let c1_opts = StartOptions {
        handle: "client1".into(),
        algo_passphrase: Some(test_util::PASSPHRASE_10MIL.parse().unwrap()),
        static_ip: None,
        am_relay: false,
        stun_servers: Some(stun_list.clone()),
        algo_provider_config: Some(cfg.clone()),
        algo_network: None,
        app_id: Some(app_id),
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: true,
        log_mode: bingle_core::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    };

    // A client instance without staticEndpoint; provide the STUN server list to Engine.start
    let client1 = BingleApiImpl::new(&c1_opts);
    let ops_client = test_util::ops_from_mnemonic(
        test_util::ADDRESS_10MIL,
        test_util::PASSPHRASE_10MIL,
        cfg.clone(),
    );
    ops_client.opt_in_app(app_id).expect("client opt-in app");
    register_client_on_blockchain(
        test_util::ADDRESS_10MIL,
        test_util::PASSPHRASE_10MIL,
        "client1",
        app_id,
        asset_id,
        &ops_admin,
        cfg.clone(),
    );

    client1
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| c.start(&c1_opts))
        .expect("client1 start() failed");

    // Wait up to 60 seconds for client engine to enter an operational state (Registered)
    // (allow indexer/DTLS timing variability).
    let wait_start = Instant::now();
    while wait_start.elapsed() < Duration::from_secs(60) {
        match client1.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_state_for_tests()) {
            Some(EngineState::Registered) => break,
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    // State is expected to be operational by this point.
    let s1_state =
        client1.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_state_for_tests());
    assert!(
        matches!(s1_state, Some(EngineState::Registered)),
        "unexpected client1 state: {:?}",
        s1_state
    );

    // Validate that both relays have an entry for client 1 in their DDB backend
    let client1_id = client1.get_my_id().expect("client1 should have an ID");
    tracing::info!(
        "[Test] Client1 started, validating DDB entry for client1 ({}) on both relays",
        client1_id
    );

    let wait_ddb_start = Instant::now();
    let mut r1_ok = false;
    let mut r2_ok = false;
    while wait_ddb_start.elapsed() < Duration::from_secs(60) {
        if !r1_ok {
            if relay1
                .with_engine_mut(|e| e.ddb_backend_lookup_for_tests(&client1_id))
                .is_some()
            {
                tracing::info!("[Test] Relay 1 has DDB entry for client 1");
                r1_ok = true;
            }
        }
        if !r2_ok {
            if relay2
                .with_engine_mut(|e| e.ddb_backend_lookup_for_tests(&client1_id))
                .is_some()
            {
                tracing::info!("[Test] Relay 2 has DDB entry for client 1");
                r2_ok = true;
            }
        }
        if r1_ok && r2_ok {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    assert!(
        r1_ok,
        "Relay 1 should have a DDB entry for client 1 within timeout"
    );
    assert!(
        r2_ok,
        "Relay 2 should have a DDB entry for client 1 within timeout"
    );

    // Stop instances and STUN servers
    relay1.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    relay2.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    client1.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());
    s1.stop();
    s2.stop();
}
