use rust_comms::api::bingle_api::{BingleApi, OnMessageHandler, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::blockchain::algo_bingle::AlgoBingle;
use rust_comms::engine::BingleAccessUnsafeForTests;
use rust_comms::engine::EngineState;
use rust_comms::stun::{SimpleStunServer, SimpleStunStartOptions};
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};
use std::time::{Duration, Instant};

use crate::setup_localnet;
use crate::util::test_util;

// Helper: start a relay node at a fixed address
fn start_root_relay(name: &str, addr: SocketAddr, passphrase: &str, app_id: u64, cfg: rust_comms::blockchain::algo_ops::AlgoChainConfig) -> Arc<BingleApiImpl> {
    log::info!("[Test] start_root_relay name={} addr={} app_id={}", name, addr, app_id);
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
    };
    let api = BingleApiImpl::new(&opts);
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts)).expect("relay start");
    log::info!("[Test] root relay {} started", name);
    api
}

// Helper: start a relay node using STUN discovery (non-root)
fn start_relay(name: &str, passphrase: &str, stun_list: Vec<SocketAddr>, app_id: u64, cfg: rust_comms::blockchain::algo_ops::AlgoChainConfig) -> Arc<BingleApiImpl> {
    log::info!("[Test] start_relay name={} stun_list={:?} app_id={}", name, stun_list, app_id);
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
    };
    let api = BingleApiImpl::new(&opts);
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts)).expect("relay start");
    log::info!("[Test] non-root relay {} started", name);
    api
}

// Helper: start a client node with given STUN list
fn start_client(name: &str, passphrase: &str, stun_list: Vec<SocketAddr>, app_id: u64, cfg: rust_comms::blockchain::algo_ops::AlgoChainConfig) -> Arc<BingleApiImpl> {
    log::info!("[Test] start_client name={} stun_list={:?} app_id={}", name, stun_list, app_id);
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
    };
    let api = BingleApiImpl::new(&opts);
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts)).expect("client start");
    log::info!("[Test] client {} started", name);
    api
}

fn wait_for_registered(api: &Arc<BingleApiImpl>, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(st) = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.engine_state_for_tests()) {
            if st == EngineState::Registered { return true; }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

fn deploy_bingle_app() -> u64 {
    let cfg = test_util::localnet_config();
    // Ensure relay accounts are funded
    // id_b is in same segment as id_a
    let id_b = "P577OS2FPV7COU3Y43PCTS2IIZ5HAXHBZRHINAATVA5ECCEYKFSEVIYTHE";

    setup_localnet::ensure_localnet_accounts_funded(&cfg,
                                                    &[test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE , id_b])
        .expect("Failed to fund localnet accounts");

    // Build AlgoOps for two relay accounts and one creator (use SPEND as creator)
    let ops_creator = test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone());

    test_util::deploy_bingle_app(&ops_creator)
}

// Helper: wait for given duration and return true if both relays are visible via discovery
fn wait_for_relays_visible(ab: &AlgoBingle, app_id: u64, accounts: &[String], timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(found) = ab.discover_root_relays(app_id, accounts) {
            if found.len() == accounts.len() {
                log::info!("[Test] All {} relays visible via discover_root_relays after {:?}", accounts.len(), start.elapsed());
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(1000));
    }
    false
}

fn register_relays(app_id: u64, relay1_addr: SocketAddr, relay2_addr: SocketAddr) {
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

    // Register endpoints for both relays
    ab_r1.register_endpoint(app_id, &relay1_addr.to_string()).expect("register_endpoint r1");
    ab_r2.register_endpoint(app_id, &relay2_addr.to_string()).expect("register_endpoint r2");

    // Wait for discovery to see them (using discover_root_relays as requested)
    log::info!("[Test] Waiting for relays to be visible via discover_root_relays...");
    let accounts = vec![test_util::ADDRESS_SPEND.to_string(), test_util::ADDRESS_RECEIVE.to_string()];
    if !wait_for_relays_visible(&ab_creator, app_id, &accounts, Duration::from_secs(60)) {
        panic!("Relays did not become visible via discover_root_relays within 60s");
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
        log::info!("[Test][{} on_message] sender={} handle={} msg={}", name, sender, sender_handle, message);
        if let Ok(mut g) = payload_store.lock() { *g = Some(message); }
        if let Ok(mut who) = who_store.lock() { *who = Some((sender.clone(), sender_handle.clone())); }
        received_flag.store(true, Ordering::SeqCst);
    });
    api.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.set_on_message(Some(on_message)));
}

