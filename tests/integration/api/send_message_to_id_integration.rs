// To run these tests for integration test:
// cargo test --test all integration::api::send_message_to_id_integration -- --ignored

use rust_comms::api::bingle_api::{BingleApi, OnMessageHandler, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::blockchain::algo_bingle::AlgoBingle;
use rust_comms::engine::BingleAccessUnsafeForTests;
use rust_comms::stun::{SimpleStunServer, SimpleStunStartOptions};
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};
use std::time::{Duration, Instant};
use crate::setup_localnet;
use crate::util::test_util;
use crate::util::test_util::register_client_on_blockchain;
use crate::util::relay_test_util::wait_for_relays_visible;
use serial_test::serial;

const ADDRESS_B: &str = "P577OS2FPV7COU3Y43PCTS2IIZ5HAXHBZRHINAATVA5ECCEYKFSEVIYTHE";
const PASSPHRASE_B: &str = "lift all minute first hair appear panel unfold pony property also dinosaur start robot board erupt tent pink essence stem protect ugly orphan absent dust";

// Helper: start a relay node at a fixed address
fn start_root_relay(name: &str, addr: SocketAddr, passphrase: &str, app_id: u64, cfg: rust_comms::blockchain::algo_ops::AlgoChainConfig) -> Arc<BingleApiImpl> {
    tracing::info!("[Test] start_root_relay name={} addr={} app_id={}", name, addr, app_id);
    let opts = StartOptions {
        handle: name.into(),
        algo_passphrase: Some(passphrase.parse().unwrap()),
        static_ip: Some(addr),
        am_relay: true,
        stun_servers: None,
        algo_provider_config: Some(cfg),
        algo_network: None,
        app_id: Some(app_id),
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: false, log_mode: rust_comms::util::logging::LogMode::Plain,
    };
    let api = BingleApiImpl::new(&opts);
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts)).expect("relay start");
    tracing::info!("[Test] root relay {} started, wait for registered", name);

    test_util::wait_for_registered(&api, Duration::from_secs(30));
    tracing::info!("[Test] root relay {} registered", name);

    if !test_util::wait_for_relay_available(&api, Duration::from_secs(360)) {
        panic!("root relay {} did not become Available within 360s", name);
    }
    tracing::info!("[Test] root relay {} Available", name);

    api
}

// Helper: start a relay node using STUN discovery (non-root)
fn start_relay(name: &str, passphrase: &str, stun_list: Vec<SocketAddr>, app_id: u64, cfg: rust_comms::blockchain::algo_ops::AlgoChainConfig) -> Arc<BingleApiImpl> {
    tracing::info!("[Test] start_relay name={} stun_list={:?} app_id={}", name, stun_list, app_id);
    let opts = StartOptions {
        handle: name.into(),
        algo_passphrase: Some(passphrase.parse().unwrap()),
        static_ip: None,
        am_relay: true,
        stun_servers: Some(stun_list),
        algo_provider_config: Some(cfg),
        algo_network: None,
        app_id: Some(app_id),
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: false, log_mode: rust_comms::util::logging::LogMode::Plain,
    };
    let api = BingleApiImpl::new(&opts);
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts)).expect("relay start");
    tracing::info!("[Test] non-root relay {} started", name);

    if !test_util::wait_for_relay_available(&api, Duration::from_secs(360)) {
        panic!("non-root relay {} did not become Available within 360s", name);
    }
    tracing::info!("[Test] non-root relay {} Available", name);

    api
}

// Helper: start a client node at a fixed address (for restart scenarios)
fn start_client_at_addr(name: &str, passphrase: &str, addr: SocketAddr, stun_list: Vec<SocketAddr>, app_id: u64, cfg: rust_comms::blockchain::algo_ops::AlgoChainConfig) -> Arc<BingleApiImpl> {
    tracing::info!("[Test] start_client_at_addr name={} addr={} stun_list={:?} app_id={}", name, addr, stun_list, app_id);
    let opts = StartOptions {
        handle: name.into(),
        algo_passphrase: Some(passphrase.parse().unwrap()),
        static_ip: Some(addr),
        am_relay: false,
        stun_servers: Some(stun_list),
        algo_provider_config: Some(cfg),
        algo_network: None,
        app_id: Some(app_id),
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: false, log_mode: rust_comms::util::logging::LogMode::Plain,
    };
    let api = BingleApiImpl::new(&opts);
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts)).expect("client start at addr");
    tracing::info!("[Test] client {} started at {}", name, addr);
    api
}

// Helper: start a client node with given STUN list
fn start_client(name: &str, passphrase: &str, stun_list: Vec<SocketAddr>, app_id: u64, cfg: rust_comms::blockchain::algo_ops::AlgoChainConfig) -> Arc<BingleApiImpl> {
    tracing::info!("[Test] start_client name={} stun_list={:?} app_id={}", name, stun_list, app_id);
    let opts = StartOptions {
        handle: name.into(),
        algo_passphrase: Some(passphrase.parse().unwrap()),
        static_ip: None,
        am_relay: false,
        stun_servers: Some(stun_list),
        algo_provider_config: Some(cfg),
        algo_network: None,
        app_id: Some(app_id),
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: false, log_mode: rust_comms::util::logging::LogMode::Plain,
    };
    let api = BingleApiImpl::new(&opts);
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts)).expect("client start");
    tracing::info!("[Test] client {} started", name);
    api
}



