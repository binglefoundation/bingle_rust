use std::sync::{Arc, Mutex};

use dtls_pki::messages::{route, Message, RelayMessage};
use dtls_pki::messages::handlers::MessageHandler;
use dtls_pki::messages::types::{RelayTriangleTest1};

struct CapturingHandler {
    last_from_id: Arc<Mutex<Option<String>>>,
}

impl CapturingHandler {
    fn new(store: Arc<Mutex<Option<String>>>) -> Self { Self { last_from_id: store } }
}

impl MessageHandler for CapturingHandler {
    fn on_triangle_test1(&self, from_id: &str, _msg: &RelayTriangleTest1) {
        if let Ok(mut g) = self.last_from_id.lock() { *g = Some(from_id.to_string()); }
    }
}

#[test]
fn route_passes_from_id_into_handler() {
    let store: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let handler = CapturingHandler::new(store.clone());
    let msg = Message::Relay(RelayMessage::TriangleTest1(RelayTriangleTest1 { app: None, checkingEndpoint: "127.0.0.1:5000".parse().unwrap() }));
    route(&handler, &msg, "ALGOADDR123");
    let got = store.lock().unwrap().clone();
    assert_eq!(got.as_deref(), Some("ALGOADDR123"));
}
