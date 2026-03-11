use std::net::SocketAddr;
use std::sync::Arc;

use rust_comms::api::bingle_api::{NetworkEndpoint, StartOptions};
use rust_comms::engine::Engine;
use rust_comms::dtls::dtls_trait::{Dtls, HandleMessage, HandlePeerCertificate, Result as DtlsResult};
use rust_comms::dtls::UdpNetworkMux;

#[derive(Default, Clone)]
struct MockDtls;
impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<UdpNetworkMux>) -> DtlsResult<()> { Ok(()) }
    fn stop(&mut self) -> DtlsResult<()> { Ok(()) }
    fn send(&self, _to: &NetworkEndpoint, _data: &[u8]) -> DtlsResult<()> { Ok(()) }
    fn get_handle_message(&self) -> Option<HandleMessage> { None }
    fn set_handle_message(&mut self, _handler: Option<HandleMessage>) {}
    fn with_handle_message(self, _handler: HandleMessage) -> Self { self }
    fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> { None }
    fn set_handle_peer_certificate(&mut self, _handler: Option<HandlePeerCertificate>) {}
    fn with_handle_peer_certificate(self, _handler: HandlePeerCertificate) -> Self { self }
    fn get_ca_cert(&self) -> Option<&[u8]> { None }
    fn set_ca_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_ca_cert(self, _pem: Vec<u8>) -> Self { self }
    fn get_client_cert(&self) -> Option<&[u8]> { None }
    fn set_client_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_cert(self, _pem: Vec<u8>) -> Self { self }
    fn get_client_private_key(&self) -> Option<&[u8]> { None }
    fn set_client_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_private_key(self, _pem: Vec<u8>) -> Self { self }
    fn get_server_signing_cert(&self) -> Option<&[u8]> { None }
    fn set_server_signing_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self { self }
    fn get_server_signing_private_key(&self) -> Option<&[u8]> { None }
    fn set_server_signing_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self { self }
}

#[test]
fn engine_tracks_seen_endpoints() {
    let options = StartOptions::default();
    // Use MockApiBoth for engine creation (requires Arc/Weak)
    let api = Arc::new(crate::util::reusable_mock_api::MockApiBoth::new());
    let engine = Engine::new_with_dtls(&options, Arc::downgrade(&api) as _, Box::new(MockDtls::default()));

    let addr1: SocketAddr = "127.0.0.1:1111".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:2222".parse().unwrap();
    let nsk1 = NetworkEndpoint::new_direct(addr1);
    let nsk2 = NetworkEndpoint::new_direct(addr2);

    // Act: send to endpoints
    engine.send_to_peer(&nsk1, b"hello").unwrap();
    engine.send_to_peer(&nsk2, b"world").unwrap();
    // Duplicate send should not duplicate entries in the set (HashSet handles this)
    engine.send_to_peer(&nsk1, b"hello again").unwrap();

    // Assert: both unique addresses recorded
    let seen = engine.seen_endpoints_for_tests();
    assert_eq!(seen.len(), 2);
    assert!(seen.iter().any(|a| a.to_string() == addr1.to_string()));
    assert!(seen.iter().any(|a| a.to_string() == addr2.to_string()));
}