fn register_relays(app_id: u64, asset_id: u64, relay1_addr: SocketAddr, relay2_addr: SocketAddr) {
    let cfg = test_util::localnet_config();
    let ops_creator = test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone());
    let ops_relay1 = ops_creator.clone();
    let ops_relay2 = test_util::ops_from_mnemonic(test_util::ADDRESS_RECEIVE, test_util::PASSPHRASE_RECEIVE, cfg.clone());

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

    // handle must be registered
    register_client_on_blockchain(
        test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, "relay1",
        app_id, asset_id, &ops_creator, cfg.clone(),
    );
    register_client_on_blockchain(
        test_util::ADDRESS_RECEIVE, test_util::PASSPHRASE_RECEIVE, "relay2",
        app_id, asset_id, &ops_creator, cfg.clone(),
    );

    // Register endpoints for both relays
    ab_r1.register_endpoint(app_id, &relay1_addr.to_string()).expect("register_endpoint r1");
    ab_r2.register_endpoint(app_id, &relay2_addr.to_string()).expect("register_endpoint r2");

    // Wait for indexer to see the registered endpoints
    tracing::info!("[Test] Waiting for relays to be visible via list_static_endpoints_via_indexer_sync...");
    let expected = vec![
        (test_util::ADDRESS_SPEND.to_string(), relay1_addr),
        (test_util::ADDRESS_RECEIVE.to_string(), relay2_addr),
    ];
    if !wait_for_relays_visible(&ab_creator, app_id, &expected, Duration::from_secs(60)) {
        panic!("Relays did not become visible via list_static_endpoints_via_indexer_sync within 60s");
    }
}

fn setup_stun_servers(broken_nat: bool) -> (SimpleStunServer, SimpleStunServer, Vec<SocketAddr>) {
    let p1 = test_util::find_unused_loopback_port();
    let p2 = test_util::find_unused_loopback_port();
    let a1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p1);
    let a2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p2);

    let s1 = SimpleStunServer::start(SimpleStunStartOptions { bind_addr: a1, attach_to: None, broken_nat }).expect("start s1");
    let s2 = SimpleStunServer::start(SimpleStunStartOptions { bind_addr: a2, attach_to: None, broken_nat }).expect("start s2");

    (s1, s2, vec![a1, a2])
}

fn setup_on_message(
    api: &Arc<BingleApiImpl>,
    received: &Arc<AtomicBool>,
    payload_guard: &Arc<Mutex<Option<serde_json::Value>>>,
    who_guard: &Arc<Mutex<Option<(String, String)>>>,
) {
    let received_flag = received.clone();
    let payload_store = payload_guard.clone();
    let who_store = who_guard.clone();
    let name = api.get_handle().unwrap_or_else(|| "unknown".to_string());
    let on_message: Arc<OnMessageHandler> = Arc::new(move |sender, sender_handle, message| {
        tracing::info!("[Test][{} on_message] sender={} handle={} msg={}", name, sender, sender_handle, message);
        if let Ok(mut g) = payload_store.lock() { *g = Some(message); }
        if let Ok(mut who) = who_store.lock() { *who = Some((sender.clone(), sender_handle.clone())); }
        received_flag.store(true, Ordering::SeqCst);
    });
    api.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.set_on_message(Some(on_message)));
}

