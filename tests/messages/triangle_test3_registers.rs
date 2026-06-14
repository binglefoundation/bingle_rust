use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};
use std::time::{Duration, Instant};

use crate::util::reusable_mock_api::{InnerBingleApiInternal, MockApiBoth};
use rust_comms::engine::EngineState;
use rust_comms::messages::types::RelayTriangleTest3;
use rust_comms::messages::{Message, RelayMessage};

// Mock internal that simulates registering our discovered IP and records state transitions
struct MockInternal {
    last_public_addr: SocketAddr,
    register_called: AtomicBool,
    set_registered: AtomicBool,
}
impl MockInternal {
    fn new(addr: SocketAddr) -> Self { Self { last_public_addr: addr, register_called: AtomicBool::new(false), set_registered: AtomicBool::new(false) } }
}
impl InnerBingleApiInternal for MockInternal {
    fn set_state(&self, state: EngineState) {
        if state == EngineState::Registered { self.set_registered.store(true, Ordering::SeqCst); }
        // Accept EndpointAvailable too but do not record; test only checks final Registered
    }
    fn get_last_public_addr(&self) -> Option<SocketAddr> { Some(self.last_public_addr) }
    fn ddb_register_ip(&self, endpoint: SocketAddr, am_relay: bool) -> Result<(), rust_comms::api::bingle_api::BingleError> {
        assert_eq!(endpoint, self.last_public_addr, "handler should register the discovered public address");
        assert!(!am_relay, "this test simulates a non-relay");
        self.register_called.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn triangle_test3_triggers_ddb_register_and_sets_registered() {
    // Arrange: router with MockApi and MockInternal
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 45000);
    let internal = Arc::new(MockInternal::new(addr));
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_internal_override(internal.clone()))));
    // router.set_bingle_api_internal(Some(internal.clone() as Arc<dyn rust_comms::api::bingle_api::BingleApiInternal>));

    // Act: route TriangleTest3 through DefaultPrintingHandler
    let handler = rust_comms::messages::handlers::DefaultPrintingHandler;
    let msg = Message::Relay(RelayMessage::TriangleTest3(RelayTriangleTest3 { app: None }));
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "FROMID");
    });

    // Assert: registration was initiated and state eventually set to Registered
    let start = Instant::now();
    let timeout = Duration::from_millis(500);
    while !internal.set_registered.load(Ordering::SeqCst) && start.elapsed() < timeout {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(internal.register_called.load(Ordering::SeqCst), "ddb_register_ip should be called");
    assert!(internal.set_registered.load(Ordering::SeqCst), "engine state should be set to Registered");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn triangle_test3_triggers_relay_registration_sequence() {
    // Mock that records call order
    #[derive(Default)]
    struct RelayMockInternal {
        calls: Mutex<Vec<(String, bool)>>,
        set_registered: AtomicBool,
    }
    impl InnerBingleApiInternal for RelayMockInternal {
        fn is_relay(&self) -> bool { true }
        fn get_last_public_addr(&self) -> Option<SocketAddr> { Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 45002)) }
        fn initialize_relay(&self) { self.calls.lock().unwrap().push(("init".to_string(), false)); }
        fn ddb_register_ip(&self, _ep: SocketAddr, am_relay: bool) -> Result<(), rust_comms::api::bingle_api::BingleError> {
            self.calls.lock().unwrap().push(("register".to_string(), am_relay));
            Ok(())
        }
        fn set_state(&self, state: EngineState) {
            if state == EngineState::Registered { self.set_registered.store(true, Ordering::SeqCst); }
        }
    }

    let internal = Arc::new(RelayMockInternal::default());
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_internal_override(internal.clone()))));

    let handler = rust_comms::messages::handlers::DefaultPrintingHandler;
    let msg = Message::Relay(RelayMessage::TriangleTest3(RelayTriangleTest3 { app: None }));
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "FROMID");
    });

    let start = Instant::now();
    let timeout = Duration::from_secs(2);
    while !internal.set_registered.load(Ordering::SeqCst) && start.elapsed() < timeout {
        std::thread::sleep(Duration::from_millis(10));
    }

    let calls = internal.calls.lock().unwrap().clone();
    // Expected sequence:
    // 1. register(am_relay=false)
    // 2. initialize_relay()
    // 3. register(am_relay=true)
    assert_eq!(calls.len(), 3, "expected 3 calls in sequence");
    assert_eq!(calls[0], ("register".to_string(), false));
    assert_eq!(calls[1], ("init".to_string(), false));
    assert_eq!(calls[2], ("register".to_string(), true));
    assert!(internal.set_registered.load(Ordering::SeqCst));
}
