use std::sync::{Arc, Mutex};

use crate::util::reusable_mock_api::MockApiBoth;
use bingle_core::api::bingle_api::BingleApiBoth;
use bingle_core::messages::handlers::MessageHandler;
use bingle_core::messages::types::RelayTriangleTest1Response;
use bingle_core::messages::{Message, RelayMessage};

struct CapturingHandler {
    hit: Arc<Mutex<bool>>,
}
impl CapturingHandler {
    fn new(hit: Arc<Mutex<bool>>) -> Self {
        Self { hit }
    }
}

impl MessageHandler for CapturingHandler {
    fn on_triangle_test1_response(
        &self,
        _api: Arc<dyn BingleApiBoth>,
        _from: &bingle_core::messages::handlers::FromStruct,
        _msg: &RelayTriangleTest1Response,
    ) {
        if let Ok(mut g) = self.hit.lock() {
            *g = true;
        }
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn router_dispatches_triangle_test1_response() {
    let hit = Arc::new(Mutex::new(false));
    let handler = CapturingHandler::new(hit.clone());

    let router = std::sync::Arc::new(bingle_core::messages::router::Router::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new()),
    ));
    let msg = Message::Relay(RelayMessage::TriangleTest1Response(
        RelayTriangleTest1Response {
            app: None,
            no_corner_node: false,
            response_tag: None,
        },
    ));
    bingle_core::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "SENDERID");
    });

    assert_eq!(*hit.lock().unwrap(), true);
}
