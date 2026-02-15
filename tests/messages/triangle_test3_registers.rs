use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
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
    fn ddb_register_ip(&self, endpoint: SocketAddr) -> Result<(), String> {
        assert_eq!(endpoint, self.last_public_addr, "handler should register the discovered public address");
        self.register_called.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[test]
fn triangle_test3_triggers_ddb_register_and_sets_registered() {
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