fn run_send_message_to_id_test(broken_nat: bool) {
    test_util::init_test_logging();

    // This test requires a running local Algorand localnet + indexer.
    if !test_util::should_run_localnet() {
        eprintln!("[skipped] Localnet required: set RUST_COMMS_RUN_LOCALNET=true and ensure local Algorand localnet and indexer are running");
        return;
    }

    // Fixed relay endpoints on loopback
    let r1_port = test_util::find_unused_loopback_port();
    let r2_port = test_util::find_unused_loopback_port();
    assert_ne!(r1_port, 0);
    assert_ne!(r2_port, 0);
    let relay1_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r1_port);
    let relay2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r2_port);

    log::info!("[Test] relay1_addr = {}", relay1_addr);
    log::info!("[Test] relay2_addr = {}", relay2_addr);

    // Deploy app + asset so we can register client handles on-chain for reverse lookups
    let cfg = test_util::localnet_config();
    let creator = test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone());
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE$", 1_000_000);

    register_relays(app_id, relay1_addr, relay2_addr);

    // Start two relays
    let relay1 = start_root_relay("relay1", relay1_addr, test_util::PASSPHRASE_SPEND, app_id, cfg.clone());
    let relay2 = start_root_relay("relay2", relay2_addr, test_util::PASSPHRASE_RECEIVE, app_id, cfg.clone());

    // Start two local STUN servers
    let (mut s1, mut s2, stun_list) = setup_stun_servers(broken_nat);

    // Start two clients A and B; B will receive
    let passphrase_b = "lift all minute first hair appear panel unfold pony property also dinosaur start robot board erupt tent pink essence stem protect ugly orphan absent dust";
    let client_a = start_client("client_a", test_util::PASSPHRASE_10MIL, stun_list.clone(), app_id, cfg.clone());
    let client_b = start_client("client_b", passphrase_b, stun_list.clone(), app_id, cfg.clone());

    // Before sending: ensure client A has its handle registered on-chain so reverse lookup by id succeeds.
    {
        use rust_comms::blockchain::algo_bingle::AlgoBingle;
        let ops_a = test_util::ops_from_mnemonic(test_util::ADDRESS_10MIL, test_util::PASSPHRASE_10MIL, cfg.clone());
        // Opt-in A to the asset and app, and fund A with a few ASA units from creator
        let _ = ops_a.opt_in_to_asset(asset_id).expect("client_a opt-in ASA");
        let _ = ops_a.opt_in_app(app_id).expect("client_a opt-in app");
        let _ = creator.send_asset(asset_id, 10, test_util::ADDRESS_10MIL).expect("fund client_a with ASA");

        // Perform on-chain handle registration for client_a
        let ab_a = AlgoBingle::new(ops_a.clone(), app_id, asset_id);
        let _ = ab_a.register(app_id, asset_id, "client_a", 1).expect("register handle for client_a");

        // Wait until local state for client A reflects the Handle key to avoid race conditions
        let start = Instant::now();
        let timeout = Duration::from_secs(30);
        let mut ok = false;
        while start.elapsed() < timeout {
            if let Ok(Some(entries)) = ops_a.local_state_for_account(app_id, test_util::ADDRESS_10MIL) {
                if entries.iter().any(|(k, v)| k == "Handle" && v == "client_a") { ok = true; break; }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        assert!(ok, "client_a Handle not visible in local state within timeout");
    }

    // Install OnMessage handler for client B to capture delivery and who sent it
    let received = Arc::new(AtomicBool::new(false));
    let payload_guard: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
    let who_guard: Arc<Mutex<Option<(String, String)>>> = Arc::new(Mutex::new(None));
    setup_on_message(&client_b, &received, &payload_guard, &who_guard);

    // Wait for both clients to reach Registered
    let ok_a = wait_for_registered(&client_a, Duration::from_secs(120));
    let ok_b = wait_for_registered(&client_b, Duration::from_secs(120));
    assert!(ok_a, "client A did not reach Registered state (state = {:?})", client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_state_for_tests()));
    assert!(ok_b, "client B did not reach Registered state (state = {:?})", client_b.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_state_for_tests()));

    // Validate DDB lookups: use DdbClientImpl::lookup (via API) to check registered endpoints
    {
        let id_r1 = relay1.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("relay1 id");
        let id_r2 = relay2.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("relay2 id");
        let id_a = client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("client_a id");
        let id_b = client_b.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("client_b id");

        log::info!("[Test] Validating DDB lookups: id_r1={}, id_r2={}, id_a={}, id_b={}", id_r1, id_r2, id_a, id_b);

        // Perform lookups from client_a
        let ep_r1 = client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_ddb_lookup_for_tests(&id_r1)).expect("lookup relay1 succeeds");
        let ep_r2 = client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_ddb_lookup_for_tests(&id_r2)).expect("lookup relay2 succeeds");
        let ep_a = client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_ddb_lookup_for_tests(&id_a)).expect("lookup client_a succeeds");
        let ep_b = client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_ddb_lookup_for_tests(&id_b)).expect("lookup client_b succeeds");

        log::info!("[Test] ep_r1={}, ep_r2={}, ep_a={}, ep_b={}", ep_r1, ep_r2, ep_a, ep_b);

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
    let sent = client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.send_message_to_id(&b_id, msg.clone(), None));
    assert!(sent, "send_message_to_id should return true");

    // Wait up to 60 seconds for receipt on B
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(60) {
        if received.load(Ordering::SeqCst) { break; }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(received.load(Ordering::SeqCst), "client B did not receive the message in time");

    // Validate payload shape
    {
        let guard = payload_guard.lock().expect("lock payload_guard");
        let p = guard.as_ref().expect("payload should be Some since received is true");
        log::info!("[Test] received payload: {}", p);
        assert_eq!(p.get("text").and_then(|v: &serde_json::Value| v.as_str()), Some("hello"));
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
    relay1.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    relay2.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    client_a.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());
    client_b.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());
    s1.stop();
    s2.stop();
}

