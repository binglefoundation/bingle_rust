use std::sync::{Arc, Mutex};

use crate::util::reusable_mock_api::MockApiBoth;
use bingle_core::api::bingle_api::BingleApiBoth;
use bingle_core::messages::handlers::MessageHandler;
use bingle_core::messages::types::RelayTriangleTest1;
use bingle_core::messages::{Message, RelayMessage};

struct CapturingHandler {
    last_from_id: Arc<Mutex<Option<String>>>,
}

impl CapturingHandler {
    fn new(store: Arc<Mutex<Option<String>>>) -> Self {
        Self {
            last_from_id: store,
        }
    }
}

impl MessageHandler for CapturingHandler {
    fn on_triangle_test1(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        from: &bingle_core::messages::handlers::FromStruct,
        _msg: &RelayTriangleTest1,
    ) {
        if let Ok(mut g) = self.last_from_id.lock() {
            *g = Some(from.id.to_string());
        }
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn route_passes_from_id_into_handler() {
    let store: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let handler = CapturingHandler::new(store.clone());

    // Provide a per-test Router with MockApi and route within its context
    let router = std::sync::Arc::new(bingle_core::messages::router::Router::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new()),
    ));
    let msg = Message::Relay(RelayMessage::TriangleTest1(RelayTriangleTest1 {
        app: None,
        checking_endpoint: "127.0.0.1:5000".parse().unwrap(),
        do_not_use_endpoints: Vec::new(),
        tag: None,
    }));
    bingle_core::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "ALGOADDR123");
    });
    let got = store.lock().unwrap().clone();
    assert_eq!(got.as_deref(), Some("ALGOADDR123"));
}
