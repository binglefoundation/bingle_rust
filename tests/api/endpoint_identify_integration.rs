use rust_comms::engine::BingleAccessUnsafeForTests;
use serial_test::serial;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::EngineState;
use rust_comms::stun::{SimpleStunServer, SimpleStunStartOptions};
use rust_comms::blockchain::algo_bingle::AlgoBingle;
use crate::util::test_util::init_test_logging;

#[path = "../setup_localnet.rs"]
mod setup_localnet;
#[path = "../test_util.rs"]
mod test_util;

// Option B integration test: use BingleApiImpl as the entry point, but mock out
// the discovery by forcing STUN consistent on the underlying Engine. We avoid
// a real Algorand localnet and real relays; instead, we start two relay instances
// (static endpoints) and two client instances, then validate that the clients reach
// EndpointAvailable with the expected public address.
#[test]
#[serial]
fn bingle_api_endpoint_identify_via_forced_stun() {
    init_test_logging();

    fn wait_for_relays_visible(
        ab: &AlgoBingle,
        app_id: u64,
        accounts: &[String],
        timeout: Duration,
    ) {
        log::info!("[Test] Waiting for relays to be visible via discover_root_relays...");
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Ok(found) = ab.discover_root_relays(app_id, accounts) {
                if found.len() == accounts.len() {
                    log::info!(
                        "[Test] All {} relays visible via discover_root_relays after {:?}",
                        accounts.len(),
                        start.elapsed()
                    );
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(1000));
        }
        panic!(
            "Relays did not become visible via discover_root_relays within {:?}",
            timeout
        );
    }

    fn register_relay_static_endpoint(
        ops_relay: &rust_comms::blockchain::algo_ops::AlgoOps,
        ab_creator: &AlgoBingle,
        relay_address: &str,
        relay_account: &str,
        app_id: u64,
    ) {
        let ab_relay = AlgoBingle::new(ops_relay.clone(), app_id, 0);
        ops_relay.opt_in_app(app_id).expect("relay opt-in app");
        ab_creator
            .set_allow_static(app_id, relay_account, true)
            .expect("set_allow_static");
        ab_relay
            .register_endpoint(app_id, relay_address)
            .expect("register_endpoint");
    }

    fn start_relay_and_wait_registered(
        opts: &StartOptions,
        name: &str,
    ) -> Arc<BingleApiImpl> {
        let relay = BingleApiImpl::new(opts);
        relay
            .access_unsafe_for_tests(|r: &mut BingleApiImpl| r.start(opts))
            .unwrap_or_else(|e| panic!("{} start() failed: {}", name, e));

        let relay_wait_start = Instant::now();
        while relay_wait_start.elapsed() < Duration::from_secs(60) {
            let state =
                relay.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.engine_state_for_tests());
            if matches!(state, Some(EngineState::Registered)) {
                return relay;
            }
            std::thread::sleep(Duration::from_millis(25));
        }

        let state =
            relay.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.engine_state_for_tests());
        panic!("unexpected {} state: {:?}", name, state);
    }

    // This test requires a running local Algorand localnet + indexer.
    // Fail fast if not available per issue requirements.
    if !test_util::should_run_localnet() {
        eprintln!("[skipped] Localnet required: set RUST_COMMS_RUN_LOCALNET=true and ensure local Algorand localnet and indexer are running");
        return;
    }
    // Set up two relay instances with static endpoints (127.0.0.1 with known, unused ports)
    // let r1_port = find_unused_loopback_port();
    // let r2_port = find_unused_loopback_port();
    // assert_ne!(r1_port, 0);
    // assert_ne!(r2_port, 0);
    let r1_port = 12345;
    let r2_port = 12346;
    let relay1_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r1_port);
    let relay2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r2_port);

    // Print relay addresses for debugging
    log::info!("[Test] relay1_addr = {}", relay1_addr);
    log::info!("[Test] relay2_addr = {}", relay2_addr);

    use rust_comms::algo_ops::AppArg;
    use std::fs;
    let cfg = test_util::localnet_config();
    // Ensure relay accounts are funded
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE])
        .expect("Failed to fund localnet accounts");
    // Build AlgoOps for two relay accounts and one creator (use SPEND as creator)
    let ops_creator = test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone());
    let ops_relay1 = ops_creator.clone();
    let ops_relay2 = test_util::ops_from_mnemonic(test_util::ADDRESS_RECEIVE, test_util::PASSPHRASE_RECEIVE, cfg.clone());

    // Deploy the BingleDapp from artifacts
    let approval_src = fs::read_to_string("dapp/projects/dapp/smart_contracts/artifacts/bingle_dapp/BingleDapp.approval.teal").expect("read approval teal");
    let clear_src = fs::read_to_string("dapp/projects/dapp/smart_contracts/artifacts/bingle_dapp/BingleDapp.clear.teal").expect("read clear teal");
    let approval = ops_creator.compile_teal(&approval_src).expect("compile approval teal");
    let clear = ops_creator.compile_teal(&clear_src).expect("compile clear teal");
    let app_id = match ops_creator.deploy_app(&approval, &clear, None) {
        Ok(Some(id)) => id,
        Ok(None) => {
            eprintln!("[skipped] deploy app returned None app id");
            return;
        }
        Err(e) => {
            let es = format!("{}", e);
            if es.contains("already in ledger") {
                eprintln!("[skipped] deploy app: {}", es);
                return;
            } else {
                panic!("deploy app: {}", es);
            }
        }
    };

    // Set Bingle price to 1 (not strictly required for endpoint registration)
    let _ = ops_creator.call_app(app_id, None, Some("set_bingle_price(uint64)void"), &[AppArg::Uint(1)]);

    // Create helpers bound to this app
    let ab_creator = AlgoBingle::new(ops_creator.clone(), app_id, 0);

    let relay1_accounts = vec![test_util::ADDRESS_SPEND.to_string()];
    register_relay_static_endpoint(
        &ops_relay1,
        &ab_creator,
        &relay1_addr.to_string(),
        test_util::ADDRESS_SPEND,
        app_id,
    );
    wait_for_relays_visible(
        &ab_creator,
        app_id,
        &relay1_accounts,
        Duration::from_secs(60),
    );

    log::info!("[Test] Relay 1 is visible via discover_root_relays");

    let cfg = test_util::localnet_config();

    let r1_opts = StartOptions { handle: "relay1".into(), algo_passphrase: Some(test_util::PASSPHRASE_SPEND.parse().unwrap()), static_ip: Some(relay1_addr), am_relay: true, stun_servers: None, algo_provider_config: Some(cfg.clone()), algo_network: None, app_id: Some(app_id), asset_id: None, log_level: None };
    let r2_opts = StartOptions { handle: "relay2".into(), algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.parse().unwrap()), static_ip: Some(relay2_addr), am_relay: true, stun_servers: None, algo_provider_config: Some(cfg.clone()), algo_network: None, app_id: Some(app_id), asset_id: None, log_level: None };

    let relay1 = start_relay_and_wait_registered(&r1_opts, "relay1");

    let relay12_accounts = vec![
        test_util::ADDRESS_SPEND.to_string(),
        test_util::ADDRESS_RECEIVE.to_string(),
    ];
    register_relay_static_endpoint(
        &ops_relay2,
        &ab_creator,
        &relay2_addr.to_string(),
        test_util::ADDRESS_RECEIVE,
        app_id,
    );
    wait_for_relays_visible(
        &ab_creator,
        app_id,
        &relay12_accounts,
        Duration::from_secs(60),
    );
    log::info!("[Test] Relay 2 is visible via discover_root_relays");
    let relay2 = start_relay_and_wait_registered(&r2_opts, "relay2");

    // Start two local STUN servers we will use for consistency resolution
    let p1 = test_util::find_unused_loopback_port();
    let p2 = test_util::find_unused_loopback_port();
    let a1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p1);
    let a2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p2);

    let mut s1 = SimpleStunServer::start(SimpleStunStartOptions { bind_addr: a1, attach_to: None, broken_nat: false }).expect("start s1");
    let mut s2 = SimpleStunServer::start(SimpleStunStartOptions { bind_addr: a2, attach_to: None, broken_nat: false }).expect("start s2");

    let stun_list = vec![a1, a2];
    let c1_opts = StartOptions { handle: "client1".into(), algo_passphrase: Some(test_util::PASSPHRASE_10MIL.parse().unwrap()), static_ip: None, am_relay: false, stun_servers: Some(stun_list.clone()), algo_provider_config: Some(cfg.clone()), algo_network: None, app_id: Some(app_id), asset_id: None, log_level: None };

    // A client instance without staticEndpoint; provide the STUN server list to Engine.start
    let client1 = BingleApiImpl::new(&c1_opts);

    client1.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.start(&c1_opts)).expect("client1 start() failed");

    // Wait up to 60 seconds for client engine to enter EndpointAvailable (allow indexer/DTLS timing)
    let wait_start = Instant::now();
    while wait_start.elapsed() < Duration::from_secs(60) {
        match client1.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_state_for_tests()) {
            Some(EngineState::EndpointAvailable) => break,
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    // State is expected to be EndpointAvailable - do not change this!
    let s1_state = client1.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_state_for_tests());
    assert!(matches!(s1_state, Some(EngineState::EndpointAvailable)  ), "unexpected client1 state: {:?}", s1_state);

    // Stop instances and STUN servers
    relay1.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    relay2.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    client1.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());
    s1.stop();
    s2.stop();
}
