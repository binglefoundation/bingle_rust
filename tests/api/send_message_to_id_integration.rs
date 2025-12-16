use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}, Mutex};
use std::time::{Duration, Instant};
use libc::sleep;
use rust_comms::api::bingle_api::{BingleApi, StartOptions, OnMessageHandler};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::EngineState;
use rust_comms::stun::{SimpleStunServer, SimpleStunStartOptions};
use rust_comms::blockchain::algo_bingle::AlgoBingle;
use serde_json::json;

#[path = "../setup_localnet.rs"]
mod setup_localnet;
#[path = "../test_util.rs"]
mod test_util;

// Helper: start a relay node at a fixed address
fn start_relay(name: &str, addr: SocketAddr, passphrase: &str) -> BingleApiImpl {
    let mut api = BingleApiImpl::new();
    let opts = StartOptions {
        handle: name.into(),
        algo_passphrase: Some(passphrase.parse().unwrap()),
        static_ip: Some(addr),
        am_relay: true,
        stun_servers: None,
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None,
    };
    api.start(opts).expect("relay start");
    api
}

// Helper: start a client node with given STUN list
fn start_client(name: &str, passphrase: &str, stun_list: Vec<SocketAddr>) -> BingleApiImpl {
    let mut api = BingleApiImpl::new();
    let opts = StartOptions {
        handle: name.into(),
        algo_passphrase: Some(passphrase.parse().unwrap()),
        static_ip: None,
        am_relay: false,
        stun_servers: Some(stun_list),
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None,
    };
    api.start(opts).expect("client start");
    api
}

fn wait_for_registered(api: &BingleApiImpl, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(st) = api.engine_state_for_tests() {
            if st == EngineState::Registered { return true; }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

// Localnet-style integration test for send_message_to_id using two relays and two clients.
// Follows the pattern of bingle_api_endpoint_identify_via_forced_stun and extracts helpers to avoid duplication.
#[test]
fn bingle_api_send_message_to_id_localnet() {
    // This test requires a running local Algorand localnet + indexer.
    // Fail fast if not available per issue requirements.
    if !test_util::should_run_localnet() {
        eprintln!("[skipped] Localnet required: set RUST_COMMS_RUN_LOCALNET=true and ensure local Algorand localnet and indexer are running");
        return;
    }
    // Fixed relay endpoints on loopback (matches pattern in endpoint_identify_integration.rs)
    let r1_port = test_util::find_unused_loopback_port();
    let r2_port = test_util::find_unused_loopback_port();
    assert_ne!(r1_port, 0);
    assert_ne!(r2_port, 0);
    let relay1_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r1_port);
    let relay2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r2_port);

    log::info!("[Test] relay1_addr = {}", relay1_addr);
    log::info!("[Test] relay2_addr = {}", relay2_addr);

    use rust_comms::algo_ops::AppArg;
    use std::fs;
    let cfg = test_util::localnet_config();
    // Ensure relay accounts are funded
    // id_b is in same segment as id_a
    let id_b = "P577OS2FPV7COU3Y43PCTS2IIZ5HAXHBZRHINAATVA5ECCEYKFSEVIYTHE";
    let passphrase_b = "lift all minute first hair appear panel unfold pony property also dinosaur start robot board erupt tent pink essence stem protect ugly orphan absent dust";

    setup_localnet::ensure_localnet_accounts_funded(&cfg,
                                                    &[test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE , id_b])
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
    unsafe { sleep(20); }

    // Start two relays
    let mut relay1 = start_relay("relay1", relay1_addr, test_util::PASSPHRASE_SPEND);
    let mut relay2 = start_relay("relay2", relay2_addr, test_util::PASSPHRASE_RECEIVE);

    // Start two local STUN servers for consistency resolution
    let p1 = test_util::find_unused_loopback_port();
    let p2 = test_util::find_unused_loopback_port();
    let a1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p1);
    let a2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p2);

    let mut s1 = SimpleStunServer::start(SimpleStunStartOptions { bind_addr: a1, attach_to: None, broken_nat: false }).expect("start s1");
    let mut s2 = SimpleStunServer::start(SimpleStunStartOptions { bind_addr: a2, attach_to: None, broken_nat: false }).expect("start s2");

    // Start two clients A and B; B will receive
    let stun_list = vec![a1, a2];
    let mut client_a = start_client("client_a", test_util::PASSPHRASE_10MIL, stun_list.clone());
    let mut client_b = start_client("client_b", passphrase_b, stun_list.clone());

    // Install OnMessage handler for client B to capture delivery
    let received = Arc::new(AtomicBool::new(false));
    let payload_guard: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
    {
        let received_flag = received.clone();
        let payload_store = payload_guard.clone();
        let on_message: Arc<OnMessageHandler> = Arc::new(move |sender, sender_handle, message| {
            log::info!("[Test][client_b on_message] sender={} handle={} msg={}", sender, sender_handle, message);
            if let Ok(mut g) = payload_store.lock() { *g = Some(message); }
            received_flag.store(true, Ordering::SeqCst);
        });
        client_b.set_on_message(Some(on_message));
    }

    // Wait for both clients to reach Registered
    let ok_a = wait_for_registered(&client_a, Duration::from_secs(120));
    let ok_b = wait_for_registered(&client_b, Duration::from_secs(120));
    assert!(ok_a, "client A did not reach Registered state (state = {:?})", client_a.engine_state_for_tests());
    assert!(ok_b, "client B did not reach Registered state (state = {:?})", client_b.engine_state_for_tests());

    // Obtain B's user id and send a message from A -> B using send_message_to_id
    let b_id = client_b.get_my_id().expect("client_b.get_my_id Some");
    let msg = json!({ "text": "hello" });
    let sent = client_a.send_message_to_id(&b_id, msg.clone(), None);
    assert!(sent, "send_message_to_id should return true");

    // Wait up to 60 seconds for receipt on B
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(60) {
        if received.load(Ordering::SeqCst) { break; }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(received.load(Ordering::SeqCst), "client B did not receive the message in time");

    // (Optional) Validate payload shape
    if let Ok(guard) = payload_guard.lock() {
        if let Some(p) = &*guard {
            log::info!("[Test] received payload: {}", p);
            assert_eq!(p.get("text").and_then(|v| v.as_str()), Some("hello"));
        }
    }

    // Tear down
    relay1.stop();
    relay2.stop();
    client_a.stop();
    client_b.stop();
    s1.stop();
    s2.stop();

    if test_util::should_run_localnet() {
        unsafe { std::env::remove_var("BINGLE_APP_ID"); }
    }
}