fn run_send_message_to_id_test(broken_nat: bool) {
    test_util::init_test_logging_with_filter("info,rust_comms::dtls=info");

    // This test requires a running local Algorand localnet + indexer.
    if !test_util::should_run_localnet() {
        eprintln!("[skipped] Localnet required: set RUST_COMMS_RUN_LOCALNET=true and ensure local Algorand localnet and indexer are running");
        return;
    }

    let cfg = test_util::localnet_config();
    let _ = setup_localnet::ensure_localnet_accounts_funded(&cfg, &[test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE, test_util::ADDRESS_10MIL, ADDRESS_B]);

    // Fixed relay endpoints on loopback
    let r1_port = test_util::find_unused_loopback_port();
    let r2_port = test_util::find_unused_loopback_port();
    assert_ne!(r1_port, 0);
    assert_ne!(r2_port, 0);
    let relay1_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r1_port);
    let relay2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r2_port);

    tracing::info!("[Test] relay1_addr = {}", relay1_addr);
    tracing::info!("[Test] relay2_addr = {}", relay2_addr);

    // Deploy app + asset so we can register client handles on-chain for reverse lookups
    let cfg = test_util::localnet_config();
    let creator = test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone());
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE$", 1_000_000);

    register_relays(app_id, asset_id, relay1_addr, relay2_addr);

    // Start two relays
    let relay1 = start_root_relay("relay1", relay1_addr, test_util::PASSPHRASE_SPEND, app_id, cfg.clone());
    let relay2 = start_root_relay("relay2", relay2_addr, test_util::PASSPHRASE_RECEIVE, app_id, cfg.clone());

    // Start two local STUN servers
    let (mut s1, mut s2, stun_list) = setup_stun_servers(broken_nat);

    // Before sending: ensure clients have handles registered on-chain so reverse lookup by id succeeds.
    register_client_on_blockchain(
        test_util::ADDRESS_10MIL, test_util::PASSPHRASE_10MIL, "client_a",
        app_id, asset_id, &creator, cfg.clone(),
    );
    register_client_on_blockchain(
        ADDRESS_B, PASSPHRASE_B, "client_b",
        app_id, asset_id, &creator, cfg.clone(),
    );

    // Start two clients A and B; B will receive
    let client_a = start_client("client_a", test_util::PASSPHRASE_10MIL, stun_list.clone(), app_id, cfg.clone());
    let client_b = start_client("client_b", PASSPHRASE_B, stun_list.clone(), app_id, cfg.clone());

    // Install OnMessage handler for client B to capture delivery and who sent it
    let received = Arc::new(AtomicBool::new(false));
    let payload_guard: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
    let who_guard: Arc<Mutex<Option<(String, String)>>> = Arc::new(Mutex::new(None));
    setup_on_message(&client_b, &received, &payload_guard, &who_guard);

    // Wait for both clients to reach Registered
    let ok_a = test_util::wait_for_registered(&client_a, Duration::from_secs(120));
    let ok_b = test_util::wait_for_registered(&client_b, Duration::from_secs(120));
    assert!(ok_a, "client A did not reach Registered state (state = {:?})", client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_state_for_tests()));
    assert!(ok_b, "client B did not reach Registered state (state = {:?})", client_b.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_state_for_tests()));

    // Validate DDB lookups: use DdbClientImpl::lookup (via API) to check registered endpoints
    {
        let id_r1 = relay1.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("relay1 id");
        let id_r2 = relay2.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("relay2 id");
        let id_a = client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("client_a id");
        let id_b = client_b.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("client_b id");

        tracing::info!("[Test] Validating DDB lookups: id_r1={}, id_r2={}, id_a={}, id_b={}", id_r1, id_r2, id_a, id_b);

        // Perform lookups from client_a
        let ep_r1 = client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_ddb_lookup_for_tests(&id_r1)).expect("lookup relay1 succeeds");
        let ep_r2 = client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_ddb_lookup_for_tests(&id_r2)).expect("lookup relay2 succeeds");
        let ep_a = client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_ddb_lookup_for_tests(&id_a)).expect("lookup client_a succeeds");
        let ep_b = client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_ddb_lookup_for_tests(&id_b)).expect("lookup client_b succeeds");

        tracing::info!("[Test] ep_r1={}, ep_r2={}, ep_a={}, ep_b={}", ep_r1, ep_r2, ep_a, ep_b);

        // Relays: must have direct static endpoints
        assert_eq!(ep_r1.inet_socket_address(), Some(relay1_addr), "relay1 lookup should return static address");
        assert_eq!(ep_r2.inet_socket_address(), Some(relay2_addr), "relay2 lookup should return static address");

        if broken_nat {
            // Clients: should have a relay endpoint if NAT is broken
            assert!(ep_a.is_relay(), "client_a should be registered as a relay endpoint (broken_nat=true)");
            assert!(ep_b.is_relay(), "client_b should be registered as a relay endpoint (broken_nat=true)");
            // Also check that it is one of our relays
            let rid_a = ep_a.relay_id().expect("ep_a relay_id");
            let rid_b = ep_b.relay_id().expect("ep_b relay_id");
            assert!(rid_a == id_r1 || rid_a == id_r2, "client_a should use one of the two relays");
            assert!(rid_b == id_r1 || rid_b == id_r2, "client_b should use one of the two relays");
        } else {
            // Clients: should have a direct (STUN-discovered) endpoint if NAT is okay
            assert!(!ep_a.is_relay(), "client_a should be registered as a direct endpoint (broken_nat=false)");
            assert!(!ep_b.is_relay(), "client_b should be registered as a direct endpoint (broken_nat=false)");
            assert!(ep_a.inet_socket_address().is_some(), "client_a should have a public IP");
            assert!(ep_b.inet_socket_address().is_some(), "client_b should have a public IP");
        }
    }

    // Obtain B's user id and send a message from A -> B using send_message_to_id
    let b_id = client_b.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("client_b.get_my_id Some");
    let msg = json!({ "text": "hello" });
    tracing::info!("[Test] sending message to id={} msg={:?}", b_id, msg);
    let sent = client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.send_message_to_id(&b_id, msg.clone(), None)).unwrap();
    assert!(sent, "send_message_to_id should return true");
    tracing::info!("[Test] send_message_to_id returned true");

    // Wait up to 60 seconds for receipt on B
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(60) {
        if received.load(Ordering::SeqCst) { break; }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(received.load(Ordering::SeqCst), "client B did not receive the message in time");
    tracing::info!("[Test] client B received the message");

    // Validate payload shape
    {
        let guard = payload_guard.lock().expect("lock payload_guard");
        let p = guard.as_ref().expect("payload should be Some since received is true");
        tracing::info!("[Test] received payload: {}", p);
        assert_eq!(p.get("text").and_then(|v: &serde_json::Value| v.as_str()), Some("hello"));
        // Validate that cipher_suite is present and non-empty in the inbound message.
        // The engine injects the DTLS cipher suite into every received message.
        let cs = p.get("cipher_suite")
            .expect("cipher_suite field must be present in inbound message");
        let cs_str = cs.as_str().expect("cipher_suite must be a string");
        assert!(!cs_str.is_empty(), "cipher_suite must not be empty, got: {:?}", cs_str);
        tracing::info!("[Test] cipher_suite in received message: {}", cs_str);
    }

    // Validate that reverse handle lookup on incoming message worked: sender_handle should be client_a
    {
        let who = who_guard.lock().expect("lock who_guard").clone().expect("who should be captured");
        let (seen_sender_id, seen_handle) = who;
        let id_a = client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("client_a id");
        assert_eq!(seen_sender_id, id_a, "sender id should match client_a id");
        assert_eq!(seen_handle, "client_a", "sender handle should be resolved via blockchain to 'client_a'");
    }

    // Tear down
    tracing::info!("[Test] tearing down");
    relay1.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    relay2.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());
    client_b.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());
    s1.stop();
    s2.stop();
}

