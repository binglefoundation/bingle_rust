use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use crate::util::reusable_mock_api::{to_weak_api_both, InnerBingleApiInternal, MockApiBoth};
use rust_comms::messages::handlers::DefaultPrintingHandler;
use rust_comms::messages::types::RelayListen;
use rust_comms::messages::{Message, RelayMessage};
use rust_comms::turn::turn_handler::TurnHandler;

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

#[cfg_attr(not(target_os = "ios"), test)]
pub fn relay_listen_registers_and_responds() {
    // Arrange: a Router configured as a relay with a TURN handler
    let turn = std::sync::Arc::new(rust_comms::turn::turn_handler::TurnHandlerImpl::new());
    // Provide internal API that exposes the shared TurnHandlerImpl
    struct MockInternal { pub turn: std::sync::Arc<rust_comms::turn::turn_handler::TurnHandlerImpl> }
    impl InnerBingleApiInternal for MockInternal {
        fn turn_lookup_addr_by_id(&self, id: std::string::String) -> Option<std::net::SocketAddr> { self.turn.lookup_addr_by_id(&id) }
        fn turn_handle_call(&self, source_id: String, dest_id: String, source: std::net::SocketAddr, dest: std::net::SocketAddr) -> i32 { rust_comms::turn::turn_handler::TurnRelayHandler::handle_call(&*self.turn, &source_id, &dest_id, &source, &dest) }
        fn turn_handle_listen(&self, id: std::string::String, source: std::net::SocketAddr) -> bool { self.turn.handle_listen(&id, &source) }
    }
    let internal = Arc::new(MockInternal { turn: turn.clone() });
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(to_weak_api_both(MockApiBoth::new_with_internal_override(internal))));
    router.set_am_relay(true);
    let source = addr(9001);
    router.set_last_from(Some(source));

    // Act: route a Relay::Listen message via DefaultPrintingHandler
    let handler = DefaultPrintingHandler;
    let msg = Message::Relay(RelayMessage::Listen(RelayListen { app: None, tag: None }));
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        // from_id not used in this path
        router.route(&handler, &msg, "ALGOADDR123");
    });

    // Assert response was produced and source IP registered
    let out = router.take_outbound_response();
    assert!(out.is_some(), "expected an outbound response");
    let obj = out.unwrap();
    let t = obj.get("type").and_then(|v: &serde_json::Value| v.as_str());
    assert_eq!(t, Some("ListenResponse"));

    // id->addr map should contain the from.id (issuer trimmed) -> source address
    assert_eq!(turn.lookup_addr_by_id("ALGOADDR123"), Some(source));
}
