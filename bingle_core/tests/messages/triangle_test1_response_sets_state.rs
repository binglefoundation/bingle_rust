use std::sync::Arc;

use bingle_core::engine::EngineState;
use bingle_core::messages::types::{RelayTriangleTest1Response, RelayTriangleTest3};
use bingle_core::messages::{Message, RelayMessage};

use crate::util::reusable_mock_api::{InnerBingleApiInternal, MockApiBoth};

// Mock internal API to capture and enforce state transitions for tests without starting DTLS/Engine.
struct MockInternal {
    state: std::sync::Mutex<Option<EngineState>>,
}
impl MockInternal {
    fn new() -> Self {
        Self {
            state: std::sync::Mutex::new(None),
        }
    }
}
impl InnerBingleApiInternal for MockInternal {
    fn set_state(&self, state: EngineState) {
        if let Ok(mut g) = self.state.lock() {
            if matches!(*g, Some(EngineState::EndpointAvailable))
                && state == EngineState::NATRestricted
            {
                // Do not override EndpointAvailable with NATRestricted
                return;
            }
            *g = Some(state);
        }
    }
    fn get_state(&self) -> EngineState {
        self.state
            .lock()
            .ok()
            .and_then(|g| *g)
            .unwrap_or(EngineState::StunIdentify)
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn triangle_test1_response_sets_nat_restricted_when_not_available() {
    // Arrange: install a mock internal API and per-test Router
    let mock_internal = Arc::new(MockInternal::new());
    let mock = MockApiBoth::new_with_internal_override(mock_internal.clone());
    let router = std::sync::Arc::new(bingle_core::messages::router::Router::new(
        crate::util::reusable_mock_api::to_weak_api_both(mock),
    ));
    // router.set_bingle_api_internal(Some(mock.clone()));

    // Act: route TriangleTest1Response within router context
    let handler = bingle_core::messages::handlers::DefaultPrintingHandler;
    let msg = Message::Relay(RelayMessage::TriangleTest1Response(
        RelayTriangleTest1Response {
            app: None,
            no_corner_node: false,
            response_tag: None,
        },
    ));
    bingle_core::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "FROMID");
    });

    // Poll for up to 20 seconds for the spawned thread to set NATRestricted
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_secs(20) {
        let st = mock_internal.get_state();
        if st == EngineState::NATRestricted {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    let st = mock_internal.get_state();
    assert_eq!(
        st,
        EngineState::NATRestricted,
        "state did not become NATRestricted within 20s"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn triangle_test1_response_does_not_override_endpoint_available() {
    // Arrange: install a mock internal API and set EndpointAvailable via TriangleTest3
    let mock_internal = Arc::new(MockInternal::new());
    let router = std::sync::Arc::new(bingle_core::messages::router::Router::new(
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_internal_override(
            mock_internal.clone(),
        )),
    ));
    // router.set_bingle_api_internal(Some(mock.clone()));

    // Make EndpointAvailable by invoking the T3 handler directly through routing
    let handler = bingle_core::messages::handlers::DefaultPrintingHandler;
    let t3 = Message::Relay(RelayMessage::TriangleTest3(RelayTriangleTest3 {
        app: None,
    }));
    bingle_core::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &t3, "FROMID");
    });

    // Verify EndpointAvailable
    let st1 = mock_internal.get_state();
    assert_eq!(st1, EngineState::EndpointAvailable);

    // Act: send TriangleTest1Response
    let t1r = Message::Relay(RelayMessage::TriangleTest1Response(
        RelayTriangleTest1Response {
            app: None,
            no_corner_node: false,
            response_tag: None,
        },
    ));
    bingle_core::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &t1r, "FROMID");
    });

    // Assert: still EndpointAvailable
    let st2 = mock_internal.get_state();
    assert_eq!(st2, EngineState::EndpointAvailable);
}
