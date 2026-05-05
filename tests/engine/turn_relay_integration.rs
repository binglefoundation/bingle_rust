use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::Duration;

use rust_comms::dtls::network_mux_trait::NetworkMux;
use rust_comms::dtls::network_mux_udp::UdpNetworkMux;
use rust_comms::messages::handlers::DefaultPrintingHandler;
use rust_comms::messages::types::{RelayCall, RelayListen};
use rust_comms::messages::{Message, RelayMessage};
use rust_comms::turn::turn_handler::TurnHandler;

use crate::util::reusable_mock_api::{InnerBingleApiInternal, MockApiBoth};

#[path = "../test_util.rs"]
pub mod test_util;

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

#[cfg_attr(not(target_os = "ios"), test)]
pub fn end_to_end_turn_relay_forwards_payload() {
    // Allocate three ports: relay, client A, client B
    let relay_port = test_util::find_unused_loopback_port();
    let a_port = test_util::find_unused_loopback_port();
    let b_port = test_util::find_unused_loopback_port();
    assert_ne!(relay_port, 0);
    assert_ne!(a_port, 0);
    assert_ne!(b_port, 0);

    let relay_addr = addr(relay_port);
    let a_addr = addr(a_port);
    let b_addr = addr(b_port);

    // Create the relay UDP mux and TURN handler
    let mut mux = UdpNetworkMux::bind(relay_addr).expect("bind relay mux");
    let turn = std::sync::Arc::new(rust_comms::turn::turn_handler::TurnHandlerImpl::new());

    // Configure TURN ChannelData handler to forward payloads to destination
    {
        let turn_clone = turn.clone();
        let th: std::sync::Arc<dyn Fn(&dyn NetworkMux, &SocketAddr, &[u8]) + Send + Sync> = std::sync::Arc::new(move |source: &dyn NetworkMux, from: &SocketAddr, packet: &[u8]| {
            if let Some(wrapped) = turn_clone.handle_turn_incoming(Some(from), Some(relay_addr), packet) {
                if let Some(udp) = source.as_any().downcast_ref::<UdpNetworkMux>() {
                    let nsk = rust_comms::api::bingle_api::NetworkEndpoint::new_direct(wrapped.ip_address);
                    let _ = udp.write(&nsk, &wrapped.message);
                }
            }
        });
        mux.set_handle_turn(Some(&th));
    }

    // Start mux receive loop
    let mux = std::sync::Arc::new(mux);
    mux.start().expect("start mux");

    // Prepare a Router acting as the relay to process Listen and Call messages
    struct MockInternal { pub turn: std::sync::Arc<rust_comms::turn::turn_handler::TurnHandlerImpl> }
    impl InnerBingleApiInternal for MockInternal {
        fn turn_lookup_addr_by_id(&self, id: String) -> Option<SocketAddr> { self.turn.lookup_addr_by_id(&id) }
        fn turn_handle_call(&self, source_id: String, dest_id: String, source: SocketAddr, dest: SocketAddr) -> i32 { rust_comms::turn::turn_handler::TurnRelayHandler::handle_call(&*self.turn, &source_id, &dest_id, &source, &dest) }
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
    router.set_last_from(Some(a_addr));
    let call_msg = Message::Relay(RelayMessage::Call(RelayCall { app: None, called_id: "BID".to_string() }));
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &call_msg, "AID");
    });
    // Extract channel from outbound response
    let out = router.take_outbound_response().expect("RelayResponse present");
    let ch = out.get("channel").and_then(|v: &serde_json::Value| v.as_u64()).expect("channel") as u16;

    // 3) Bind a raw UDP socket on B's address to receive the forwarded payload
    let recv_sock = UdpSocket::bind(b_addr).expect("bind b udp");
    recv_sock.set_read_timeout(Some(Duration::from_secs(2))).ok();

    // 4) Send TURN ChannelData from A to the relay and verify it arrives at B
    let payload = b" TURN_OK"; // leading space to avoid special mux classifications
    let ch_data = build_channel_data(ch, payload);
    let send_sock = UdpSocket::bind(a_addr).expect("bind temp udp");
    send_sock.send_to(&ch_data, relay_addr).expect("send channeldata");

    // Receive forwarded packet at B: relay forwards the stripped inner payload (no TURN header)
    let mut buf = [0u8; 2048];
    let (n, _from) = recv_sock.recv_from(&mut buf).expect("receive at B");
    assert_eq!(n, payload.len());
    assert_eq!(&buf[..n], payload);

    // Cleanup
    mux.stop();
}

