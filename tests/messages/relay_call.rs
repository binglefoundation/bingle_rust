use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use crate::util::reusable_mock_api::{InnerBingleApiInternal, MockApiBoth};
use rust_comms::api::bingle_api::{NetworkEndpoint, UserId};
use rust_comms::messages::handlers::DefaultPrintingHandler;
use rust_comms::messages::types::RelayCall;
use rust_comms::messages::{Message, RelayMessage};
use rust_comms::turn::turn_handler::TurnHandler;

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

#[test]
fn relay_call_allocates_channel_and_maps_pair() {
    // Arrange: router as relay with TURN handler and two registered peers
    let turn = std::sync::Arc::new(rust_comms::turn::turn_handler::TurnHandlerImpl::new());
    struct MockInternal { pub turn: std::sync::Arc<rust_comms::turn::turn_handler::TurnHandlerImpl> }
    impl InnerBingleApiInternal for MockInternal {
        fn turn_lookup_addr_by_id(&self, id: std::string::String) -> Option<std::net::SocketAddr> { self.turn.lookup_addr_by_id(&id) }
        fn turn_handle_call(&self, source: std::net::SocketAddr, dest: std::net::SocketAddr) -> i32 { rust_comms::turn::turn_handler::TurnRelayHandler::handle_call(&*self.turn, &source, &dest) }
        fn turn_handle_listen(&self, id: std::string::String, source: std::net::SocketAddr) -> bool { use rust_comms::turn::turn_handler::TurnHandler; self.turn.handle_listen(&id, &source) }
    }
    let mock_internal = Arc::new(MockInternal { turn: turn.clone() });
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_internal_override(mock_internal))));
    router.set_am_relay(true);

    let caller = addr(9101);
    let callee = addr(9102);
    // Register both via handle_listen
    assert!(turn.handle_listen("CALLERID", &caller));
    assert!(turn.handle_listen("CALLEEID", &callee));

    // Source of message is caller
    router.set_last_from(Some(caller));

    // Install a sender to capture RelayCalled sent to callee
    let captured: Arc<Mutex<Option<(NetworkEndpoint, String, serde_json::Value)>>> = Arc::new(Mutex::new(None));
    let captured_clone = captured.clone();
    router.set_sender(Some(Arc::new(move |nsk: &NetworkEndpoint, uid: &UserId, json: serde_json::Value| {
        *captured_clone.lock().unwrap() = Some((nsk.clone(), uid.to_string(), json.clone()));
        true
    })));

    // Act: send Relay::Call(calledId = CALLEEID)
    let handler = DefaultPrintingHandler;
    let call = Message::Relay(RelayMessage::Call(RelayCall { app: None, called_id: "CALLEEID".to_string() }));
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &call, "CALLERID");
    });

    // Assert: RelayResponse with channel
    let out = router.take_outbound_response();
    assert!(out.is_some(), "expected a RelayResponse");
    let obj = out.unwrap();
    let t = obj.get("type").and_then(|v: &serde_json::Value| v.as_str());
    assert_eq!(t, Some("RelayResponse"));
    let ch = obj.get("channel").and_then(|v: &serde_json::Value| v.as_u64()).expect("channel");
    let ch_u16 = ch as u16;

    // Verify RelayCalled was sent to callee with the same channel
    let sent = captured.lock().unwrap().clone().expect("RelayCalled should be sent to callee");
    let (nsk_sent, user_id_sent, json_sent) = sent;
    assert_eq!(nsk_sent, NetworkEndpoint::new_direct(callee));
    assert_eq!(user_id_sent, "CALLEEID");
    assert_eq!(json_sent.get("type").and_then(|v: &serde_json::Value| v.as_str()), Some("RelayCalled"));
    assert_eq!(json_sent.get("channel").and_then(|v: &serde_json::Value| v.as_u64()), Some(ch));

    // Verify internal mappings reflect (caller, callee) -> ch and ch -> caller
    let mapped_dest = turn.lookup_addr_by_channel_for_tests(ch_u16).expect("ch->addr");
    assert_eq!(mapped_dest, (caller, callee));
    let p2c = turn.lookup_channel_for_pair_for_tests(&caller, &callee).expect("pair->ch");
    assert_eq!(p2c, ch_u16);
}
