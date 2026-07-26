use std::sync::Arc;

use bingle_core::engine::EngineState;
use bingle_core::messages::marshal::{from_json_str, to_json_value};
use bingle_core::messages::types::{
    Message, RelayMessage, RelayTriangleTest1Response, RelayTriangleTest3,
};

use crate::util::reusable_mock_api::{InnerBingleApiInternal, MockApiBoth};

// Mock internal API that tracks state and nat_type changes.
struct MockInternal {
    state: std::sync::Mutex<Option<EngineState>>,
    nat_type: std::sync::Mutex<Option<bingle_core::engine::NatType>>,
}
impl MockInternal {
    fn new() -> Self {
        Self {
            state: std::sync::Mutex::new(None),
            nat_type: std::sync::Mutex::new(None),
        }
    }
}
impl InnerBingleApiInternal for MockInternal {
    fn set_state(&self, state: EngineState) {
        if let Ok(mut g) = self.state.lock() {
            if matches!(*g, Some(EngineState::EndpointAvailable))
                && state == EngineState::NATRestricted
            {
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
    fn set_nat_type(&self, nat_type: bingle_core::engine::NatType) {
        if let Ok(mut g) = self.nat_type.lock() {
            *g = Some(nat_type);
        }
    }
    fn get_nat_type(&self) -> bingle_core::engine::NatType {
        self.nat_type
            .lock()
            .ok()
            .and_then(|g| *g)
            .unwrap_or(bingle_core::engine::NatType::Unknown)
    }
}

/// Serialization: no_corner_node=false serializes as false.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_corner_node_false_serializes_as_false() {
    let resp = RelayTriangleTest1Response {
        app: None,
        no_corner_node: false,
        response_tag: None,
    };
    let msg = Message::Relay(RelayMessage::TriangleTest1Response(resp));
    let json = to_json_value(&msg);
    let json_str = serde_json::to_string(&json).expect("serialize");
    assert!(
        json_str.contains("\"noCornerNode\":false"),
        "noCornerNode should be false: {}",
        json_str
    );
}

/// Serialization: no_corner_node=true appears in JSON.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_corner_node_true_serializes_with_field() {
    let resp = RelayTriangleTest1Response {
        app: None,
        no_corner_node: true,
        response_tag: None,
    };
    let msg = Message::Relay(RelayMessage::TriangleTest1Response(resp));
    let json = to_json_value(&msg);
    let json_str = serde_json::to_string(&json).expect("serialize");
    assert!(
        json_str.contains("\"noCornerNode\":true"),
        "noCornerNode should be true: {}",
        json_str
    );
}

/// Deserialization: missing noCornerNode defaults to false.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_corner_node_defaults_to_false_on_deserialize() {
    let json = r#"{"app":null,"type":"TriangleTest1Response"}"#;
    let msg = from_json_str(json).expect("decode");
    match msg {
        Message::Relay(RelayMessage::TriangleTest1Response(resp)) => {
            assert!(
                !resp.no_corner_node,
                "no_corner_node should default to false"
            );
        }
        other => panic!("expected TriangleTest1Response, got {:?}", other),
    }
}

/// Deserialization: noCornerNode=true round-trips correctly.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_corner_node_true_round_trips() {
    let json = r#"{"app":null,"type":"TriangleTest1Response","noCornerNode":true}"#;
    let msg = from_json_str(json).expect("decode");
    match msg {
        Message::Relay(RelayMessage::TriangleTest1Response(resp)) => {
            assert!(resp.no_corner_node, "no_corner_node should be true");
        }
        other => panic!("expected TriangleTest1Response, got {:?}", other),
    }
}

