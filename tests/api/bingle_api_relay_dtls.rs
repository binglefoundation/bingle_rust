use rust_comms::api::bingle_api::{StartOptions, Handle, NetworkEndpoint, BingleApi};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::dtls::{Dtls, DtlsOpenSsl};
use rust_comms::dtls::network_mux_trait::NetworkMux;
use rust_comms::dtls::network_mux_udp::UdpNetworkMux;
use rust_comms::messages::{Message, RelayMessage};
use rust_comms::messages::handlers::DefaultPrintingHandler;
use rust_comms::messages::types::{RelayListen, RelayCall};
use rust_comms::turn::turn_handler::TurnHandler;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[path = "../test_util.rs"]
mod test_util;

#[cfg(not(target_os = "ios"))]
#[path = "../dtls/pki.rs"]
mod pki;

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

#[test]
#[ignore]
#[cfg(not(target_os = "ios"))]
fn bingle_api_send_via_relay_end_to_end() {
    // 1) Spin up destination DTLS server (node B)
    let b_port = test_util::find_unused_loopback_port();
    let b_addr = addr(b_port);

    let mux_b = UdpNetworkMux::bind(b_addr).expect("bind B mux");
    let mux_b = Arc::new(mux_b);
    mux_b.start().expect("start B mux");

    // Prepare server PKI
    let certs = pki::generate_ed25519_test_certs();
    let server_cert_pem: Vec<u8> = certs.server_crt.clone();
    let server_key_pem: Vec<u8> = certs.server_key.clone();
    let ca_pem: Vec<u8> = certs.ca_crt.clone();

    // Record received application messages on B
    let received: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let rec_clone = received.clone();

    let mut server = DtlsOpenSsl::new()
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone())
        .with_handle_message(Arc::new(move |_server: &dyn Dtls, _from: &NetworkEndpoint, _issuer: &str, data: &[u8]| {
            if let Ok(mut v) = rec_clone.lock() { v.push(data.to_vec()); }
        }));
    server.start(mux_b.clone()).expect("server start");

    // 2) Spin up a relay UDP mux + TURN handler + Router acting as a relay
    let relay_port = test_util::find_unused_loopback_port();
    let relay_addr = addr(relay_port);

    let mut mux_relay = UdpNetworkMux::bind(relay_addr).expect("bind relay mux");
    let turn = std::sync::Arc::new(rust_comms::turn::turn_handler::TurnHandlerImpl::new());

    {
        // Forward TURN ChannelData payloads to the indicated destination
        let turn_clone = turn.clone();
        mux_relay.set_handle_turn(Some(Arc::new(move |source: &dyn NetworkMux, from: &SocketAddr, packet: &[u8]| {
            if let Some(wrapped) = turn_clone.handle_turn_incoming(Some(from), Some(relay_addr), packet) {
                if let Some(udp) = source.as_any().downcast_ref::<UdpNetworkMux>() {
                    let nsk = NetworkEndpoint::new_direct(wrapped.ip_address);
                    let _ = udp.write(&nsk, &wrapped.message);
                }
            }
        })));
    }

    let mux_relay = Arc::new(mux_relay);
    mux_relay.start().expect("start relay mux");

    let router = Arc::new(rust_comms::messages::router::Router::new(Arc::new(MockApi)));
    router.set_am_relay(true);
    router.set_turn_handler(Some(turn.clone()));
    let handler = DefaultPrintingHandler;

    // 3) B sends RelayListen to the relay to register its id -> address mapping
    router.set_last_from(Some(b_addr));
    let listen_msg = Message::Relay(RelayMessage::Listen(RelayListen { app: None }));
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &listen_msg, "BID");
    });
    assert_eq!(turn.lookup_addr_by_id("BID"), Some(b_addr));

    // 4) A sends RelayCall(calledId=BID) to the relay; extract assigned channel
    let a_addr = addr(test_util::find_unused_loopback_port());
    router.set_last_from(Some(a_addr));
    let call_msg = Message::Relay(RelayMessage::Call(RelayCall { app: None, called_id: "BID".to_string() }));
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &call_msg, "AID");
    });
    let out = router.take_outbound_response().expect("RelayResponse present");
    let ch = out.get("channel").and_then(|v| v.as_u64()).expect("channel") as u16;

    // 5) Build Bingle API client (node A)
    let mut api = BingleApiImpl::new(&StartOptions::default());
    let opts = StartOptions {
        handle: Handle::from("alice"),
        algo_passphrase: Some(test_util::PASSPHRASE_SPEND.to_string()),
        static_ip: None,
        am_relay: false,
        stun_servers: Some(vec![SocketAddr::from(([127, 0, 0, 1], 3478))]),
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None,
    };
    let start_res = api.start(&opts);
    if let Err(e) = start_res { eprintln!("api.start error: {}", e); }

    // 6) Send a message via the relay using NetworkEndpoint::new_relay
    let nsk = NetworkEndpoint::new_relay("BID".to_string(), Some(relay_addr), Some(ch));
    let uid = test_util::ADDRESS_SPEND.to_string();
    let payload = serde_json::json!({"app": null, "type": "HelloRelay", "ts": 1});
    // Retry send a few times to avoid flakiness due to startup races
    let mut ok = false;
    for _ in 0..10 {
        if api.send_message_to_network(&nsk, &uid, payload.clone(), None) { ok = true; break; }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(ok, "send_message_to_network returned false");

    // 7) Await delivery at B
    let start = Instant::now();
    let mut delivered = false;
    while start.elapsed() < Duration::from_secs(3) {
        if let Ok(v) = received.lock() {
            if !v.is_empty() { delivered = true; break; }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(delivered, "expected destination to receive data via DTLS over relay");

    // Optionally, validate content is JSON and matches our payload type
    if let Ok(v) = received.lock() {
        if let Some(first) = v.first() {
            if let Ok(txt) = std::str::from_utf8(first) {
                let parsed: serde_json::Value = serde_json::from_str(txt).unwrap_or(serde_json::Value::Null);
                assert_eq!(parsed.get("type").and_then(|s| s.as_str()), Some("HelloRelay"));
            }
        }
    }

    // Cleanup
    mux_relay.stop();
    mux_b.stop();
}

// Minimal API stub for Router context in this test
struct MockApi;
impl rust_comms::api::bingle_api::BingleApi for MockApi {
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> { None }
    fn get_handle(&self) -> Option<String> { None }
    fn get_app_id(&self) -> Option<u64> { None }
    fn get_algo_provider_config(&self) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> { None }
    fn start(&mut self, _options: &rust_comms::api::bingle_api::StartOptions) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}
    fn send_message_to_id(&self, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<std::sync::Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &rust_comms::api::bingle_api::Handle, _message: serde_json::Value, _progress: Option<std::sync::Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _network_source_key: &rust_comms::api::bingle_api::NetworkEndpoint, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<std::sync::Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<std::sync::Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &rust_comms::api::bingle_api::Handle, _message: serde_json::Value, _progress: Option<std::sync::Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_network_with_response(&self, _network_source_key: &rust_comms::api::bingle_api::NetworkEndpoint, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<std::sync::Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn set_on_message(&mut self, _handler: Option<std::sync::Arc<rust_comms::api::bingle_api::OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<std::sync::Arc<rust_comms::api::bingle_api::OnConnectHandler>>) {}
}
