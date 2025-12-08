use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::EngineState;
use rust_comms::stun::{SimpleStunServer, SimpleStunStartOptions};
use rust_comms::blockchain::algo_bingle::AlgoBingle;

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
#[ignore]
fn bingle_api_endpoint_identify_via_forced_stun() {
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

    // If a localnet + indexer is available, deploy the dApp and register relay endpoints on-chain.
    if test_util::should_run_localnet() {
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
        let app_id = ops_creator.deploy_app(&approval, &clear, None).expect("deploy app").expect("app id");

        // Set Bingle price to 1 (not strictly required for endpoint registration)
        let _ = ops_creator.call_app(app_id, None, Some("set_bingle_price(uint64)void"), &[AppArg::Uint(1)]);

        // Create helpers bound to this app
        let ab_creator = AlgoBingle::new(ops_creator.clone(), app_id, 0);
        let ab_r1 = AlgoBingle::new(ops_relay1.clone(), app_id, 0);
        let ab_r2 = AlgoBingle::new(ops_relay2.clone(), app_id, 0);

        // Opt relays into the app and allow static endpoints
        ops_relay1.opt_in_app(app_id).expect("relay1 opt-in app");
        ops_relay2.opt_in_app(app_id).expect("relay2 opt-in app");
        // Grant allow_static for relay accounts via creator
        ab_creator.set_allow_static(app_id, test_util::ADDRESS_SPEND, true).expect("set_allow_static r1");
        ab_creator.set_allow_static(app_id, test_util::ADDRESS_RECEIVE, true).expect("set_allow_static r2");

        // Register endpoints for both relays
        ab_r1.register_endpoint(app_id, &relay1_addr.to_string()).expect("register_endpoint r1");
        ab_r2.register_endpoint(app_id, &relay2_addr.to_string()).expect("register_endpoint r2");

        // Tell Engine/handlers to use indexer-based discovery for this app id
        unsafe { std::env::set_var("BINGLE_APP_ID", app_id.to_string()); }
    }

    let mut relay1 = BingleApiImpl::new();
    let mut relay2 = BingleApiImpl::new();

    let r1_opts = StartOptions { handle: "relay1".into(), algo_passphrase: Some(test_util::PASSPHRASE_SPEND.parse().unwrap()), static_ip: Some(relay1_addr), am_relay: true, stun_servers: None, algo_provider_config: None, algo_network: None, app_id: None, asset_id: None, log_level: None };
    let r2_opts = StartOptions { handle: "relay2".into(), algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.parse().unwrap()), static_ip: Some(relay2_addr), am_relay: true, stun_servers: None, algo_provider_config: None, algo_network: None, app_id: None, asset_id: None, log_level: None };

    // Start relays (no assertions about DTLS; we use them only as placeholders)
    let _ = relay1.start(r1_opts).expect("relay1 start() failed");
    let _ = relay2.start(r2_opts).expect("relay2 start() failed");

    // Start two local STUN servers we will use for consistency resolution
    let p1 = test_util::find_unused_loopback_port();
    let p2 = test_util::find_unused_loopback_port();
    let a1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p1);
    let a2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p2);

    let mut s1 = SimpleStunServer::start(SimpleStunStartOptions { bind_addr: a1, attach_to: None, broken_nat: false }).expect("start s1");
    let mut s2 = SimpleStunServer::start(SimpleStunStartOptions { bind_addr: a2, attach_to: None, broken_nat: false }).expect("start s2");

    // A client instance without staticEndpoint; provide the STUN server list to Engine.start
    let mut client1 = BingleApiImpl::new();

    let stun_list = vec![a1, a2];
    let c1_opts = StartOptions { handle: "client1".into(), algo_passphrase: Some(test_util::PASSPHRASE_10MIL.parse().unwrap()), static_ip: None, am_relay: false, stun_servers: Some(stun_list.clone()), algo_provider_config: None, algo_network: None, app_id: None, asset_id: None, log_level: None };

    client1.start(c1_opts).expect("client1 start() failed");

    // Wait up to 60 seconds for client engine to enter EndpointAvailable (allow indexer/DTLS timing)
    let wait_start = Instant::now();
    while wait_start.elapsed() < Duration::from_secs(60) {
        match client1.engine_state_for_tests() {
            Some(EngineState::EndpointAvailable) => break,
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    // State is expected to be EndpointAvailable - do not change this!
    let s1_state = client1.engine_state_for_tests();
    assert!(matches!(s1_state, Some(EngineState::EndpointAvailable)  ), "unexpected client1 state: {:?}", s1_state);

    // Stop instances and STUN servers
    relay1.stop();
    relay2.stop();
    client1.stop();
    s1.stop();
    s2.stop();

    // Clean up env var if we set it
    if test_util::should_run_localnet() {
        unsafe { std::env::remove_var("BINGLE_APP_ID"); }
    }
}