/// When no_corner_node=true, state is set to NATRestricted immediately (no 10s delay).
#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_corner_node_sets_nat_restricted_immediately() {
    let mock_internal = Arc::new(MockInternal::new());
    let mock = MockApiBoth::new_with_internal_override(mock_internal.clone());
    let router = std::sync::Arc::new(bingle_core::messages::router::Router::new(
        crate::util::reusable_mock_api::to_weak_api_both(mock),
    ));

    let handler = bingle_core::messages::handlers::DefaultPrintingHandler;
    let msg = Message::Relay(RelayMessage::TriangleTest1Response(
        RelayTriangleTest1Response {
            app: None,
            no_corner_node: true,
            response_tag: None,
        },
    ));
    bingle_core::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "FROMID");
    });

    // The no_corner_node path spawns a thread (without the 10s delay) that sets the state.
    // Poll briefly for the state to be set.
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_secs(5) {
        if mock_internal.get_state() == EngineState::NATRestricted {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let st = mock_internal.get_state();
    assert_eq!(
        st,
        EngineState::NATRestricted,
        "state should be NATRestricted shortly after no_corner_node=true (no 10s delay)"
    );

    let nt = mock_internal.nat_type.lock().unwrap();
    assert_eq!(
        *nt,
        Some(bingle_core::engine::NatType::Restricted),
        "nat_type should be Restricted"
    );
}

/// Regression for #41: a stale `Registered` state left over from before a STUN
/// public-address change reset the node (nat_type back to Unknown) must NOT suppress the
/// no-corner fallback. With state=Registered but nat_type=Unknown, no_corner_node=true should
/// still re-classify to NATRestricted (rather than staying Unknown forever).
#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_corner_node_reclassifies_when_registered_but_nat_unknown() {
    let mock_internal = Arc::new(MockInternal::new());
    // Simulate the stale post-reset condition: Registered flag still set, but NAT is Unknown.
    mock_internal.set_state(EngineState::Registered);
    assert_eq!(mock_internal.get_state(), EngineState::Registered);
    assert_eq!(
        mock_internal.get_nat_type(),
        bingle_core::engine::NatType::Unknown
    );

    let mock = MockApiBoth::new_with_internal_override(mock_internal.clone());
    let router = std::sync::Arc::new(bingle_core::messages::router::Router::new(
        crate::util::reusable_mock_api::to_weak_api_both(mock),
    ));
    let handler = bingle_core::messages::handlers::DefaultPrintingHandler;
    let msg = Message::Relay(RelayMessage::TriangleTest1Response(
        RelayTriangleTest1Response {
            app: None,
            no_corner_node: true,
            response_tag: None,
        },
    ));
    bingle_core::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "FROMID");
    });

    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_secs(5) {
        if mock_internal.get_state() == EngineState::NATRestricted {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    assert_eq!(
        mock_internal.get_state(),
        EngineState::NATRestricted,
        "stale Registered + Unknown NAT must re-classify to NATRestricted (not stay Unknown)"
    );
    assert_eq!(
        *mock_internal.nat_type.lock().unwrap(),
        Some(bingle_core::engine::NatType::Restricted),
        "nat_type should be re-classified to Restricted"
    );
}

/// When the node is genuinely settled (Registered with a concrete NAT type), a late
/// no_corner_node=true must NOT downgrade it.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_corner_node_does_not_downgrade_registered_with_concrete_nat() {
    let mock_internal = Arc::new(MockInternal::new());
    mock_internal.set_state(EngineState::Registered);
    mock_internal.set_nat_type(bingle_core::engine::NatType::Restricted);

    let mock = MockApiBoth::new_with_internal_override(mock_internal.clone());
    let router = std::sync::Arc::new(bingle_core::messages::router::Router::new(
        crate::util::reusable_mock_api::to_weak_api_both(mock),
    ));
    let handler = bingle_core::messages::handlers::DefaultPrintingHandler;
    let msg = Message::Relay(RelayMessage::TriangleTest1Response(
        RelayTriangleTest1Response {
            app: None,
            no_corner_node: true,
            response_tag: None,
        },
    ));
    bingle_core::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "FROMID");
    });

    // Give the spawned handler thread time to run (and confirm it does not change state).
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert_eq!(
        mock_internal.get_state(),
        EngineState::Registered,
        "a settled Registered node with concrete NAT should not be downgraded"
    );
}

/// When no_corner_node=true but state is already EndpointAvailable, do not override.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_corner_node_does_not_override_endpoint_available() {
    let mock_internal = Arc::new(MockInternal::new());
    let mock = MockApiBoth::new_with_internal_override(mock_internal.clone());
    let router = std::sync::Arc::new(bingle_core::messages::router::Router::new(
        crate::util::reusable_mock_api::to_weak_api_both(mock),
    ));

    // First set EndpointAvailable via TriangleTest3
    let handler = bingle_core::messages::handlers::DefaultPrintingHandler;
    let t3 = Message::Relay(RelayMessage::TriangleTest3(RelayTriangleTest3 {
        app: None,
    }));
    bingle_core::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &t3, "FROMID");
    });
    assert_eq!(mock_internal.get_state(), EngineState::EndpointAvailable);

    // Now send no_corner_node=true — should NOT override EndpointAvailable
    let msg = Message::Relay(RelayMessage::TriangleTest1Response(
        RelayTriangleTest1Response {
            app: None,
            no_corner_node: true,
            response_tag: None,
        },
    ));
    bingle_core::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "FROMID");
    });

    let st = mock_internal.get_state();
    assert_eq!(
        st,
        EngineState::EndpointAvailable,
        "EndpointAvailable should not be overridden"
    );
}
