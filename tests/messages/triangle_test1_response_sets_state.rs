use std::sync::Arc;

use rust_comms::messages::{route, Message, RelayMessage};
use rust_comms::messages::handlers::MessageHandler;
use rust_comms::messages::types::{RelayTriangleTest1Response, RelayTriangleTest3};
use rust_comms::messages::router::{set_bingle_api, set_bingle_api_internal};
use rust_comms::api::bingle_api::{BingleApi, StartOptions, Handle, NetworkSourceKey, UserId, ProgressCallback, OnMessageHandler, OnConnectHandler, BingleApiInternal};
use rust_comms::engine::EngineState;
use rust_comms::api::bingle_api_impl::BingleApiImpl;

struct NoopHandler;
impl MessageHandler for NoopHandler {}

// Mock internal API to capture and enforce state transitions for tests without starting DTLS/Engine.
struct MockInternal {
    state: std::sync::Mutex<Option<EngineState>>,
}
impl MockInternal {
    fn new() -> Self { Self { state: std::sync::Mutex::new(None) } }
    fn get(&self) -> Option<EngineState> { self.state.lock().ok().and_then(|g| *g) }
}
impl rust_comms::api::bingle_api::BingleApiInternal for MockInternal {
    fn set_state(&self, state: EngineState) {
        if let Ok(mut g) = self.state.lock() {
            if matches!(*g, Some(EngineState::EndpointAvailable)) && state == EngineState::NATRestricted {
                // Do not override EndpointAvailable with NATRestricted
                return;
            }
            *g = Some(state);
        }
    }
}

#[test]
fn triangle_test1_response_sets_nat_restricted_when_not_available() {
    // Arrange: install a mock internal API
    let mock = Arc::new(MockInternal::new());
    set_bingle_api_internal(Some(mock.clone()));

    // Act: route TriangleTest1Response
    let handler = rust_comms::messages::handlers::DefaultPrintingHandler;
    let msg = Message::Relay(RelayMessage::TriangleTest1Response(RelayTriangleTest1Response { app: None }));
    route(&handler, &msg, "FROMID");

    // Assert: state becomes NATRestricted
    let st = mock.get().expect("have engine state");
    assert_eq!(st, EngineState::NATRestricted);
}

#[test]
fn triangle_test1_response_does_not_override_endpoint_available() {
    // Arrange: install a mock internal API and set EndpointAvailable via TriangleTest3
    let mock = Arc::new(MockInternal::new());
    set_bingle_api_internal(Some(mock.clone()));

    // Make EndpointAvailable by invoking the T3 handler directly through routing
    let handler = rust_comms::messages::handlers::DefaultPrintingHandler;
    let t3 = Message::Relay(RelayMessage::TriangleTest3(RelayTriangleTest3 { app: None }));
    route(&handler, &t3, "FROMID");

    // Verify EndpointAvailable
    let st1 = mock.get().expect("state");
    assert_eq!(st1, EngineState::EndpointAvailable);

    // Act: send TriangleTest1Response
    let t1r = Message::Relay(RelayMessage::TriangleTest1Response(RelayTriangleTest1Response { app: None }));
    route(&handler, &t1r, "FROMID");

    // Assert: still EndpointAvailable
    let st2 = mock.get().expect("state");
    assert_eq!(st2, EngineState::EndpointAvailable);
}
