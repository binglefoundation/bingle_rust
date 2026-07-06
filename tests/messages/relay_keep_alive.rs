// Tests for the relay-side Relay::KeepAlive handler: it must refresh the
// client's TURN listener mapping and send no response.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use crate::util::reusable_mock_api::{InnerBingleApiInternal, MockApiBoth, to_weak_api_both};
use rust_comms::messages::handlers::DefaultPrintingHandler;
use rust_comms::messages::types::RelayKeepAlive;
use rust_comms::messages::{Message, RelayMessage};
use rust_comms::turn::turn_handler::TurnHandler;

fn addr(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

struct MockInternal {
    pub turn: std::sync::Arc<rust_comms::turn::turn_handler::TurnHandlerImpl>,
}
impl InnerBingleApiInternal for MockInternal {
    fn turn_lookup_addr_by_id(&self, id: String) -> Option<std::net::SocketAddr> {
        self.turn.lookup_addr_by_id(&id)
    }
    fn turn_handle_listen(&self, id: String, source: std::net::SocketAddr) -> bool {
        self.turn.handle_listen(&id, &source)
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_keep_alive_refreshes_mapping_without_response() {
    // Arrange: a Router configured as a relay with a TURN handler
    let turn = std::sync::Arc::new(rust_comms::turn::turn_handler::TurnHandlerImpl::new());
    let internal = Arc::new(MockInternal { turn: turn.clone() });
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(to_weak_api_both(
        MockApiBoth::new_with_internal_override(internal),
    )));
    router.set_am_relay(true);
    // Simulate a NAT rebind: the keep-alive arrives from a new source port
    let source = addr(9101);
    router.set_last_from(Some(source));

    // Act: route a Relay::KeepAlive message via DefaultPrintingHandler
    let handler = DefaultPrintingHandler;
    let msg = Message::Relay(RelayMessage::KeepAlive(RelayKeepAlive { app: None }));
    let responses =
        rust_comms::messages::router::Router::with_current_router(router.clone(), || {
            router.route(&handler, &msg, "ALGOADDR123")
        });

    // Assert: fire-and-forget (no response) and the mapping now points at the source
    assert!(
        responses.is_empty(),
        "KeepAlive must not produce a response, got {:?}",
        responses
    );
    assert_eq!(turn.lookup_addr_by_id("ALGOADDR123"), Some(source));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_keep_alive_ignored_when_not_a_relay() {
    let turn = std::sync::Arc::new(rust_comms::turn::turn_handler::TurnHandlerImpl::new());
    let internal = Arc::new(MockInternal { turn: turn.clone() });
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(to_weak_api_both(
        MockApiBoth::new_with_internal_override(internal),
    )));
    router.set_am_relay(false);
    let source = addr(9102);
    router.set_last_from(Some(source));

    let handler = DefaultPrintingHandler;
    let msg = Message::Relay(RelayMessage::KeepAlive(RelayKeepAlive { app: None }));
    let responses =
        rust_comms::messages::router::Router::with_current_router(router.clone(), || {
            router.route(&handler, &msg, "ALGOADDR123")
        });

    assert!(responses.is_empty());
    assert_eq!(
        turn.lookup_addr_by_id("ALGOADDR123"),
        None,
        "non-relay node must not register a mapping"
    );
}