// Helper: wait for indexer to see the relays
fn wait_for_indexer_visible(app_id: u64, accounts: &[String], timeout: Duration) -> bool {
    let cfg = test_util::localnet_config();
    // Provide a placeholder address for read-only indexer ops
    let ops = test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone());
    let ab = AlgoBingle::new(ops, app_id, 0);
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(list) = ab.list_static_endpoints_via_indexer(app_id) {
            let found_count = list.iter().filter(|(addr, _)| accounts.contains(addr)).count();
            if found_count == accounts.len() {
                log::info!("[Test] All {} relays visible via indexer after {:?}", accounts.len(), start.elapsed());
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(1000));
    }
    false
}

// Localnet-style integration test for send_message_to_id using two relays and two clients.
// Follows the pattern of bingle_api_endpoint_identify_via_forced_stun and extracts helpers to avoid duplication.
#[cfg_attr(not(target_os = "ios"), test)]
#[ntest::timeout(180_000)]
#[ignore]
pub fn bingle_api_send_message_to_id_localnet() {
    run_send_message_to_id_test(false);
}

// Localnet-style integration test for send_message_to_id using two relays and two clients,
// where both clients have broken NAT and must use relays.
#[cfg_attr(not(target_os = "ios"), test)]
#[ntest::timeout(180_000)]
#[ignore]
pub fn bingle_api_send_message_to_id_relay_only_localnet() {
    run_send_message_to_id_test(true);
}

