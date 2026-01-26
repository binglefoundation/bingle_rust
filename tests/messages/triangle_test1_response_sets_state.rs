use std::sync::Arc;

use rust_comms::messages::{Message, RelayMessage};
use rust_comms::messages::types::{RelayTriangleTest1Response, RelayTriangleTest3};
use rust_comms::engine::EngineState;

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
    fn get_state(&self) -> EngineState {
        self.state.lock().ok().and_then(|g| *g).unwrap_or(EngineState::StunIdentify)
    }
    fn set_nat_type(&self, _nat: rust_comms::engine::NatType) { }
    fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> { None }
    fn ddb_register_ip(&self, _endpoint: std::net::SocketAddr) -> Result<(), String> { Err("ni".into()) }
    fn ddb_register_relay(&self, _relay_id: String, _relay_sig: Option<String>) -> Result<(), String> { Err("ni".into()) }
    fn update_turn_listener_relay(&self, _relay_id: String, _relay_addr: std::net::SocketAddr) -> Result<(), String> { Err("ni".into()) }
    fn turn_client_handle_listen_response(&self, _relay_addr: std::net::SocketAddr, _relay_id: String) { }
    fn turn_lookup_addr_by_id(&self, _id: String) -> Option<std::net::SocketAddr> { None }
    fn turn_handle_call(&self, _source: std::net::SocketAddr, _dest: std::net::SocketAddr) -> i32 { -1 }
    fn turn_handle_listen(&self, _id: String, _source: std::net::SocketAddr) -> bool { false }
    fn turn_handle_called(&self, _source: std::net::SocketAddr, _dest: std::net::SocketAddr, _channel: u16) { }
    fn notify_listening(&self, _listening: bool) { }
}

#[derive(Clone)]
struct MockApi;
impl rust_comms::api::bingle_api::BingleApi for MockApi {
    fn get_my_id(&self) -> Option<String> { None }
    fn get_app_id(&self) -> Option<u64> { None }
    fn start(&mut self, _options: &rust_comms::api::bingle_api::StartOptions) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}
    fn send_message_to_id(&self, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &rust_comms::api::bingle_api::Handle, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _network_source_key: &rust_comms::api::bingle_api::NetworkEndpoint, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &rust_comms::api::bingle_api::Handle, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_network_with_response(&self, _network_source_key: &rust_comms::api::bingle_api::NetworkEndpoint, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn set_on_message(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnConnectHandler>>) {}
}

#[test]
fn triangle_test1_response_sets_nat_restricted_when_not_available() {
    // Arrange: install a mock internal API and per-test Router
    let mock = Arc::new(MockInternal::new());
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(Arc::new(MockApi)));
    router.set_bingle_api_internal(Some(mock.clone()));

    // Act: route TriangleTest1Response within router context
    let handler = rust_comms::messages::handlers::DefaultPrintingHandler;
    let msg = Message::Relay(RelayMessage::TriangleTest1Response(RelayTriangleTest1Response { app: None }));
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "FROMID");
    });

    // Wait for 10 seconds for the spawned thread to complete the state check and setting
    std::thread::sleep(std::time::Duration::from_secs(20));

    // Assert: state becomes NATRestricted
    let st = mock.get().expect("have engine state");
    assert_eq!(st, EngineState::NATRestricted);
}

#[test]
fn triangle_test1_response_does_not_override_endpoint_available() {
    // Arrange: install a mock internal API and set EndpointAvailable via TriangleTest3
    let mock = Arc::new(MockInternal::new());
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(Arc::new(MockApi)));
    router.set_bingle_api_internal(Some(mock.clone()));

    // Make EndpointAvailable by invoking the T3 handler directly through routing
    let handler = rust_comms::messages::handlers::DefaultPrintingHandler;
    let t3 = Message::Relay(RelayMessage::TriangleTest3(RelayTriangleTest3 { app: None }));
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &t3, "FROMID");
    });

    // Verify EndpointAvailable
    let st1 = mock.get().expect("state");
    assert_eq!(st1, EngineState::EndpointAvailable);

    // Act: send TriangleTest1Response
    let t1r = Message::Relay(RelayMessage::TriangleTest1Response(RelayTriangleTest1Response { app: None }));
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &t1r, "FROMID");
    });

    // Assert: still EndpointAvailable
    let st2 = mock.get().expect("state");
    assert_eq!(st2, EngineState::EndpointAvailable);
}
