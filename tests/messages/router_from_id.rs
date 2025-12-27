use std::sync::{Arc, Mutex};

use rust_comms::messages::{Message, RelayMessage};
use rust_comms::messages::handlers::MessageHandler;
use rust_comms::messages::types::RelayTriangleTest1;
use rust_comms::api::bingle_api::{BingleApi, StartOptions, Handle, NetworkSourceKey, UserId, ProgressCallback, OnMessageHandler, OnConnectHandler};

struct CapturingHandler {
    last_from_id: Arc<Mutex<Option<String>>>,
}

impl CapturingHandler {
    fn new(store: Arc<Mutex<Option<String>>>) -> Self { Self { last_from_id: store } }
}

impl MessageHandler for CapturingHandler {
    fn on_triangle_test1(&self, _api: Arc<dyn BingleApi>, from: &rust_comms::messages::handlers::FromStruct, _msg: &RelayTriangleTest1) {
        if let Ok(mut g) = self.last_from_id.lock() { *g = Some(from.id.to_string()); }
    }
}

// Minimal API impl so router can pass an API into the handler
struct MockApi;
impl BingleApi for MockApi {
    fn get_my_id(&self) -> Option<String> { None }
    fn get_app_id(&self) -> Option<u64> { None }
    fn start(&mut self, _options: &StartOptions) -> Result<(), String> { Ok(()) }
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

#[test]
fn route_passes_from_id_into_handler() {
    let store: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let handler = CapturingHandler::new(store.clone());

    // Provide a per-test Router with MockApi and route within its context
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(Arc::new(MockApi)));
    let msg = Message::Relay(RelayMessage::TriangleTest1(RelayTriangleTest1 { app: None, checking_endpoint: "127.0.0.1:5000".parse().unwrap() }));
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "ALGOADDR123");
    });
    let got = store.lock().unwrap().clone();
    assert_eq!(got.as_deref(), Some("ALGOADDR123"));
}
