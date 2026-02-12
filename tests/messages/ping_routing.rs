use rust_comms::messages::Router;
use std::sync::{Arc, Mutex};

use rust_comms::messages::{Message};
use rust_comms::messages::handlers::MessageHandler;
use rust_comms::messages::types::{PingMessage, PingPing};

use rust_comms::api::bingle_api::{BingleApi, BingleApiBoth, StartOptions, Handle, NetworkEndpoint, UserId, ProgressCallback, OnMessageHandler, OnConnectHandler};

struct CapturingHandler {
    called: Arc<Mutex<bool>>,
}

impl CapturingHandler { fn new(flag: Arc<Mutex<bool>>) -> Self { Self { called: flag } } }

impl MessageHandler for CapturingHandler {
    fn on_ping_ping(&self, _api: Arc<dyn BingleApiBoth>, _from: &rust_comms::messages::handlers::FromStruct, msg: &PingPing) {
        // Ensure we received the ping message with expected fields
        assert_eq!(msg.app, "ping");
        assert_eq!(msg.text.as_deref(), Some("hello"));
        if let Ok(mut g) = self.called.lock() { *g = true; }
    }
}

// Minimal API impl so router can pass an API into the handler
struct MockApi;
impl BingleApi for MockApi { 
    fn set_on_listening(&mut self, _handler: Option<std::sync::Arc<rust_comms::api::bingle_api::OnListeningHandler>>) {} 
    fn get_algo_provider_config(&self) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> { None } 
    fn get_handle(&self) -> Option<String> { None } 
    fn get_user_id(&self) -> Option<String> { None } 
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> { None }
    fn get_app_id(&self) -> Option<u64> { None }
    fn start(&mut self, _options: &StartOptions) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}
    fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _network_source_key: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_network_with_response(&self, _network_source_key: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
}

#[test]
fn route_invokes_on_ping_ping() {
    let flag = Arc::new(Mutex::new(false));
    let handler = CapturingHandler::new(flag.clone());

    if let Some(router) = Router::current() {
        // Provide API to router so it can be passed into handler per new signature
        router.set_bingle_api(Some(Arc::new(MockApi)));

        let ping = PingPing { app: "ping".into(), tag: None, response_tag: None, text: Some("hello".into()), data: None };
        let msg = Message::Ping(PingMessage::Ping(ping));
        router.route(&handler, &msg, "SOMEISSUER.");

        let got = flag.lock().unwrap().clone();
        assert!(got, "on_ping_ping was not called");
    }
}
