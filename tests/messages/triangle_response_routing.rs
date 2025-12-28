use std::sync::{Arc, Mutex};

use rust_comms::messages::{Message, RelayMessage};
use rust_comms::messages::handlers::MessageHandler;
use rust_comms::messages::types::RelayTriangleTest1Response;
use rust_comms::api::bingle_api::{BingleApi, StartOptions, Handle, NetworkSourceKey, UserId, ProgressCallback, OnMessageHandler, OnConnectHandler};

struct CapturingHandler { hit: Arc<Mutex<bool>> }
impl CapturingHandler { fn new(hit: Arc<Mutex<bool>>) -> Self { Self { hit } } }

impl MessageHandler for CapturingHandler {
    fn on_triangle_test1_response(&self, _api: Arc<dyn BingleApi>, _from: &rust_comms::messages::handlers::FromStruct, _msg: &RelayTriangleTest1Response) {
        if let Ok(mut g) = self.hit.lock() { *g = true; }
    }
}

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
fn router_dispatches_triangle_test1_response() {
    let hit = Arc::new(Mutex::new(false));
    let handler = CapturingHandler::new(hit.clone());

    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(Arc::new(MockApi)));
    let msg = Message::Relay(RelayMessage::TriangleTest1Response(RelayTriangleTest1Response { app: None }));
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "SENDERID");
    });

    assert_eq!(*hit.lock().unwrap(), true);
}
