use rust_comms::api::network_endpoint::NetworkEndpoint;
use rust_comms::dtls::network_mux_trait::NetworkMux;
use rust_comms::dtls::network_mux_udp::UdpNetworkMux;
use rust_comms::messages::handlers::DefaultPrintingHandler;
use rust_comms::messages::types::{RelayCall, RelayListen};
use rust_comms::messages::{Message, RelayMessage};
use rust_comms::turn::turn_handler::TurnHandler;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::util::reusable_mock_api::{InnerBingleApiInternal, MockApiBoth};

#[path = "../test_util.rs"]
mod test_util;

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

fn build_channel_data(channel: u16, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + payload.len() + 3);
    out.extend_from_slice(&channel.to_be_bytes());
    out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    out.extend_from_slice(payload);
    let pad = (4 - (payload.len() % 4)) % 4;
    for _ in 0..pad { out.push(0); }
    out
}

#[test]
fn end_to_end_turn_relay_forwards_dtls() {
    // Allocate two free ports on loopback: relay and destination mux
    let relay_port = test_util::find_unused_loopback_port();
    let mut b_port = test_util::find_unused_loopback_port();
    if b_port == relay_port { b_port = test_util::find_unused_loopback_port(); }
    assert_ne!(relay_port, 0);
    assert_ne!(b_port, 0);

    let relay_addr = addr(relay_port);
    let b_addr = addr(b_port);

    // Create destination mux (client B) with a DTLS handler that records packets
    let mut mux_b = UdpNetworkMux::bind(b_addr).expect("bind B mux");
    let dtls_records: Arc<Mutex<Vec<(NetworkEndpoint, Vec<u8>)>>> = Arc::new(Mutex::new(Vec::new()));
    let dtls_records_clone = dtls_records.clone();
    mux_b.set_handle_dtls(Some(Arc::new(move |_source: &dyn NetworkMux, from: &NetworkEndpoint, data: &[u8]| {
        if let Ok(mut rec) = dtls_records_clone.lock() {
            rec.push((from.clone(), data.to_vec()));
        }
    })));
    let mux_b = Arc::new(mux_b);
    mux_b.start().expect("start B mux");

    // Create the relay UDP mux and TURN handler
    let mut mux_relay = UdpNetworkMux::bind(relay_addr).expect("bind relay mux");
    let turn = std::sync::Arc::new(rust_comms::turn::turn_handler::TurnHandlerImpl::new());

    // Configure TURN ChannelData handler to forward stripped payloads to destination
    {
        let turn_clone = turn.clone();
        let th: std::sync::Arc<dyn Fn(&dyn NetworkMux, &SocketAddr, &[u8]) + Send + Sync> = std::sync::Arc::new(move |source: &dyn NetworkMux, from: &SocketAddr, packet: &[u8]| {
            if let Some(wrapped) = turn_clone.handle_turn_incoming(Some(from), Some(relay_addr), packet) {
                if let Some(udp) = source.as_any().downcast_ref::<UdpNetworkMux>() {
                    let _ = udp.write(&NetworkEndpoint::new_direct(wrapped.ip_address), &wrapped.message);
                }
            }
        });
        mux_relay.set_handle_turn(Some(&th));
    }

    // Start relay mux receive loop
    let mux_relay = std::sync::Arc::new(mux_relay);
    mux_relay.start().expect("start relay mux");

    // Prepare a Router acting as the relay to process Listen and Call messages
    struct MockInternal { pub turn: std::sync::Arc<rust_comms::turn::turn_handler::TurnHandlerImpl> }
    impl InnerBingleApiInternal for MockInternal {
        fn turn_lookup_addr_by_id(&self, id: String) -> Option<SocketAddr> { self.turn.lookup_addr_by_id(&id) }
        fn turn_handle_call(&self, source: SocketAddr, dest: SocketAddr) -> i32 { rust_comms::turn::turn_handler::TurnRelayHandler::handle_call(&*self.turn, &source, &dest) }
        fn turn_handle_listen(&self, id: String, source: SocketAddr) -> bool { use rust_comms::turn::turn_handler::TurnHandler; self.turn.handle_listen(&id, &source) }
    }
    let mock_internal = Arc::new(MockInternal { turn: turn.clone() });
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_internal_override(mock_internal))));
    router.set_am_relay(true);

    let handler = DefaultPrintingHandler;

    // 1) Simulate B sending RelayListen to the relay
    router.set_last_from(Some(b_addr));
    let listen_msg = Message::Relay(RelayMessage::Listen(RelayListen { app: None }));
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &listen_msg, "BID");
    });
    // Validate id->addr registration
    assert_eq!(turn.lookup_addr_by_id("BID"), Some(b_addr));

    // 2) Simulate A sending RelayCall(calledId=BID) to the relay
    let a_addr = addr(test_util::find_unused_loopback_port());
    router.set_last_from(Some(a_addr));
    let call_msg = Message::Relay(RelayMessage::Call(RelayCall { app: None, called_id: "BID".to_string() }));
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &call_msg, "AID");
    });
    // Extract channel from outbound response
    let out = router.take_outbound_response().expect("RelayResponse present");
    let ch = out.get("channel").and_then(|v: &serde_json::Value| v.as_u64()).expect("channel") as u16;

    // 3) Send TURN ChannelData from A to the relay containing a DTLS-shaped payload (first byte 20..=63)
    let dtls_payload: [u8; 6] = [20, 1, 2, 3, 4, 5]; // classified as DTLS by mux_type_for
    let ch_data = build_channel_data(ch, &dtls_payload);
    let send_sock = UdpSocket::bind(a_addr).expect("bind sender udp");
    send_sock.send_to(&ch_data, relay_addr).expect("send channeldata");

    // 4) Wait for DTLS handler on B mux to be invoked and validate
    let start = Instant::now();
    let ok = loop {
        {
            let rec = dtls_records.lock().unwrap();
            if !rec.is_empty() { break true; }
        }
        if start.elapsed() > Duration::from_secs(20) { break false; }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(ok, "expected DTLS handler on B to be invoked");

    // Validate content and that source was relay_addr
    let recs = dtls_records.lock().unwrap();
    let (from_addr, data) = recs[0].clone();
    assert_eq!(from_addr.inet_socket_address(), Some(relay_addr));
    assert_eq!(data, dtls_payload);

    // Cleanup
    mux_relay.stop();
    mux_b.stop();
}
