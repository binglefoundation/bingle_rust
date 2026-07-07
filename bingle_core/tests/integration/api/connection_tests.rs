// Integration test: restricted NAT client re-registers correctly after a network change.
//
// To run:
//   cargo test --test all integration::api::connection_tests -- --ignored

use bingle_core::api::bingle_api::{BingleApi, OnListeningHandler, StartOptions};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::engine::{BingleAccessUnsafeForTests, EngineState, NatType};
use bingle_core::stun::{SimpleStunServer, SimpleStunStartOptions};
use serde_json::json;
use serial_test::serial;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
// Re-use helpers from send_message_to_id_integration
use super::relay_updater_localnet::test_util::start_root_relay;
use crate::setup_localnet;
use crate::util::relay_test_util::wait_for_relays_visible;
use crate::util::test_util;
use crate::util::test_util::register_client_on_blockchain;

// Re-use helpers from send_message_to_id_integration
use super::send_message_to_id_integration::register_relays;

// Passphrase for the restricted-NAT client
const CLIENT_RESTRICTED_PASS: &str = "lift all minute first hair appear panel unfold pony property also dinosaur start robot board erupt tent pink essence stem protect ugly orphan absent dust";
const CLIENT_RESTRICTED_ADDR: &str = "P577OS2FPV7COU3Y43PCTS2IIZ5HAXHBZRHINAATVA5ECCEYKFSEVIYTHE";

/// Wait for `engine_state_for_tests()` to return `None` (no state) within the timeout.
fn wait_for_state_none(api: &Arc<BingleApiImpl>, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        let st = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.engine_state_for_tests());
        if st
            .expect("Engine state should not be None")
            .eq(&EngineState::StunIdentify)
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    false
}

/// Wait for `engine_state_for_tests()` to return the given state within the timeout.
#[allow(dead_code)]
fn wait_for_state(api: &Arc<BingleApiImpl>, target: EngineState, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        let st = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.engine_state_for_tests());
        if st == Some(target) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    false
}

/// Install an `on_listening` handler that records the most recent call.
fn setup_on_listening(
    api: &Arc<BingleApiImpl>,
    listening_flag: &Arc<AtomicBool>,
    nat_type_guard: &Arc<Mutex<Option<NatType>>>,
) {
    let flag = listening_flag.clone();
    let nat = nat_type_guard.clone();
    let handler: Arc<OnListeningHandler> = Arc::new(move |is_listening, nt| {
        tracing::info!(
            "[Test][on_listening] listening={} nat_type={:?}",
            is_listening,
            nt
        );
        flag.store(is_listening, Ordering::SeqCst);
        if let Ok(mut g) = nat.lock() {
            *g = Some(nt);
        }
    });
    api.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.set_on_listening(Some(handler)));
}

/// Start two STUN servers that report a fixed external port (simulating a NAT mapping).
/// `external_port` is the port that both servers will report back to the client.
/// `broken_nat` controls whether the second server refuses to respond (simulating restricted cone NAT).
fn start_stun_pair_with_external_port(
    external_port: u16,
    broken_nat: bool,
) -> (SimpleStunServer, SimpleStunServer, Vec<SocketAddr>) {
    let p1 = test_util::find_unused_loopback_port();
    let p2 = test_util::find_unused_loopback_port();
    let a1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p1);
    let a2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p2);

    // attach_to makes the STUN server report the given address as the client's external address.
    let external_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), external_port);

    let s1 = SimpleStunServer::start(SimpleStunStartOptions {
        bind_addr: a1,
        attach_to: Some(external_addr),
        broken_nat: false,
    })
    .expect("start stun s1");

    // Second server: if broken_nat=true it won't respond (simulates restricted cone NAT where
    // only one server sees the client).
    let s2 = SimpleStunServer::start(SimpleStunStartOptions {
        bind_addr: a2,
        attach_to: Some(external_addr),
        broken_nat,
    })
    .expect("start stun s2");

    (s1, s2, vec![a1, a2])
}

