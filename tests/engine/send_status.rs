use std::sync::Arc;
use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth};
use rust_comms::api::bingle_api::{NetworkEndpoint, StartOptions};
use rust_comms::dtls::network_mux_udp::UdpNetworkMux;
use rust_comms::dtls::{Dtls, HandleMessage};
use rust_comms::engine::{Engine, EndpointStatus, SEND_FAIL_BACKOFF};
use rust_comms::messages::router::Router;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// A mock API that authenticates any sender, allowing the engine auth check to pass.
struct AlwaysAuthenticatedApi;
impl InnerBingleApi for AlwaysAuthenticatedApi {
    fn handle_lookup_by_id(&self, _user_id: &rust_comms::api::bingle_api::UserId) -> Option<rust_comms::api::bingle_api::Handle> {
        Some("test_handle".to_string())
    }
}

/// Build an engine where any sender is authenticated; return the engine, installed DTLS callback,
/// and the router (needed for `Router::with_current_router` when invoking the handler in tests).
fn build_engine_with_auth() -> (Engine, HandleMessage, Arc<Router>) {
    let api = crate::util::reusable_mock_api::to_weak_api_both(
        MockApiBoth::new_with_api_override(Arc::new(AlwaysAuthenticatedApi))
    );
    let mut engine = Engine::new_with_dtls(&StartOptions::new("".into()), api.clone(), Box::new(SucceedingDtls::new()));
    let router = Arc::new(Router::new(api));
    engine.set_router(router.clone());
    engine.install_dtls_handler_for_tests().expect("install_dtls_handler_for_tests failed");
    let handler = {
        let mut h: Option<HandleMessage> = None;
        engine.with_dtls_mut(|dtls| { h = dtls.get_handle_message(); });
        h.expect("DTLS handler should be installed")
    };
    (engine, handler, router)
}

/// Simulate a packet arriving from the given endpoint via the installed DTLS handler.
fn simulate_packet_arrival(handler: &HandleMessage, router: Arc<Router>, from: &NetworkEndpoint, issuer: &str, data: &[u8]) {
    let fake_dtls = SucceedingDtls::new();
    Router::with_current_router(router, || {
        handler(&fake_dtls as &dyn Dtls, from, issuer, data);
    });
}

/// A mock DTLS implementation that always succeeds and auto-acks FRPT DATA_SINGLE packets.
struct SucceedingDtls {
    handler: std::sync::Mutex<Option<HandleMessage>>,
}

impl SucceedingDtls {
    fn new() -> Self {
        Self { handler: std::sync::Mutex::new(None) }
    }
}