// Helper: wait for indexer to see the relays
fn wait_for_indexer_visible(app_id: u64, expected: &[(String, SocketAddr)], timeout: Duration) -> bool {
    let cfg = test_util::localnet_config();
    // Provide a placeholder address for read-only indexer ops
    let ops = test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone());
    let ab = AlgoBingle::new(ops, app_id, 0);
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(list) = ab.list_static_endpoints_via_indexer(app_id) {
            let mut all_match = true;
            for (exp_id, exp_addr) in expected {
                let exp_addr_str = exp_addr.to_string();
                if !list.iter().any(|(fid, faddr_str)| fid == exp_id && faddr_str == &exp_addr_str) {
                    all_match = false;
                    tracing::info!("[Test] Relay {} not yet visible via indexer with address {} (found: {:?})", exp_id, exp_addr_str, list);
                    break;
                }
            }
            if all_match && list.len() >= expected.len() {
                tracing::info!("[Test] All {} relays visible via indexer with correct addresses after {:?}", expected.len(), start.elapsed());
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(1000));
    }
    false
}

// Localnet-style integration test for send_message_to_id using two relays and two clients.
// Follows the pattern of bingle_api_endpoint_identify_via_forced_stun and extracts helpers to avoid duplication.
#[serial(send_message_to_id)]
#[cfg_attr(not(target_os = "ios"), test)]
#[ntest::timeout(1800_000)]
pub fn bingle_api_send_message_to_id_localnet() {
    run_send_message_to_id_test(false);
}

// Localnet-style integration test for send_message_to_id using two relays and two clients,
// where both clients have broken NAT and must use relays.
#[serial(send_message_to_id)]
#[cfg_attr(not(target_os = "ios"), test)]
#[ntest::timeout(1_800_000)]
pub fn bingle_api_send_message_to_id_relay_only_localnet() {
    run_send_message_to_id_test(true);
}

#[serial(send_message_to_id)]
#[cfg_attr(not(target_os = "ios"), test)]
#[ntest::timeout(1_800_000)]
pub fn bingle_api_send_message_to_id_non_root_relay_localnet() {
    test_util::init_test_logging_with_filter("info,rust_comms::dtls=info");
    if !test_util::should_run_localnet() { return; }

    let relay3_id = "3RLYTSRX54G5WOPPPV4FYWRV2QXKIC5WRPM54YKXGVLTAFGUEIG2QN4DMQ";
    let relay3_pass = "horror stuff huge crunch green marriage parent soon hamster tonight miracle company fee cup hard media shiver emotion hybrid shiver main cube lemon about obvious";
    let relay4_id = "4RLY44PVAFKYGLAZC4FQFZGRPWZZUBPEX3OBCCROJQYJ5MEOETLQY5CJLE";
    let relay4_pass = "airport there model more limb audit surprise black recipe eagle rely switch sphere debate report chapter pig hope fabric open transfer behind tent absorb deal";

    let id2 = "QASXBML72DKIJEJ5GLMEBBX33KCKW3TSJW7ETFOTLEREQCDMW5BXCLXSQU";
    let pp2 ="group avocado audit dentist baby index pipe attack enough stairs fame position column media copper athlete resource noodle forward wage middle into fitness ability dragon";
    let id3 = "YA2UAJPUJZBY4KR2B4FBM57NSA7252PJQTVKJEGB2MOISRUECW4JGE4USM";
    let pp3 ="glide crawl soda hole assault tide fault century seed tip daughter student rice swap imitate setup like card reject claim truck squeeze same able remind";

    let client3_id = id2;
    let client3_pass = pp2;
    let client4_id = id3;
    let client4_pass = pp3;
    let cfg = test_util::localnet_config();
    // Fund all addresses
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[
        test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE,
        relay3_id, relay4_id, client3_id, client4_id
    ]).expect("fund accounts");

    let creator = test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone());
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE$", 1_000_000);

    register_client_on_blockchain(relay3_id, relay3_pass, "relay3", app_id, asset_id, &creator, cfg.clone());
    register_client_on_blockchain(relay4_id, relay4_pass, "relay4", app_id, asset_id, &creator, cfg.clone());

    register_client_on_blockchain(id2, pp2, "id2", app_id, asset_id, &creator, cfg.clone());
    register_client_on_blockchain(id3, pp3, "id3", app_id, asset_id, &creator, cfg.clone());

    // Setup root relays and STUN
    let r1_port = test_util::find_unused_loopback_port();
    let r2_port = test_util::find_unused_loopback_port();
    let relay1_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r1_port);
    let relay2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r2_port);

    register_relays(app_id, asset_id, relay1_addr, relay2_addr);

    // Wait for root relays to be visible via indexer too, as the engine's initialization depends on it
    tracing::info!("[Test] Waiting for root relay ids to be visible via indexer");
    let roots = vec![
        (test_util::ADDRESS_SPEND.to_string(), relay1_addr),
        (test_util::ADDRESS_RECEIVE.to_string(), relay2_addr),
    ];
    if !wait_for_indexer_visible(app_id, &roots, Duration::from_secs(60)) {
        panic!("Root relayids  did not become visible via indexer");
    }

    tracing::info!("[Test] Starting root relays");
    let relay1 = start_root_relay("relay1", relay1_addr, test_util::PASSPHRASE_SPEND, app_id, cfg.clone());
    let relay2 = start_root_relay("relay2", relay2_addr, test_util::PASSPHRASE_RECEIVE, app_id, cfg.clone());

    tracing::info!("[Test] Starting stun servers");
    let (mut s1_full, mut s2_full, stun_list_full) = setup_stun_servers(false);
    let (mut s1_iso, mut s2_iso, _stun_list_isolated) = setup_stun_servers(true);

    // Wait for root relays to be fully ready (registered in blockchain)??
    tracing::info!("[Test] Waiting for root relays to register themselves in blockchain");
    let ab_creator = AlgoBingle::new(test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone()), app_id, 0);
    if !wait_for_relays_visible(&ab_creator, app_id, &roots, Duration::from_secs(60)) {
        panic!("Root relays did not become visible");
    }

    // Start non-root relays
    tracing::info!("[Test] Starting non-root relays");
    let relay3 = start_relay("relay3", relay3_pass, stun_list_full.clone(), app_id, cfg.clone());
    let relay4 = start_relay("relay4", relay4_pass, stun_list_full.clone(), app_id, cfg.clone());

    // Wait for non-root relays to register themselves in DDB (they should enter Registered state)
    tracing::info!("[Test] Waiting for non-root relays to register themselves in DDB");
    assert!(test_util::wait_for_registered(&relay3, Duration::from_secs(120)), "relay3 did not register");
    assert!(test_util::wait_for_registered(&relay4, Duration::from_secs(120)), "relay4 did not register");

    // Start clients
    tracing::info!("[Test] Starting clients");
    let client3 = start_client("3USE", client3_pass, stun_list_full.clone(), app_id, cfg.clone());
    let client4 = start_client("4USE", client4_pass, stun_list_full.clone(), app_id, cfg.clone());

    // Install OnMessage on client4
    let received = Arc::new(AtomicBool::new(false));
    let payload_guard: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
    let who_guard: Arc<Mutex<Option<(String, String)>>> = Arc::new(Mutex::new(None));
    setup_on_message(&client4, &received, &payload_guard, &who_guard);

    // Wait for clients to reach Registered
    tracing::info!("[Test] Waiting for clients to reach Registered state");
    assert!(test_util::wait_for_registered(&client3, Duration::from_secs(120)), "client3 did not register");
    assert!(test_util::wait_for_registered(&client4, Duration::from_secs(120)), "client4 did not register");

    // Send message from 3USE to 4USE
    tracing::info!("[Test] Sending message from 3USE to 4USE");
    let c4_id = client4.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("client4 id");
    let msg = json!({ "text": "hello from 3USE" });
    let sent = client3.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.send_message_to_id(&c4_id, msg.clone(), None)).unwrap();
    assert!(sent, "send_message_to_id should return true");

    // Wait for receipt
    tracing::info!("[Test] Waiting for receipt on 4USE");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(60) {
        if received.load(Ordering::SeqCst) { break; }
        std::thread::sleep(Duration::from_millis(200));
    }

    assert!(received.load(Ordering::SeqCst), "client 4USE did not receive the message from 3USE");

    tracing::info!("[Test] Received payload: {:?}", payload_guard.lock().expect("lock payload_guard"));

    // Tear down
    relay1.access_unsafe_for_tests(|r| r.stop());
    relay2.access_unsafe_for_tests(|r| r.stop());
    relay3.access_unsafe_for_tests(|r| r.stop());
    relay4.access_unsafe_for_tests(|r| r.stop());
    client3.access_unsafe_for_tests(|c| c.stop());
    client4.access_unsafe_for_tests(|c| c.stop());
    s1_full.stop();
    s2_full.stop();
    s1_iso.stop();
    s2_iso.stop();
}

