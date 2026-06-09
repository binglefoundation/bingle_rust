use rust_comms::engine::BingleAccessUnsafeForTests;
use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, StartOptions, UserId};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::dtls::network_mux_trait::NetworkMux;
use rust_comms::dtls::network_mux_udp::UdpNetworkMux;
use rust_comms::dtls::{Dtls, DtlsOpenSsl};
use rust_comms::messages::handlers::DefaultPrintingHandler;
use rust_comms::messages::types::{RelayCall, RelayListen, RelayListenResponse};
use rust_comms::messages::{Message, RelayMessage};
use rust_comms::turn::turn_handler::TurnClientImpl;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use crate::util::reusable_mock_api::{InnerBingleApiInternal, MockApiBoth};
use crate::util::test_util::ADDRESS_SPEND;

#[path = "../test_util.rs"]
pub mod test_util;


#[path = "../dtls/pki.rs"]
pub mod pki;

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

#[cfg_attr(not(target_os = "ios"), test)]
#[ntest::timeout(30_000)]

pub fn bingle_api_send_via_relay() {
    test_util::init_test_logging();

    let b_id = ADDRESS_SPEND;

    // 1) Spin up destination DTLS server (node B)
    tracing::info!("Starting DTLS server");
    let b_port = test_util::find_unused_loopback_port();
    let b_addr = addr(b_port);

    let mut mux_b = UdpNetworkMux::bind(b_addr).expect("bind B mux");

    let turn_client = std::sync::Arc::new(TurnClientImpl::new());
    let turn_client_clone = turn_client.clone();
    let client_turn_handler: std::sync::Arc<dyn Fn(&dyn NetworkMux, &SocketAddr, &[u8]) + Send + Sync> = Arc::new(
        move |source: &dyn NetworkMux, from: &SocketAddr, packet: &[u8]| {
            tracing::info!("client_turn_handler Got packet from {}:", from);
            // Parse/unwrap the TURN ChannelData using our handler
            if let Some(wrapped) =
                turn_client_clone.handle_turn_incoming(Some(from), Some(b_addr), packet)
            {
                tracing::info!(
                            "Got wrapped message {} bytes from {}:",
                            wrapped.message.len(),
                            wrapped.network_endpoint
                        );
                // Non-relay role: this packet is for us. Re-inject the stripped payload into the UDP mux
                if let Some(udp) = source
                    .as_any()
                    .downcast_ref::<UdpNetworkMux>(
                    ) {
                    udp.reprocess(&wrapped.network_endpoint, &wrapped.message);
                    tracing::info!(
                                "reprocessed {} bytes from {}",
                                wrapped.message.len(),
                                wrapped.network_endpoint
                            );
                } else {
                    tracing::warn!(
                                "source is not UdpNetworkMux; cannot reprocess"
                            );
                }
            } else {
                tracing::warn!("handle_turn_incoming returned None (ignored)");
            }
        });

    mux_b.set_handle_turn(Some(&client_turn_handler));

    let mux_b_arc = Arc::new(mux_b);
    mux_b_arc.start().expect("start B mux");

    // Prepare server PKI
    let certs = pki::generate_ed25519_test_certs();
    let server_cert_pem: Vec<u8> = certs.server_crt.clone();
    let server_key_pem: Vec<u8> = certs.server_key.clone();
    let ca_pem: Vec<u8> = certs.ca_crt.clone();

    // Record received application messages on B
    let received: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let rec_clone = received.clone();

    let mut server = DtlsOpenSsl::new("server".to_string())
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone())
        .with_handle_message(Arc::new(move |server: &dyn Dtls, from: &NetworkEndpoint, _issuer: &str, data: &[u8]| {
            // Send FRPT ACK_COMPLETE for DATA_SINGLE packets
            if data.len() >= 4 && (data[0] >> 4) == 0x1 && (data[0] & 0x0F) == 0x1 {
                let ack = vec![0x14u8, 0x00, data[2], data[3]];
                let _ = server.send(from, &ack);
            }
            let unwrapped = test_util::maybe_unwrap_data_single(data);
            tracing::info!("Received message from B: {:?}", std::str::from_utf8(unwrapped));
            if let Ok(mut v) = rec_clone.lock() {
                v.push(unwrapped.to_vec());
            } else {
                panic!("received mutex poisoned");
            }
        }));
    server.start(mux_b_arc.clone()).expect("server start");

    // 2) Spin up a relay UDP mux + TURN handler + Router acting as a relay
    tracing::info!("Starting relay UDP mux + TURN handler + Router");
    let relay_port = test_util::find_unused_loopback_port();
    let relay_addr = addr(relay_port);

    let mut mux_relay = UdpNetworkMux::bind(relay_addr).expect("bind relay mux");
    let turn = std::sync::Arc::new(rust_comms::turn::turn_handler::TurnHandlerImpl::new());

    let a_addr_capture = Arc::new(Mutex::new(None));
    {
        // Forward TURN ChannelData payloads to the indicated destination
        let turn_clone = turn.clone();
        let a_addr_capture_inner = a_addr_capture.clone();
        let th: rust_comms::dtls::network_mux_trait::HandleTurn = Arc::new(move |source: &dyn NetworkMux, from: &SocketAddr, packet: &[u8]| {
            if let Ok(mut g) = a_addr_capture_inner.lock() {
                if g.is_none() { *g = Some(*from); }
            }
            if let Some(wrapped) = turn_clone.handle_turn_incoming(Some(from), Some(relay_addr), packet) {
                if let Some(udp) = source.as_any().downcast_ref::<UdpNetworkMux>() {
                    let nsk = NetworkEndpoint::new_direct(wrapped.ip_address);
                    tracing::info!("Relay forwarding (TURN->RAW) from {} to {}", from, nsk);
                    let _ = udp.write(&nsk, &wrapped.message);
                }
            }
        });
        mux_relay.set_handle_turn(Some(&th));

        let turn_clone2 = turn.clone();
        let a_addr_capture_inner2 = a_addr_capture.clone();
        let dh: rust_comms::dtls::network_mux_trait::HandleDtls = Arc::new(move |source: &dyn NetworkMux, from: &NetworkEndpoint, packet: &[u8]| {
            let from_addr = from.inet_socket_address().expect("DTLS from IP");
            let a_addr = {
                let g = a_addr_capture_inner2.lock().unwrap();
                match *g {
                    Some(addr) => addr,
                    None => return,
                }
            };
            if let Some(wrapped) = turn_clone2.send_turn_outgoing(&from_addr, &a_addr, packet) {
                if let Some(udp) = source.as_any().downcast_ref::<UdpNetworkMux>() {
                    let nsk = NetworkEndpoint::new_direct(wrapped.ip_address);
                    tracing::info!("Relay forwarding (RAW->TURN) from {} to {}", from, nsk);
                    let _ = udp.write(&nsk, &wrapped.message);
                }
            }
        });
        mux_relay.set_handle_dtls(Some(dh));
    }

    let mux_relay = Arc::new(mux_relay);
    mux_relay.start().expect("start relay mux");

    // Provide internal API exposing the shared TurnHandlerImpl
    struct MockInternal { pub turn: std::sync::Arc<rust_comms::turn::turn_handler::TurnHandlerImpl> }
    impl InnerBingleApiInternal for MockInternal {
        fn turn_lookup_addr_by_id(&self, id: std::string::String) -> Option<std::net::SocketAddr> { self.turn.lookup_addr_by_id(&id) }
        fn turn_handle_call(&self, source_id: String, dest_id: String, source: std::net::SocketAddr, dest: std::net::SocketAddr) -> i32 { rust_comms::turn::turn_handler::TurnRelayHandler::handle_call(&*self.turn, &source_id, &dest_id, &source, &dest) }
        fn turn_handle_listen(&self, id: std::string::String, source: std::net::SocketAddr) -> bool { use rust_comms::turn::turn_handler::TurnHandler;
            self.turn.handle_listen(&id, &source)
        }
    }
    let mock_internal = Arc::new(MockInternal { turn: turn.clone() });
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_internal_override(mock_internal))));
    router.set_am_relay(true);
    let captured_relay_called: Arc<Mutex<Option<(NetworkEndpoint, String, serde_json::Value)>>> = Arc::new(Mutex::new(None));
    let captured_relay_called_clone = captured_relay_called.clone();
    router.set_sender(Some(Arc::new(move |nsk: &NetworkEndpoint, uid: &UserId, json: serde_json::Value| {
        *captured_relay_called_clone.lock().expect("capture relay called") = Some((nsk.clone(), uid.to_string(), json.clone()));
        true
    })));
    let handler = DefaultPrintingHandler;

    // 3) B sends RelayListen to the relay to register its id -> address mapping
    tracing::info!("B sending RelayListen");
    router.set_last_from(Some(b_addr));
    let listen_msg = Message::Relay(RelayMessage::Listen(RelayListen { app: None, tag: None }));
    let listen_responses = rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &listen_msg, b_id)
    });
    assert_eq!(turn.lookup_addr_by_id(b_id), Some(b_addr));
    let listen_out = listen_responses
        .into_iter().next()
        .expect("ListenResponse present");
    assert_eq!(
        listen_out
            .get("type")
            .and_then(|v: &serde_json::Value| v.as_str()),
        Some("ListenResponse")
    );

    tracing::info!("Relay sending RelayListenResponse to B");
    let resp = Message::Relay(RelayMessage::ListenResponse(RelayListenResponse { app: None, response_tag: None }));
    let resp_bytes = serde_json::to_vec(&resp).expect("marshal ListenResponse");
    let b_nsk = NetworkEndpoint::new_direct(b_addr);
    mux_relay.write(&b_nsk, &resp_bytes).expect("relay write to B");

    // 5) Build Bingle API client (node A)
    tracing::info!("Starting Bingle API client");
    let api = BingleApiImpl::new(&StartOptions::default());
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
        log_level: None, handle_cache_expiry: None, dangerous_debug: true, log_mode: rust_comms::util::logging::LogMode::Plain,
    };
    let start_res = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts));
    if let Err(e) = start_res { eprintln!("api.start error: {}", e); }

    // 4) A sends RelayCall(calledId=BID) to the relay; extract assigned channel
    tracing::info!("A sending RelayCall");
    let a_addr = api
        .access_unsafe_for_tests(|a: &mut BingleApiImpl| a.engine_local_bind_addr_for_tests())
        .expect("api local bind addr");
    router.set_last_from(Some(a_addr));
    api.engine_for_tests().access_unsafe_for_tests(|e| e.set_last_public_addr(Some(a_addr)));
    let call_msg = Message::Relay(RelayMessage::Call(RelayCall { app: None, called_id: b_id.to_string(), tag: None }));
    let call_responses = rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &call_msg, "AID")
    });
    let out = call_responses.into_iter().next().expect("RelayResponse present");
    let ch = out.get("channel").and_then(|v: &serde_json::Value| v.as_u64()).expect("channel") as u16;
    let relay_called = captured_relay_called
        .lock()
        .expect("relay called lock")
        .clone()
        .expect("RelayCalled should be sent to called id");
    assert_eq!(relay_called.0, NetworkEndpoint::new_direct(b_addr));
    assert_eq!(relay_called.1, b_id);
    assert_eq!(relay_called.2.get("type").and_then(|v: &serde_json::Value| v.as_str()), Some("RelayCalled"));
    assert_eq!(relay_called.2.get("channel").and_then(|v: &serde_json::Value| v.as_u64()), Some(ch as u64));

    tracing::info!("Node A faking handle_call_response for relay channel");
    let turn_client_a = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.engine_turn_client_handler_for_tests());
    use rust_comms::turn::turn_handler::TurnHandler;
    turn_client_a.handle_call_response(&a_addr, &relay_addr, ch, "RID");

    // 6) Send a message via the relay using NetworkEndpoint::new_relay
    tracing::info!("Sending message via relay");
    let nsk = NetworkEndpoint::new_relay("RID".to_string(), Some(relay_addr), Some(ch));
    let uid = test_util::ADDRESS_SPEND.to_string();
    let payload = serde_json::json!({"app": null, "type": "HelloRelay", "ts": 1});
    // Retry send a few times to avoid flakiness due to startup races
    let mut ok = false;
    for _ in 0..10 {
        if api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.send_message_to_network(&nsk, &uid, payload.clone(), None)).unwrap_or(false) { ok = true; break; }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(ok, "send_message_to_network returned false");

    // 7) Await delivery at B
    tracing::info!("Awaiting delivery at B");
    let start = Instant::now();
    let mut delivered = false;
    while start.elapsed() < Duration::from_secs(3) {
        if let Ok(v) = received.lock() {
            if !v.is_empty() {
                delivered = true;
                break;
            }
        } else {
            panic!("received mutex poisoned while awaiting delivery");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(delivered, "expected destination to receive data via DTLS over relay");

    // Optionally, validate content is JSON and matches our payload type
    tracing::info!("Validating received message content");
    if let Ok(v) = received.lock() {
        if let Some(first) = v.first() {
            if let Ok(txt) = std::str::from_utf8(test_util::maybe_unwrap_data_single(first)) {
                let parsed: serde_json::Value = serde_json::from_str(txt).unwrap_or(serde_json::Value::Null);
                assert_eq!(parsed.get("type").and_then(|s| s.as_str()), Some("HelloRelay"));
            } else {
                panic!("received payload was not valid UTF-8");
            }
        } else {
            panic!("received payload list was unexpectedly empty");
        }
    } else {
        panic!("received mutex poisoned during validation");
    }

    // Cleanup
    mux_relay.stop();
    mux_b_arc.stop();
}