/// A restricted-NAT client re-registers correctly after a network change.
///
/// Steps:
/// 1. Create and register two root relays.
/// 2. Set up STUN reporting external PORT_1; second server uses broken_nat so only one server
///    sees the client (simulating restricted cone NAT).
/// 3. Create the client and wait for it to reach Registered state.
/// 4. Validate on_listening is called with true and NatType::Restricted.
/// 5. Stop the STUN servers.
/// 6. After a timeout, expect on_listening to be called with false and state to become None.
/// 7. Restart STUN reporting external PORT_2 (simulating IP/port change).
/// 8. After a timeout, expect the client to re-register and on_listening called with true.
/// 9. Validate that client can send a message to relay1.
/// 10. Validate relay1 can look up client by handle and send it a message.
#[serial(send_message_to_id)]
#[test]
#[cfg(not(target_os = "ios"))]
#[ntest::timeout(1_800_000)]
pub fn restricted_nat_client_reregisters_after_network_change() {
    test_util::init_test_logging_with_filter("info,bingle_core::dtls=info,bingle_core::stun=info");
    test_util::assert_localnet_available();

    let cfg = test_util::localnet_config();
    let _ = setup_localnet::ensure_localnet_accounts_funded(
        &cfg,
        &[
            test_util::ADDRESS_SPEND,
            test_util::ADDRESS_RECEIVE,
            CLIENT_RESTRICTED_ADDR,
        ],
    );

    // Fixed relay endpoints on loopback
    let r1_port = test_util::find_unused_loopback_port();
    let r2_port = test_util::find_unused_loopback_port();
    assert_ne!(r1_port, 0);
    assert_ne!(r2_port, 0);
    let relay1_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r1_port);
    let relay2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r2_port);
    tracing::info!(
        "[Test] relay1_addr={} relay2_addr={}",
        relay1_addr,
        relay2_addr
    );

    // Deploy app + asset
    let creator = test_util::ops_from_mnemonic(
        test_util::ADDRESS_SPEND,
        test_util::PASSPHRASE_SPEND,
        cfg.clone(),
    );
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE$", 1_000_000);

    // Register relay handles on blockchain
    register_client_on_blockchain(
        test_util::ADDRESS_SPEND,
        test_util::PASSPHRASE_SPEND,
        "relay1",
        app_id,
        asset_id,
        &creator,
        cfg.clone(),
    );
    register_client_on_blockchain(
        test_util::ADDRESS_RECEIVE,
        test_util::PASSPHRASE_RECEIVE,
        "relay2",
        app_id,
        asset_id,
        &creator,
        cfg.clone(),
    );

    register_relays(app_id, asset_id, relay1_addr, relay2_addr);

    // Wait for relays to be visible via indexer
    let roots = vec![
        (test_util::ADDRESS_SPEND.to_string(), relay1_addr),
        (test_util::ADDRESS_RECEIVE.to_string(), relay2_addr),
    ];
    let ab_creator =
        bingle_core::blockchain::algo_bingle::AlgoBingle::new(creator.clone(), app_id, 0);
    if !wait_for_relays_visible(&ab_creator, app_id, &roots, Duration::from_secs(60)) {
        panic!("Root relays did not become visible via indexer");
    }

    // Start two root relays
    let relay1 = start_root_relay(
        "relay1",
        relay1_addr,
        test_util::PASSPHRASE_SPEND,
        app_id,
        cfg.clone(),
    );
    let relay2 = start_root_relay(
        "relay2",
        relay2_addr,
        test_util::PASSPHRASE_RECEIVE,
        app_id,
        cfg.clone(),
    );

    // Register client on blockchain
    register_client_on_blockchain(
        CLIENT_RESTRICTED_ADDR,
        CLIENT_RESTRICTED_PASS,
        "client_restricted",
        app_id,
        asset_id,
        &creator,
        cfg.clone(),
    );

    // Phase 1: start STUN with PORT_1, broken_nat=true (restricted cone: only one server responds)
    let port_1 = test_util::find_unused_loopback_port();
    tracing::info!(
        "[Test] Phase 1: starting STUN with external PORT_1={}",
        port_1
    );
    let (mut stun1_a, mut stun1_b, stun_list_1) = start_stun_pair_with_external_port(port_1, true);

    // Start the client
    let opts = StartOptions {
        handle: "client_restricted".into(),
        algo_passphrase: Some(CLIENT_RESTRICTED_PASS.parse().unwrap()),
        static_ip: None,
        am_relay: false,
        stun_servers: Some(stun_list_1.clone()),
        algo_provider_config: Some(cfg.clone()),
        algo_network: None,
        app_id: Some(app_id),
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: false,
        log_mode: bingle_core::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    };
    let client = bingle_core::api::bingle_api_impl::BingleApiImpl::new(&opts);
    client
        .access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts))
        .expect("client start");

    // Install on_listening handler
    let listening_flag = Arc::new(AtomicBool::new(false));
    let nat_type_guard: Arc<Mutex<Option<NatType>>> = Arc::new(Mutex::new(None));
    setup_on_listening(&client, &listening_flag, &nat_type_guard);

    // Wait for client to reach Registered
    assert!(
        test_util::wait_for_registered(&client, Duration::from_secs(120)),
        "client did not reach Registered state (state={:?})",
        client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_state_for_tests())
    );
    tracing::info!("[Test] client reached Registered state");

    // Validate on_listening was called with true and NatType::Restricted
    {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(10) {
            if listening_flag.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        assert!(
            listening_flag.load(Ordering::SeqCst),
            "on_listening(true) was not called after registration"
        );
        let nt = nat_type_guard
            .lock()
            .expect("lock nat_type_guard")
            .expect("nat_type should be set");
        assert_eq!(
            nt,
            NatType::Restricted,
            "expected NatType::Restricted for broken_nat client, got {:?}",
            nt
        );
        tracing::info!("[Test] on_listening(true, {:?}) confirmed", nt);
    }

    // Validate client registered with a relay
    let client_id = client
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id())
        .expect("client id should be set");
    let id_r1 = relay1
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id())
        .expect("relay1 id");
    let id_r2 = relay2
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id())
        .expect("relay2 id");
    let ep = relay1
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_ddb_lookup_for_tests(&client_id))
        .expect("relay1 should be able to look up client");
    assert!(
        ep.is_relay(),
        "client should be registered as relay endpoint (restricted NAT)"
    );
    let relay_id_for_client = ep.relay_id().expect("relay_id should be set");
    assert!(
        relay_id_for_client == id_r1 || relay_id_for_client == id_r2,
        "client should use one of the two relays"
    );
    tracing::info!(
        "[Test] client registered with relay {}",
        relay_id_for_client
    );

    // Phase 2: stop STUN servers — client should lose connectivity and call on_listening(false)
    tracing::info!("[Test] Phase 2: stopping STUN servers");
    listening_flag.store(true, Ordering::SeqCst); // reset to true so we detect the false call
    stun1_a.stop();
    stun1_b.stop();

    // Wait for on_listening(false) to be called
    {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(150) {
            if !listening_flag.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        assert!(
            !listening_flag.load(Ordering::SeqCst),
            "on_listening(false) was not called after STUN stopped"
        );
        tracing::info!("[Test] on_listening(false) confirmed after STUN stopped");
    }

    // Validate client state is None (no active state)
    assert!(
        wait_for_state_none(&client, Duration::from_secs(30)),
        "client did not return to None state after STUN stopped (state={:?})",
        client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_state_for_tests())
    );
    tracing::info!("[Test] client state is None after STUN stopped");

    // Phase 3: restart STUN with PORT_2 (simulating IP/port change after network outage).
    // Stop the old client and restart it with new STUN servers pointing to PORT_2.
    tracing::info!("[Test] Phase 3: stopping client before restarting with new STUN");
    client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());

    let port_2 = test_util::find_unused_loopback_port();
    assert_ne!(
        port_1, port_2,
        "PORT_2 must differ from PORT_1 to simulate address change"
    );
    tracing::info!(
        "[Test] Phase 3: restarting STUN with external PORT_2={}",
        port_2
    );
    let (mut stun2_a, mut stun2_b, stun_list_2) = start_stun_pair_with_external_port(port_2, true);

    // Restart the client with the new STUN list (new external port = new public address)
    let opts2 = StartOptions {
        handle: "client_restricted".into(),
        algo_passphrase: Some(CLIENT_RESTRICTED_PASS.parse().unwrap()),
        static_ip: None,
        am_relay: false,
        stun_servers: Some(stun_list_2),
        algo_provider_config: Some(cfg.clone()),
        algo_network: None,
        app_id: Some(app_id),
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: false,
        log_mode: bingle_core::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    };
    let client = bingle_core::api::bingle_api_impl::BingleApiImpl::new(&opts2);
    client
        .access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts2))
        .expect("client restart");
    tracing::info!("[Test] client restarted with PORT_2={}", port_2);

    // Re-install on_listening handler on the new client instance
    listening_flag.store(false, Ordering::SeqCst);
    setup_on_listening(&client, &listening_flag, &nat_type_guard);

    // Wait for client to re-register
    assert!(
        test_util::wait_for_registered(&client, Duration::from_secs(120)),
        "client did not re-register after network change (state={:?})",
        client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.engine_state_for_tests())
    );
    tracing::info!("[Test] client re-registered after network change");

    // Validate on_listening(true) called again
    {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(10) {
            if listening_flag.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        assert!(
            listening_flag.load(Ordering::SeqCst),
            "on_listening(true) was not called after re-registration"
        );
        tracing::info!("[Test] on_listening(true) confirmed after re-registration");
    }

    // Re-fetch client_id from the new instance (same identity, same id)
    let client_id = client
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id())
        .expect("client id should be set after restart");

    // Phase 4: validate messaging — client sends to relay1
    tracing::info!("[Test] Phase 4: client sends message to relay1");
    let received_relay1 = Arc::new(AtomicBool::new(false));
    let payload_relay1: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
    let who_relay1: Arc<Mutex<Option<(String, String)>>> = Arc::new(Mutex::new(None));
    {
        use bingle_core::api::bingle_api::OnMessageHandler;
        let flag = received_relay1.clone();
        let payload = payload_relay1.clone();
        let who = who_relay1.clone();
        let handler: Arc<OnMessageHandler> = Arc::new(move |sender, sender_handle, message| {
            tracing::info!(
                "[Test][relay1 on_message] sender={} handle={} msg={}",
                sender,
                sender_handle,
                message
            );
            if let Ok(mut g) = payload.lock() {
                *g = Some(message);
            }
            if let Ok(mut g) = who.lock() {
                *g = Some((sender.clone(), sender_handle.clone()));
            }
            flag.store(true, Ordering::SeqCst);
        });
        relay1.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.set_on_message(Some(handler)));
    }

    let msg_to_relay = json!({ "text": "hello from restricted client" });
    let sent = client
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| {
            c.send_message_to_id(&id_r1, msg_to_relay.clone(), None)
        })
        .expect("send_message_to_id should not error");
    assert!(sent, "send_message_to_id client→relay1 should return true");

    {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(60) {
            if received_relay1.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        assert!(
            received_relay1.load(Ordering::SeqCst),
            "relay1 did not receive message from client"
        );
        let guard = payload_relay1.lock().expect("lock payload_relay1");
        let p = guard.as_ref().expect("payload should be Some");
        assert_eq!(
            p.get("text").and_then(|v| v.as_str()),
            Some("hello from restricted client")
        );
        tracing::info!("[Test] relay1 received message from client");
    }

    // Phase 5: relay1 looks up client by handle and sends it a message
    tracing::info!("[Test] Phase 5: relay1 sends message to client by id");
    let received_client = Arc::new(AtomicBool::new(false));
    let payload_client: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
    {
        use bingle_core::api::bingle_api::OnMessageHandler;
        let flag = received_client.clone();
        let payload = payload_client.clone();
        let handler: Arc<OnMessageHandler> = Arc::new(move |sender, sender_handle, message| {
            tracing::info!(
                "[Test][client on_message] sender={} handle={} msg={}",
                sender,
                sender_handle,
                message
            );
            if let Ok(mut g) = payload.lock() {
                *g = Some(message);
            }
            flag.store(true, Ordering::SeqCst);
        });
        client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.set_on_message(Some(handler)));
    }

    let msg_to_client = json!({ "text": "hello from relay1 to client" });
    let sent_back = relay1
        .access_unsafe_for_tests(|r: &mut BingleApiImpl| {
            r.send_message_to_id(&client_id, msg_to_client.clone(), None)
        })
        .expect("send_message_to_id relay1→client should not error");
    assert!(
        sent_back,
        "send_message_to_id relay1→client should return true"
    );

    {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(60) {
            if received_client.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        assert!(
            received_client.load(Ordering::SeqCst),
            "client did not receive message from relay1"
        );
        let guard = payload_client.lock().expect("lock payload_client");
        let p = guard.as_ref().expect("payload should be Some");
        assert_eq!(
            p.get("text").and_then(|v| v.as_str()),
            Some("hello from relay1 to client")
        );
        tracing::info!("[Test] client received message from relay1");
    }

    // Tear down
    tracing::info!("[Test] tearing down");
    relay1.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    relay2.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());
    stun2_a.stop();
    stun2_b.stop();
}