// Localnet-style integration test: a relay sends a message to its own relay client using send_message_to_id.
#[serial(send_message_to_id)]
#[cfg_attr(not(target_os = "ios"), test)]
#[ntest::timeout(1_800_000)]
pub fn bingle_api_send_message_to_id_relay_to_relay_client_localnet() {
    test_util::init_test_logging_with_filter("info,rust_comms::dtls=info");

    if !test_util::should_run_localnet() {
        eprintln!("[skipped] Localnet required: set RUST_COMMS_RUN_LOCALNET=true and ensure local Algorand localnet and indexer are running");
        return;
    }

    let cfg = test_util::localnet_config();
    let _ = setup_localnet::ensure_localnet_accounts_funded(&cfg, &[test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE, ADDRESS_B]);

    // Fixed relay endpoints on loopback
    let r1_port = test_util::find_unused_loopback_port();
    let r2_port = test_util::find_unused_loopback_port();
    assert_ne!(r1_port, 0);
    assert_ne!(r2_port, 0);
    let relay1_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r1_port);
    let relay2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r2_port);

    tracing::info!("[Test] relay1_addr = {}", relay1_addr);
    tracing::info!("[Test] relay2_addr = {}", relay2_addr);

    // Deploy app + asset
    let creator = test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone());
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE$", 1_000_000);

    // Register relay handles on blockchain so receiver can resolve sender handle
    register_client_on_blockchain(
        test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, "relay1",
        app_id, asset_id, &creator, cfg.clone(),
    );
    register_client_on_blockchain(
        test_util::ADDRESS_RECEIVE, test_util::PASSPHRASE_RECEIVE, "relay2",
        app_id, asset_id, &creator, cfg.clone(),
    );

    register_relays(app_id, asset_id, relay1_addr, relay2_addr);

    // Start two root relays
    let relay1 = start_root_relay("relay1", relay1_addr, test_util::PASSPHRASE_SPEND, app_id, cfg.clone());
    let relay2 = start_root_relay("relay2", relay2_addr, test_util::PASSPHRASE_RECEIVE, app_id, cfg.clone());

    // Start STUN servers with broken NAT so clients must use relays
    let (mut s1, mut s2, stun_list) = setup_stun_servers(true);

    // Register client_b on blockchain so handle lookup works
    register_client_on_blockchain(
        ADDRESS_B, PASSPHRASE_B, "client_b",
        app_id, asset_id, &creator, cfg.clone(),
    );

    // Start a single client that will be forced to use a relay due to broken NAT
    let client_b = start_client("client_b", PASSPHRASE_B, stun_list.clone(), app_id, cfg.clone());

    // Wait for client to reach Registered (it should register via a relay)
    let ok_b = test_util::wait_for_registered(&client_b, Duration::from_secs(120));
    assert!(ok_b, "client B did not reach Registered state (state = {:?})",
        client_b.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_state_for_tests()));

    // Verify client B is using a relay endpoint (broken NAT)
    let id_b = client_b.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("client_b id");
    let id_r1 = relay1.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("relay1 id");
    let id_r2 = relay2.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("relay2 id");

    std::thread::sleep(Duration::from_millis(30_000));

    // Look up client_b's endpoint from relay1 to find which relay it uses
    let ep_b = relay1.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_ddb_lookup_for_tests(&id_b))
        .expect("lookup client_b succeeds");
    assert!(ep_b.is_relay(), "client_b should be registered as a relay endpoint (broken_nat=true)");
    let relay_id_for_b = ep_b.relay_id().expect("ep_b relay_id");
    assert!(relay_id_for_b == id_r1 || relay_id_for_b == id_r2, "client_b should use one of the two relays");

    // Determine which relay owns client_b and send from that relay
    let sending_relay = if relay_id_for_b == id_r1 { &relay1 } else { &relay2 };
    let relay_name = if relay_id_for_b == id_r1 { "relay1" } else { "relay2" };
    tracing::info!("[Test] client_b is using {} as its relay; sending message from that relay to client_b", relay_name);

    // Install OnMessage handler on client_b to capture delivery
    let received = Arc::new(AtomicBool::new(false));
    let payload_guard: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
    let who_guard: Arc<Mutex<Option<(String, String)>>> = Arc::new(Mutex::new(None));
    setup_on_message(&client_b, &received, &payload_guard, &who_guard);

    // Relay sends message to its own relay client using send_message_to_id.
    let msg = json!({ "text": "hello from relay" });
    let sent = sending_relay.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.send_message_to_id(&id_b, msg.clone(), None)).unwrap();
    assert!(sent, "send_message_to_id from relay to relay client should return true");

    // Wait up to 60 seconds for receipt on client_b
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(60) {
        if received.load(Ordering::SeqCst) { break; }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(received.load(Ordering::SeqCst), "client B did not receive the message from its relay in time");

    // Validate payload
    {
        let guard = payload_guard.lock().expect("lock payload_guard");
        let p = guard.as_ref().expect("payload should be Some since received is true");
        tracing::info!("[Test] received payload: {}", p);
        assert_eq!(p.get("text").and_then(|v: &serde_json::Value| v.as_str()), Some("hello from relay"));
    }

    // Tear down
    relay1.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    relay2.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    client_b.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());
    s1.stop();
    s2.stop();
}