#[cfg_attr(not(target_os = "ios"), test)]
#[ntest::timeout(300_000)]
#[ignore]
pub fn bingle_api_send_message_to_id_non_root_relay_localnet() {
    test_util::init_test_logging();
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

    let ops_id2 = test_util::ops_from_mnemonic(id2, pp2, cfg.clone());
    ops_id2.opt_in_app(app_id).expect("id2 opt-in app");
    ops_id2.opt_in_to_asset(asset_id).expect("id2 opt-in asset");
    let _ = creator.send_asset(asset_id, 10, id2).expect("fund id2 with ASA");
    let ab_id2 = AlgoBingle::new(ops_id2, app_id, asset_id);
    ab_id2.register(app_id, asset_id, "id2", 1).expect("register id2");
    let ops_id3 = test_util::ops_from_mnemonic(id3, pp3, cfg.clone());
    ops_id3.opt_in_app(app_id).expect("id3 opt-in app");
    ops_id3.opt_in_to_asset(asset_id).expect("id3 opt-in asset");
    let _ = creator.send_asset(asset_id, 10, id3).expect("fund id3 with ASA");
    let ab_id3 = AlgoBingle::new(ops_id3, app_id, asset_id);
    ab_id3.register(app_id, asset_id, "id3", 1).expect("register id3");

    // Setup root relays and STUN
    let r1_port = test_util::find_unused_loopback_port();
    let r2_port = test_util::find_unused_loopback_port();
    let relay1_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r1_port);
    let relay2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r2_port);

    register_relays(app_id, relay1_addr, relay2_addr);

    // Wait for root relays to be visible via indexer too, as the engine's initialization depends on it
    log::info!("[Test] Waiting for root relay ids to be visible via indexer");
    let roots = vec![test_util::ADDRESS_SPEND.to_string(), test_util::ADDRESS_RECEIVE.to_string()];
    if !wait_for_indexer_visible(app_id, &roots, Duration::from_secs(60)) {
        panic!("Root relayids  did not become visible via indexer");
    }

    log::info!("[Test] Starting root relays");
    let relay1 = start_root_relay("relay1", relay1_addr, test_util::PASSPHRASE_SPEND, app_id, cfg.clone());
    let relay2 = start_root_relay("relay2", relay2_addr, test_util::PASSPHRASE_RECEIVE, app_id, cfg.clone());

    log::info!("[Test] Starting stun servers");
    let (mut s1_full, mut s2_full, stun_list_full) = setup_stun_servers(false);
    let (mut s1_iso, mut s2_iso, _stun_list_isolated) = setup_stun_servers(true);

    // Wait for root relays to be fully ready (registered in blockchain)??
    log::info!("[Test] Waiting for root relays to register themselves in blockchain");
    let ab_creator = AlgoBingle::new(test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone()), app_id, 0);
    if !wait_for_relays_visible(&ab_creator, app_id, &roots, Duration::from_secs(60)) {
        panic!("Root relays did not become visible");
    }

    // Start non-root relays
    log::info!("[Test] Starting non-root relays");
    let relay3 = start_relay("relay3", relay3_pass, stun_list_full.clone(), app_id, cfg.clone());
    let relay4 = start_relay("relay4", relay4_pass, stun_list_full.clone(), app_id, cfg.clone());

    // Wait for non-root relays to register themselves in DDB (they should enter Registered state)
    log::info!("[Test] Waiting for non-root relays to register themselves in DDB");
    assert!(wait_for_registered(&relay3, Duration::from_secs(120)), "relay3 did not register");
    assert!(wait_for_registered(&relay4, Duration::from_secs(120)), "relay4 did not register");

    // Start clients
    log::info!("[Test] Starting clients");
    let client3 = start_client("3USE", client3_pass, stun_list_full.clone(), app_id, cfg.clone());
    let client4 = start_client("4USE", client4_pass, stun_list_full.clone(), app_id, cfg.clone());

    // Install OnMessage on client4
    let received = Arc::new(AtomicBool::new(false));
    let payload_guard: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
    let who_guard: Arc<Mutex<Option<(String, String)>>> = Arc::new(Mutex::new(None));
    setup_on_message(&client4, &received, &payload_guard, &who_guard);

    // Wait for clients to reach Registered
    log::info!("[Test] Waiting for clients to reach Registered state");
    assert!(wait_for_registered(&client3, Duration::from_secs(120)), "client3 did not register");
    assert!(wait_for_registered(&client4, Duration::from_secs(120)), "client4 did not register");

    // Send message from 3USE to 4USE
    log::info!("[Test] Sending message from 3USE to 4USE");
    let c4_id = client4.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("client4 id");
    let msg = json!({ "text": "hello from 3USE" });
    let sent = client3.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.send_message_to_id(&c4_id, msg.clone(), None));
    assert!(sent, "send_message_to_id should return true");

    // Wait for receipt
    log::info!("[Test] Waiting for receipt on 4USE");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(60) {
        if received.load(Ordering::SeqCst) { break; }
        std::thread::sleep(Duration::from_millis(200));
    }

    assert!(received.load(Ordering::SeqCst), "client 4USE did not receive the message from 3USE");

    log::info!("[Test] Received payload: {:?}", payload_guard.lock().expect("lock payload_guard"));

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

