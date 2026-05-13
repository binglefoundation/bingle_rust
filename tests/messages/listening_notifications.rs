use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
use std::time::{Duration, Instant};

use crate::util::reusable_mock_api::{InnerBingleApiInternal, MockApiBoth};
use rust_comms::engine::EngineState;
use rust_comms::messages::types::RelayTriangleTest3;
use rust_comms::messages::{Message, RelayMessage};

// Mock internal similar to triangle_test3_registers but records notify_listening
struct MockInternal {
    last_public_addr: SocketAddr,
    register_called: AtomicBool,
    listening_notified: AtomicBool,
}
impl MockInternal {
    fn new(addr: SocketAddr) -> Self { Self { last_public_addr: addr, register_called: AtomicBool::new(false), listening_notified: AtomicBool::new(false) } }
}
impl InnerBingleApiInternal for MockInternal {
    fn get_state(&self) -> EngineState { EngineState::StunIdentify }
    fn get_last_public_addr(&self) -> Option<SocketAddr> { Some(self.last_public_addr) }
    fn ddb_register_ip(&self, endpoint: SocketAddr, _am_relay: bool) -> Result<(), rust_comms::api::bingle_api::BingleError> {
        assert_eq!(endpoint, self.last_public_addr);
        self.register_called.store(true, Ordering::SeqCst);
        Ok(())
    }
    fn notify_listening(&self, listening: bool, _nat_type: rust_comms::engine::NatType) { if listening { self.listening_notified.store(true, Ordering::SeqCst); } }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn triangle_test3_notifies_listening_true() {
    // Arrange: router with MockApi and MockInternal
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 45001);
    let internal = Arc::new(MockInternal::new(addr));
    let router = std::sync::Arc::new(rust_comms::messages::router::Router::new(crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_internal_override(internal.clone()))));

    // Act: route TriangleTest3 through DefaultPrintingHandler
    let handler = rust_comms::messages::handlers::DefaultPrintingHandler;
    let msg = Message::Relay(RelayMessage::TriangleTest3(RelayTriangleTest3 { app: None }));
    rust_comms::messages::router::Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "FROMID");
    });

    // Assert: registration was initiated and listening notified
    let start = Instant::now();
    let timeout = Duration::from_millis(500);
    while !internal.register_called.load(Ordering::SeqCst) && start.elapsed() < timeout {
        std::thread::sleep(Duration::from_millis(10));
    }
    while !internal.listening_notified.load(Ordering::SeqCst) && start.elapsed() < timeout {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(internal.register_called.load(Ordering::SeqCst), "ddb_register_ip should be called");
    assert!(internal.listening_notified.load(Ordering::SeqCst), "notify_listening(true) should be called");
}
