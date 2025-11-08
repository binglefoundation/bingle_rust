#![cfg(not(target_os = "ios"))]

use std::net::SocketAddr;
use std::sync::Arc;

use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::api::bingle_api::StartOptions;
use rust_comms::engine::EngineState;
use rust_comms::dtls::{Dtls, Result, UdpNetworkMux, HandleMessage, HandlePeerCertificate};

#[derive(Clone)]
struct MockDtls {
    handler: Arc<std::sync::Mutex<Option<HandleMessage>>>,
}
impl MockDtls { fn new() -> Self { Self { handler: Arc::new(std::sync::Mutex::new(None)) } } }
impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<UdpNetworkMux>) -> Result<()> { Ok(()) }
    fn stop(&mut self) -> Result<()> { Ok(()) }
    fn send(&self, _to: SocketAddr, _data: &[u8]) -> Result<()> { Ok(()) }
    fn get_handle_message(&self) -> Option<HandleMessage> { self.handler.lock().unwrap().clone() }
    fn set_handle_message(&mut self, handler: Option<HandleMessage>) { *self.handler.lock().unwrap() = handler; }
    fn with_handle_message(self, handler: HandleMessage) -> Self where Self: Sized { let mut s = self; s.set_handle_message(Some(handler)); s }
    fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> { None }
    fn set_handle_peer_certificate(&mut self, _handler: Option<HandlePeerCertificate>) {}
    fn with_handle_peer_certificate(self, _handler: HandlePeerCertificate) -> Self where Self: Sized { self }
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
}

#[test]
fn triangle_test3_sets_engine_state_via_internal_api() {
    // Build API with injected DTLS so Engine exists and router is configured during start
    let mock = MockDtls::new();
    let mut api = BingleApiImpl::new_with_dtls(Box::new(mock.clone()));

    // Start with static IP so Engine installs DTLS handler without STUN
    let opts = StartOptions { handle: "client".into(), algo_passphrase: None, static_ip: Some("127.0.0.1:0".parse().unwrap()), am_relay: false, stun_servers: None };
    let _ = api.start(opts);

    // Ensure handler was installed
    let handler = mock.get_handle_message().expect("DTLS handler not installed");

    // Simulate receiving TriangleTest3 JSON from a peer
    let from: SocketAddr = "127.0.0.1:55555".parse().unwrap();
    let payload = serde_json::json!({"app": null, "type": "TriangleTest3"});
    let bytes = serde_json::to_vec(&payload).unwrap();

    // Call the handler; issuer string is arbitrary here
    handler(&mock, &from, "SOME-ISSUER", &bytes);

    // The message handler should have used the internal API to mark EndpointAvailable
    let st = api.engine_state_for_tests();
    assert!(matches!(st, Some(EngineState::EndpointAvailable)), "state not EndpointAvailable: {:?}", st);
}
