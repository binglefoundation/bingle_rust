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

// Minimal BingleApi that exposes internal interface via router and allows inspecting engine state via BingleApiImpl
struct DelegatingApi {
    inner: Arc<BingleApiImpl>,
}

impl DelegatingApi {
    fn new() -> Self {
        let api = BingleApiImpl::new();
        Self { inner: Arc::new(api) }
    }
}

impl BingleApi for DelegatingApi {
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> { None }
    fn get_app_id(&self) -> Option<u64> { None }
    fn start(&mut self, _options: StartOptions) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}
    fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _network_source_key: &NetworkSourceKey, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_network_with_response(&self, _network_source_key: &NetworkSourceKey, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
}

impl BingleApiInternal for DelegatingApi {
    fn set_state(&self, state: EngineState) {
        // Forward to the inner BingleApiImpl
        self.inner.set_state(state);
    }
}

#[test]
fn triangle_test1_response_sets_nat_restricted_when_not_available() {
    // Arrange: set up API and router bindings
    let mut deleg = DelegatingApi::new();
    set_bingle_api(Some(deleg.inner.clone()));
    set_bingle_api_internal(Some(deleg.inner.clone()));

    // Start the API minimally so Engine is created
    deleg.start(StartOptions::default()).expect("start ok");

    // Sanity: initial engine state should not be EndpointAvailable
    let initial = deleg.inner.engine_state_for_tests().unwrap_or(EngineState::StunIdentify);
    assert!(initial != EngineState::EndpointAvailable);

    // Act: route TriangleTest1Response
    let handler = rust_comms::messages::handlers::DefaultPrintingHandler;
    let msg = Message::Relay(RelayMessage::TriangleTest1Response(RelayTriangleTest1Response { app: None }));
    route(&handler, &msg, "FROMID");

    // Assert: state becomes NATRestricted
    let st = deleg.inner.engine_state_for_tests().expect("have engine state");
    assert_eq!(st, EngineState::NATRestricted);
}

#[test]
fn triangle_test1_response_does_not_override_endpoint_available() {
    // Arrange: use BingleApiImpl and force EndpointAvailable via TriangleTest3 handler call
    let mut api = BingleApiImpl::new();
    // Start and set router
    api.start(StartOptions::default()).expect("start ok");
    let api_arc = Arc::new(api);
    set_bingle_api(Some(api_arc.clone()));
    set_bingle_api_internal(Some(api_arc.clone()));

    // Make EndpointAvailable by invoking the T3 handler directly through routing
    let handler = rust_comms::messages::handlers::DefaultPrintingHandler;
    let t3 = Message::Relay(RelayMessage::TriangleTest3(RelayTriangleTest3 { app: None }));
    route(&handler, &t3, "FROMID");

    // Verify EndpointAvailable
    let st1 = api_arc.engine_state_for_tests().expect("state");
    assert_eq!(st1, EngineState::EndpointAvailable);

    // Act: send TriangleTest1Response
    let t1r = Message::Relay(RelayMessage::TriangleTest1Response(RelayTriangleTest1Response { app: None }));
    route(&handler, &t1r, "FROMID");

    // Assert: still EndpointAvailable
    let st2 = api_arc.engine_state_for_tests().expect("state");
    assert_eq!(st2, EngineState::EndpointAvailable);
}