/// Helper: wait for the `received` flag to be set within the given timeout.
/// Returns true if the message was received in time.
fn wait_for_message(received: &Arc<AtomicBool>, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if received.load(Ordering::SeqCst) { return true; }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

/// Helper: reset message-receive state so we can reuse the same flags for a new message.
fn reset_message_state(
    received: &Arc<AtomicBool>,
    payload_guard: &Arc<Mutex<Option<serde_json::Value>>>,
    who_guard: &Arc<Mutex<Option<(String, String)>>>,
) {
    received.store(false, Ordering::SeqCst);
    if let Ok(mut g) = payload_guard.lock() { *g = None; }
    if let Ok(mut g) = who_guard.lock() { *g = None; }
}

/// Exercises the DTLS connection reuse bug after a client restart.
///
/// Steps:
/// 1. Create, register and start two root relays and STUN servers.
/// 2. Start clients A and B, register A on blockchain, wait for Registered.
/// 3. Send a message from A → B and validate receipt.
/// 4. Stop client A.
/// 5. Start new client A2 with the same identity and address:port as A.
/// 6. Send a message from A2 → B and validate receipt on B.
/// 7. Send a message from B → A2 and validate receipt on A2.
///
/// This test validates that the DTLS connection is correctly reused
/// after a client restart, once the old connection state has been cleaned up.
#[serial(send_message_to_id)]
#[cfg_attr(not(target_os = "ios"), test)]
#[ntest::timeout(1_800_000)]
pub fn bingle_api_send_message_after_client_restart_localnet() {
    test_util::init_test_logging_with_filter("info,rust_comms::dtls=info");

    if !test_util::should_run_localnet() {
        eprintln!("[skipped] Localnet required: set RUST_COMMS_RUN_LOCALNET=true and ensure local Algorand localnet and indexer are running");
        return;
    }

    // ── Infrastructure: relays, STUN, blockchain ───────────────────────
    let r1_port = test_util::find_unused_loopback_port();
    let r2_port = test_util::find_unused_loopback_port();
    assert_ne!(r1_port, 0);
    assert_ne!(r2_port, 0);
    let relay1_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r1_port);
    let relay2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r2_port);
    tracing::info!("relay_1_addr: {}, relay_2_addr: {}", relay1_addr, relay2_addr);

    let cfg = test_util::localnet_config();
    let creator = test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone());
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE$", 1_000_000);

    register_relays(app_id, asset_id, relay1_addr, relay2_addr);
    let roots = vec![
        (test_util::ADDRESS_SPEND.to_string(), relay1_addr),
        (test_util::ADDRESS_RECEIVE.to_string(), relay2_addr),
    ];
    let ab_creator = AlgoBingle::new(creator.clone(), app_id, 0);
    if !wait_for_relays_visible(&ab_creator, app_id, &roots, Duration::from_secs(60)) {
        panic!("Root relays did not become visible");
    }

    let relay1 = start_root_relay("relay1", relay1_addr, test_util::PASSPHRASE_SPEND, app_id, cfg.clone());
    let relay2 = start_root_relay("relay2", relay2_addr, test_util::PASSPHRASE_RECEIVE, app_id, cfg.clone());

    let (mut s1, mut s2, stun_list) = setup_stun_servers(false);

    // ── Clients A and B ────────────────────────────────────────────────
    let client_a = start_client("client_a", test_util::PASSPHRASE_10MIL, stun_list.clone(), app_id, cfg.clone());
    let client_b = start_client("client_b", PASSPHRASE_B, stun_list.clone(), app_id, cfg.clone());

    // Register client A on blockchain for reverse handle lookup
    register_client_on_blockchain(
        test_util::ADDRESS_10MIL, test_util::PASSPHRASE_10MIL, "client_a",
        app_id, asset_id, &creator, cfg.clone(),
    );
    register_client_on_blockchain(
        &*client_b.get_my_id().expect("Client B must have id"), PASSPHRASE_B, "client_b",
        app_id, asset_id, &creator, cfg.clone(),
    );

    // Wait for both clients to reach Registered
    let ok_a = test_util::wait_for_registered(&client_a, Duration::from_secs(360));
    let ok_b = test_util::wait_for_registered(&client_b, Duration::from_secs(360));
    assert!(ok_a, "client A did not reach Registered state");
    assert!(ok_b, "client B did not reach Registered state");

    // ── Phase 1: A → B message ─────────────────────────────────────────
    let b_id = client_b.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("client_b id");

    let received_b = Arc::new(AtomicBool::new(false));
    let payload_b: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
    let who_b: Arc<Mutex<Option<(String, String)>>> = Arc::new(Mutex::new(None));
    setup_on_message(&client_b, &received_b, &payload_b, &who_b);

    tracing::info!("[Test] Phase 1: sending message from A → B");
    let msg1 = json!({ "text": "hello from A" });
    let sent1 = client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.send_message_to_id(&b_id, msg1.clone(), None)).unwrap();
    assert!(sent1, "send_message_to_id A→B should return true");

    assert!(wait_for_message(&received_b, Duration::from_secs(60)), "client B did not receive message from A in time");
    {
        let guard = payload_b.lock().expect("lock payload_b");
        let p = guard.as_ref().expect("payload should be Some");
        assert_eq!(p.get("text").and_then(|v| v.as_str()), Some("hello from A"));
    }
    tracing::info!("[Test] Phase 1 complete: A → B message received");

    // ── Stop client A and record its address ───────────────────────────
    let a_addr = client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_local_bind_addr_for_tests())
        .expect("client A should have a local bind address");
    tracing::info!("[Test] Stopping client A (addr={})", a_addr);
    client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());

    // Brief pause for the old connection state to settle
    std::thread::sleep(Duration::from_secs(2));

    // ── Start client A2 with same identity and address:port ────────────
    tracing::info!("[Test] Starting client A2 with same id at {}", a_addr);
    let client_a2 = start_client_at_addr("client_a", test_util::PASSPHRASE_10MIL, a_addr, stun_list.clone(), app_id, cfg.clone());

    let ok_a2 = test_util::wait_for_registered(&client_a2, Duration::from_secs(120));
    assert!(ok_a2, "client A2 did not reach Registered state");
    tracing::info!("[Test] client A2 reached Registered state");

    // ── Phase 2: A2 → B message ────────────────────────────────────────
    reset_message_state(&received_b, &payload_b, &who_b);

    tracing::info!("[Test] Phase 2: sending message from A2 → B");
    let msg2 = json!({ "text": "hello from A2" });
    let sent2 = client_a2.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.send_message_to_id(&b_id, msg2.clone(), None)).unwrap();
    assert!(sent2, "send_message_to_id A2→B should return true");

    assert!(wait_for_message(&received_b, Duration::from_secs(60)), "client B did not receive message from A2 in time");
    {
        let guard = payload_b.lock().expect("lock payload_b");
        let p = guard.as_ref().expect("payload should be Some");
        assert_eq!(p.get("text").and_then(|v| v.as_str()), Some("hello from A2"));
    }
    tracing::info!("[Test] Phase 2 complete: A2 → B message received");

    // ── Phase 3: B → A2 message ────────────────────────────────────────
    let a2_id = client_a2.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("client_a2 id");

    let received_a2 = Arc::new(AtomicBool::new(false));
    let payload_a2: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
    let who_a2: Arc<Mutex<Option<(String, String)>>> = Arc::new(Mutex::new(None));
    setup_on_message(&client_a2, &received_a2, &payload_a2, &who_a2);

    tracing::info!("[Test] Phase 3: sending message from B → A2");
    let msg3 = json!({ "text": "hello from B" });
    let sent3 = client_b.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.send_message_to_id(&a2_id, msg3.clone(), None)).unwrap();
    assert!(sent3, "send_message_to_id B→A2 should return true");

    assert!(wait_for_message(&received_a2, Duration::from_secs(60)), "client A2 did not receive message from B in time");
    {
        let guard = payload_a2.lock().expect("lock payload_a2");
        let p = guard.as_ref().expect("payload should be Some");
        assert_eq!(p.get("text").and_then(|v| v.as_str()), Some("hello from B"));
    }
    tracing::info!("[Test] Phase 3 complete: B → A2 message received");

    // ── Tear down ──────────────────────────────────────────────────────
    relay1.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    relay2.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    client_a2.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());
    client_b.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());
    s1.stop();
    s2.stop();
}