impl Dtls for SucceedingDtls {
    fn start(&mut self, _mux: std::sync::Arc<UdpNetworkMux>) -> rust_comms::dtls::Result<()> { Ok(()) }
    fn stop(&mut self) -> rust_comms::dtls::Result<()> { Ok(()) }
    fn send(&self, to: &NetworkEndpoint, data: &[u8]) -> rust_comms::dtls::Result<()> {
        // Auto-ack FRPT DATA_SINGLE so that packet_transport::send completes immediately
        if data.len() >= 4 && (data[0] & 0x0F) == 0x01 {
            if let Some(h) = self.handler.lock().ok().and_then(|g| g.clone()) {
                h(self, to, "mock-auto-ack", &[0x14, 0x00, data[2], data[3]]);
            }
        }
        Ok(())
    }
    fn get_handle_message(&self) -> Option<HandleMessage> { self.handler.lock().ok().and_then(|g| g.clone()) }
    fn set_handle_message(&mut self, handler: Option<HandleMessage>) { let _ = self.handler.lock().map(|mut g| *g = handler); }
    fn set_handle_new_session(&mut self, _handler: Option<rust_comms::dtls::dtls_trait::HandleNewSession>) {}
    fn with_handle_message(self, handler: HandleMessage) -> Self where Self: Sized { let _ = self.handler.lock().map(|mut g| *g = Some(handler)); self }
    fn get_handle_peer_certificate(&self) -> Option<rust_comms::dtls::HandlePeerCertificate> { None }
    fn set_handle_peer_certificate(&mut self, _handler: Option<rust_comms::dtls::HandlePeerCertificate>) {}
    fn with_handle_peer_certificate(self, _handler: rust_comms::dtls::HandlePeerCertificate) -> Self where Self: Sized { self }
    fn get_ca_cert(&self) -> Option<&[u8]> { None }
    fn set_ca_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_ca_cert(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_client_cert(&self) -> Option<&[u8]> { None }
    fn set_client_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_cert(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_client_private_key(&self) -> Option<&[u8]> { None }
    fn set_client_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_private_key(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_server_signing_cert(&self) -> Option<&[u8]> { None }
    fn set_server_signing_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_server_signing_private_key(&self) -> Option<&[u8]> { None }
    fn set_server_signing_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn set_app_layer_only_verification(&mut self, _enabled: bool) {}
    fn with_app_layer_only_verification(self, _enabled: bool) -> Self where Self: Sized { self }
    fn set_dangerous_debug(&mut self, _enabled: bool) {}
    fn with_dangerous_debug(self, _enabled: bool) -> Self where Self: Sized { self }
    fn set_null_encryption(&mut self, _enabled: bool) {}
    fn with_null_encryption(self, _enabled: bool) -> Self where Self: Sized { self }
    fn get_cipher_suite(&self, _endpoint: &NetworkEndpoint) -> Option<String> { None }
}

/// A mock DTLS implementation that always fails on send.
struct FailingDtls {
    handler: std::sync::Mutex<Option<HandleMessage>>,
}

impl FailingDtls {
    fn new() -> Self {
        Self { handler: std::sync::Mutex::new(None) }
    }
}

impl Dtls for FailingDtls {
    fn start(&mut self, _mux: std::sync::Arc<UdpNetworkMux>) -> rust_comms::dtls::Result<()> { Ok(()) }
    fn stop(&mut self) -> rust_comms::dtls::Result<()> { Ok(()) }
    fn send(&self, _to: &NetworkEndpoint, _data: &[u8]) -> rust_comms::dtls::Result<()> {
        Err("simulated send failure".to_string())
    }
    fn get_handle_message(&self) -> Option<HandleMessage> { self.handler.lock().ok().and_then(|g| g.clone()) }
    fn set_handle_message(&mut self, handler: Option<HandleMessage>) { let _ = self.handler.lock().map(|mut g| *g = handler); }
    fn set_handle_new_session(&mut self, _handler: Option<rust_comms::dtls::dtls_trait::HandleNewSession>) {}
    fn with_handle_message(self, handler: HandleMessage) -> Self where Self: Sized { let _ = self.handler.lock().map(|mut g| *g = Some(handler)); self }
    fn get_handle_peer_certificate(&self) -> Option<rust_comms::dtls::HandlePeerCertificate> { None }
    fn set_handle_peer_certificate(&mut self, _handler: Option<rust_comms::dtls::HandlePeerCertificate>) {}
    fn with_handle_peer_certificate(self, _handler: rust_comms::dtls::HandlePeerCertificate) -> Self where Self: Sized { self }
    fn get_ca_cert(&self) -> Option<&[u8]> { None }
    fn set_ca_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_ca_cert(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_client_cert(&self) -> Option<&[u8]> { None }
    fn set_client_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_cert(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_client_private_key(&self) -> Option<&[u8]> { None }
    fn set_client_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_private_key(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_server_signing_cert(&self) -> Option<&[u8]> { None }
    fn set_server_signing_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn get_server_signing_private_key(&self) -> Option<&[u8]> { None }
    fn set_server_signing_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self where Self: Sized { self }
    fn set_app_layer_only_verification(&mut self, _enabled: bool) {}
    fn with_app_layer_only_verification(self, _enabled: bool) -> Self where Self: Sized { self }
    fn set_dangerous_debug(&mut self, _enabled: bool) {}
    fn with_dangerous_debug(self, _enabled: bool) -> Self where Self: Sized { self }
    fn set_null_encryption(&mut self, _enabled: bool) {}
    fn with_null_encryption(self, _enabled: bool) -> Self where Self: Sized { self }
    fn get_cipher_suite(&self, _endpoint: &NetworkEndpoint) -> Option<String> { None }
}

fn make_engine_with_succeeding_dtls() -> Engine {
    Engine::new_with_dtls(
        &StartOptions::new("".into()),
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new()),
        Box::new(SucceedingDtls::new()),
    )
}

fn make_engine_with_failing_dtls() -> Engine {
    let mut engine = Engine::new_with_dtls(
        &StartOptions::new("".into()),
        crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new()),
        Box::new(FailingDtls::new()),
    );
    // Use zero retry delays so the test does not wait for backoff timeouts
    engine.set_retry_delays_for_packet_transport(vec![]);
    engine
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn send_status_is_success_after_successful_send() {
    let engine = make_engine_with_succeeding_dtls();

    let addr: SocketAddr = "127.0.0.1:9001".parse().unwrap();
    let endpoint = NetworkEndpoint::new_direct(addr);
    let key = endpoint.get_key().expect("endpoint must have a key");

    let result = engine.send_to_peer(&endpoint, b"hello");
    assert!(result.is_ok(), "send should succeed: {:?}", result);

    let status_map = engine.endpoint_status_for_tests();
    let status = status_map.get(&key).expect("send_status should contain an entry for the endpoint");
    assert!(status.is_working, "send_status.success should be true after a successful send");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn send_status_is_failure_after_failed_send() {
    let engine = make_engine_with_failing_dtls();

    let addr: SocketAddr = "127.0.0.1:9002".parse().unwrap();
    let endpoint = NetworkEndpoint::new_direct(addr);
    let key = endpoint.get_key().expect("endpoint must have a key");

    let result = engine.send_to_peer(&endpoint, b"hello");
    assert!(result.is_err(), "send should fail with FailingDtls");

    let status_map = engine.endpoint_status_for_tests();
    let status = status_map.get(&key).expect("send_status should contain an entry for the endpoint even on failure");
    assert!(!status.is_working, "send_status.success should be false after a failed send");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn send_status_tracks_multiple_endpoints_independently() {
    let engine = make_engine_with_succeeding_dtls();

    let addr1: SocketAddr = "127.0.0.1:9003".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:9004".parse().unwrap();
    let ep1 = NetworkEndpoint::new_direct(addr1);
    let ep2 = NetworkEndpoint::new_direct(addr2);
    let key1 = ep1.get_key().expect("endpoint 1 must have a key");
    let key2 = ep2.get_key().expect("endpoint 2 must have a key");

    let r1 = engine.send_to_peer(&ep1, b"to peer 1");
    assert!(r1.is_ok());
    let r2 = engine.send_to_peer(&ep2, b"to peer 2");
    assert!(r2.is_ok());

    let status_map = engine.endpoint_status_for_tests();
    assert_eq!(status_map.len(), 2, "should have entries for both endpoints");
    assert!(status_map.get(&key1).expect("key1 should be present").is_working);
    assert!(status_map.get(&key2).expect("key2 should be present").is_working);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn send_status_updates_from_success_to_failure_on_repeated_sends() {
    // Start with a succeeding DTLS, then swap to a failing one via set_dtls.
    let mut engine = make_engine_with_succeeding_dtls();

    let addr: SocketAddr = "127.0.0.1:9005".parse().unwrap();
    let endpoint = NetworkEndpoint::new_direct(addr);
    let key = endpoint.get_key().expect("endpoint must have a key");

    // First send: succeeds
    let r1 = engine.send_to_peer(&endpoint, b"first");
    assert!(r1.is_ok());
    assert!(engine.endpoint_status_for_tests().get(&key).expect("entry after first send").is_working);

    // Swap to a failing DTLS
    let mut failing = FailingDtls::new();
    // Copy any existing handler so the infrastructure stays consistent
    failing.set_handle_message(engine.dtls().get_handle_message());
    engine.set_dtls(Box::new(failing));
    engine.set_retry_delays_for_packet_transport(vec![]);

    // Second send: fails
    let r2 = engine.send_to_peer(&endpoint, b"second");
    assert!(r2.is_err());
    assert!(!engine.endpoint_status_for_tests().get(&key).expect("entry after second send").is_working);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn send_to_peer_returns_err_immediately_when_recent_failure_recorded() {
    // Arrange: build engine with succeeding DTLS and pre-seed the send_status map
    // with a recent failure for the target endpoint.
    let engine = make_engine_with_succeeding_dtls();
    let addr: SocketAddr = "127.0.0.1:9010".parse().unwrap();
    let endpoint = NetworkEndpoint::new_direct(addr);
    let key = endpoint.get_key().expect("endpoint must have a key");

    // Seed a recent failure (timestamp = now, within SEND_FAIL_BACKOFF)
    engine.set_endpoint_status_for_tests(key.clone(), EndpointStatus { last_checked_timestamp: Instant::now(), is_working: false });

    // Act: even though DTLS would succeed, the backoff guard should short-circuit
    let result = engine.send_to_peer(&endpoint, b"data");
    assert!(result.is_err(), "expected Err due to backoff guard, got {:?}", result);
    assert_eq!(result.unwrap_err(), "Sending is failing");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn send_to_peer_sends_normally_when_failure_is_older_than_backoff() {
    // Arrange: build engine with succeeding DTLS and pre-seed the send_status map
    // with a failure whose timestamp is older than SEND_FAIL_BACKOFF.
    let engine = make_engine_with_succeeding_dtls();
    let addr: SocketAddr = "127.0.0.1:9011".parse().unwrap();
    let endpoint = NetworkEndpoint::new_direct(addr);
    let key = endpoint.get_key().expect("endpoint must have a key");

    // Seed an old failure (timestamp = now - backoff - 1s)
    let old_timestamp = Instant::now().checked_sub(SEND_FAIL_BACKOFF + Duration::from_secs(1))
        .expect("must be able to subtract duration");
    engine.set_endpoint_status_for_tests(key.clone(), EndpointStatus { last_checked_timestamp: old_timestamp, is_working: false });

    // Act: failure is old enough — the send should proceed and succeed
    let result = engine.send_to_peer(&endpoint, b"data");
    assert!(result.is_ok(), "expected Ok since failure is older than SEND_FAIL_BACKOFF, got {:?}", result);

    // Status map should now show success
    let status_map = engine.endpoint_status_for_tests();
    let status = status_map.get(&key).expect("entry must exist after send");
    assert!(status.is_working, "send_status.success should be true after the successful send");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn send_to_peer_sends_normally_when_previous_send_succeeded() {
    // Arrange: a pre-seeded success entry should NOT trigger the backoff guard.
    let engine = make_engine_with_succeeding_dtls();
    let addr: SocketAddr = "127.0.0.1:9012".parse().unwrap();
    let endpoint = NetworkEndpoint::new_direct(addr);
    let key = endpoint.get_key().expect("endpoint must have a key");

    // Seed a recent success
    engine.set_endpoint_status_for_tests(key.clone(), EndpointStatus { last_checked_timestamp: Instant::now(), is_working: true });

    // Act: backoff guard must not fire for a success entry
    let result = engine.send_to_peer(&endpoint, b"data");
    assert!(result.is_ok(), "expected Ok since status is success, got {:?}", result);
}

// --- Packet arrival tests ---

#[test]
#[cfg(not(target_os = "ios"))]
pub fn packet_arrival_sets_endpoint_status_is_working_true() {
    let (engine, handler, router) = build_engine_with_auth();

    let addr: SocketAddr = "127.0.0.1:9020".parse().unwrap();
    let from = NetworkEndpoint::new_direct(addr);
    let key = from.get_key().expect("endpoint must have a key");
    let before = Instant::now();

    simulate_packet_arrival(&handler, router, &from, "SENDER.", b"{}");

    let status_map = engine.endpoint_status_for_tests();
    let status = status_map.get(&key).expect("endpoint_status should have an entry after packet arrival");
    assert!(status.is_working, "is_working should be true after packet arrival");
    assert!(
        status.last_checked_timestamp >= before,
        "last_checked_timestamp should be recent"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_packet_means_no_endpoint_status_entry() {
    let (engine, _handler, _router) = build_engine_with_auth();

    let addr: SocketAddr = "127.0.0.1:9021".parse().unwrap();
    let from = NetworkEndpoint::new_direct(addr);
    let key = from.get_key().expect("endpoint must have a key");

    let status_map = engine.endpoint_status_for_tests();
    assert!(
        status_map.get(&key).is_none(),
        "endpoint_status should have no entry when no packet has arrived"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn unauthenticated_packet_does_not_set_endpoint_status() {
    // Use a plain MockApiBoth (handle_lookup_by_id returns None) — auth will fail.
    let api = crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new());
    let mut engine = Engine::new_with_dtls(&StartOptions::new("".into()), api.clone(), Box::new(SucceedingDtls::new()));
    let router = Arc::new(Router::new(api));
    engine.set_router(router.clone());
    engine.install_dtls_handler_for_tests().expect("install failed");
    let handler = {
        let mut h: Option<HandleMessage> = None;
        engine.with_dtls_mut(|dtls| { h = dtls.get_handle_message(); });
        h.expect("handler must be installed")
    };

    let addr: SocketAddr = "127.0.0.1:9022".parse().unwrap();
    let from = NetworkEndpoint::new_direct(addr);
    let key = from.get_key().expect("endpoint must have a key");

    simulate_packet_arrival(&handler, router, &from, "UNKNOWN_SENDER.", b"{}");

    let status_map = engine.endpoint_status_for_tests();
    assert!(
        status_map.get(&key).is_none(),
        "endpoint_status should not be updated for unauthenticated sender"
    );
}
