#![cfg(not(target_os = "ios"))]

use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

use rust_comms::messages::handlers::MessageHandler;
use rust_comms::messages::types::{Message, PlainTextMessage};
use rust_comms::api::bingle_api::{BingleApi, BingleApiBoth, StartOptions, Handle, NetworkEndpoint, UserId, ProgressCallback, OnMessageHandler, OnConnectHandler};

struct CapturingHandler {
    called: &'static AtomicBool,
    sink: Arc<Mutex<Option<serde_json::Value>>>,
}

impl MessageHandler for CapturingHandler {
    fn on_plain_text(&self, _api: Arc<dyn BingleApiBoth>, from: &rust_comms::messages::handlers::FromStruct, msg: &PlainTextMessage) {
        assert_eq!(from.id, "from-handle");
        let json = serde_json::to_value(msg).unwrap_or_else(|_| serde_json::json!({"text": msg.text.clone()}));
        self.called.store(true, Ordering::SeqCst);
        *self.sink.lock().unwrap() = Some(json);
    }
}

// Minimal API impl to satisfy router requirements
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
fn on_plain_text_calls_handler_implementation() {
    static CALLED: AtomicBool = AtomicBool::new(false);
    let received = Arc::new(Mutex::new(None::<serde_json::Value>));

    // Provide a per-test Router with our MockApi and run the route within its context
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(Arc::new(MockApi)));

    // Build a PlainText message and route it through a custom handler implementation
    let pt = PlainTextMessage { text: "Hello".to_string(), app: None, r#type: None };
    let msg = Message::PlainText(pt.clone());

    let handler = CapturingHandler { called: &CALLED, sink: received.clone() };
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "from-handle");
    });

    assert!(CALLED.load(Ordering::SeqCst), "handler was not called by on_plain_text");

    let got = received.lock().unwrap().clone().expect("no payload captured");
    // Expect the JSON to include the text field
    assert_eq!(got.get("text").and_then(|v| v.as_str()), Some("Hello"));
}