// Localnet-style integration test: relay1 sends a message to client_a who is registered with relay2.
// This tests cross-relay delivery which is expected to fail currently.
#[serial(send_message_to_id)]
#[cfg_attr(not(target_os = "ios"), test)]
#[ntest::timeout(1_800_000)]
pub fn bingle_api_send_message_to_id_relay1_to_client_on_relay2_localnet() {
    test_util::init_test_logging_with_filter("info,rust_comms::dtls=info");

    if !test_util::should_run_localnet() {
        eprintln!("[skipped] Localnet required: set RUST_COMMS_RUN_LOCALNET=true and ensure local Algorand localnet and indexer are running");
        return;
    }

    let cfg = test_util::localnet_config();
    let _ = setup_localnet::ensure_localnet_accounts_funded(&cfg, &[test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE, ADDRESS_B]);

    // Fixed relay endpoints on loopback
    let r1_port = test_util::find_unused_loopback_port();
    let r2_port = test_util::find_unused_loopback_port();
    assert_ne!(r1_port, 0);
    assert_ne!(r2_port, 0);
    let relay1_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r1_port);
    let relay2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r2_port);

    tracing::info!("[Test] relay1_addr = {}, relay2_addr = {}", relay1_addr, relay2_addr);

    // Deploy app + asset
    let creator = test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone());
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE$", 1_000_000);

    // Register relay2 ONLY initially
    register_client_on_blockchain(
        test_util::ADDRESS_RECEIVE, test_util::PASSPHRASE_RECEIVE, "relay2",
        app_id, asset_id, &creator, cfg.clone(),
    );
    let ab_creator = AlgoBingle::new(creator.clone(), app_id, 0);
    ab_creator.set_allow_static(app_id, test_util::ADDRESS_RECEIVE, true).expect("set_allow_static r2");

    let ops_relay2 = test_util::ops_from_mnemonic(test_util::ADDRESS_RECEIVE, test_util::PASSPHRASE_RECEIVE, cfg.clone());
    let ab_r2 = AlgoBingle::new(ops_relay2.clone(), app_id, 0);
    ab_r2.register_endpoint(app_id, &relay2_addr.to_string()).expect("register_endpoint r2");

    // Start relay2
    let relay2 = start_root_relay("relay2", relay2_addr, test_util::PASSPHRASE_RECEIVE, app_id, cfg.clone());

    // Start STUN servers with broken NAT so client_a must use relay
    let (mut s1, mut s2, stun_list) = setup_stun_servers(true);

    // Register client_a
    let passphrase_a = "lift all minute first hair appear panel unfold pony property also dinosaur start robot board erupt tent pink essence stem protect ugly orphan absent dust";
    let ops_a_tmp = rust_comms::algo_ops::AlgoOps::new(Some(passphrase_a.to_string()), None, Some(cfg.clone()));
    let address_a = ops_a_tmp.address.as_deref().expect("derive client_a address from passphrase");
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[address_a]).expect("fund client_a");
    register_client_on_blockchain(
        address_a, passphrase_a, "client_a",
        app_id, asset_id, &creator, cfg.clone(),
    );

    // Start client_a - it should find only relay2 on blockchain
    let client_a = start_client("client_a", passphrase_a, stun_list.clone(), app_id, cfg.clone());

    // Wait for client_a to reach Registered
    let ok_a = test_util::wait_for_registered(&client_a, Duration::from_secs(120));
    assert!(ok_a, "client A did not reach Registered state");

    tracing::info!("[Test] client_a reached Registered state, endpoint={}", client_a.access_unsafe_for_tests(|c| c.engine_last_public_addr_for_tests()).expect("client_a endpoint"));

    // Now register and start relay1
    register_client_on_blockchain(
        test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, "relay1",
        app_id, asset_id, &creator, cfg.clone(),
    );
    ab_creator.set_allow_static(app_id, test_util::ADDRESS_SPEND, true).expect("set_allow_static r1");

    let relay1 = start_root_relay("relay1", relay1_addr, test_util::PASSPHRASE_SPEND, app_id, cfg.clone());

    // Verify client_a is using relay2
    let id_a = client_a.access_unsafe_for_tests(|c| c.get_my_id()).expect("client_a id");
    let id_r2 = relay2.access_unsafe_for_tests(|c| c.get_my_id()).expect("relay2 id");

    // Give it a bit of time for DDB to propagate
    std::thread::sleep(Duration::from_secs(10));

    // Check DDB lookup for client_a from relay1
    let ep_a = relay1.access_unsafe_for_tests(|c| c.engine_ddb_lookup_for_tests(&id_a))
        .expect("lookup client_a succeeds");
    assert!(ep_a.is_relay(), "client_a should be registered as a relay endpoint");
    assert_eq!(ep_a.relay_id(), Some(id_r2.as_str()), "client_a should be using relay2");

    // Install OnMessage handler on client_a
    let received = Arc::new(AtomicBool::new(false));
    let payload_guard = Arc::new(Mutex::new(None));
    let who_guard = Arc::new(Mutex::new(None));
    setup_on_message(&client_a, &received, &payload_guard, &who_guard);

    // relay1 sends message to client_a (id)
    tracing::info!("[Test] relay1 sending message to client_a (id={}) who is on relay2 (id={})", id_a, id_r2);
    let msg = json!({ "text": "hello from relay1 to client_a via relay2" });
    let sent = relay1.access_unsafe_for_tests(|c| c.send_message_to_id(&id_a, msg.clone(), None)).unwrap();
    assert!(sent, "send_message_to_id from relay1 should return true");

    // Wait for delivery to client_a - expected to fail
    let ok = wait_for_message(&received, Duration::from_secs(60));
    assert!(ok, "client A did not receive the message from relay1 via relay2");

    // Cleanup
    relay1.access_unsafe_for_tests(|r| r.stop());
    relay2.access_unsafe_for_tests(|r| r.stop());
    client_a.access_unsafe_for_tests(|c| c.stop());
    s1.stop();
    s2.stop();
}

