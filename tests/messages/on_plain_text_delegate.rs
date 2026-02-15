#![cfg(not(target_os = "ios"))]

use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};

use crate::util::reusable_mock_api::MockApiBoth;
use rust_comms::api::bingle_api::{BingleApiBoth};
use rust_comms::messages::handlers::MessageHandler;
use rust_comms::messages::types::{Message, PlainTextMessage};

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

#[test]
fn on_plain_text_calls_handler_implementation() {
    static CALLED: AtomicBool = AtomicBool::new(false);
    let received = Arc::new(Mutex::new(None::<serde_json::Value>));

    // Provide a per-test Router with our MockApi and run the route within its context
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new())));

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
    assert_eq!(got.get("text").and_then(|v: &serde_json::Value| v.as_str()), Some("Hello"));
}
