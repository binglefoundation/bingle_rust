use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use rust_comms::api::bingle_api::{NetworkEndpoint, StartOptions};
use rust_comms::engine::Engine;
use rust_comms::dtls::dtls_trait::{Dtls, HandleMessage, HandlePeerCertificate, Result as DtlsResult};
use rust_comms::dtls::UdpNetworkMux;

#[derive(Default, Clone)]
struct MockDtls {
    handler: Arc<Mutex<Option<HandleMessage>>>,
}
impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<UdpNetworkMux>) -> DtlsResult<()> { Ok(()) }
    fn stop(&mut self) -> DtlsResult<()> { Ok(()) }
    fn send(&self, to: &NetworkEndpoint, data: &[u8]) -> DtlsResult<()> {
        if data.len() >= 4 && (data[0] & 0x0F) == 0x01 {
            if let Some(h) = self.handler.lock().ok().and_then(|g| g.clone()) {
                h(self, to, "mock-auto-ack", &vec![0x14, 0x00, data[2], data[3]]);
            }
        }
        Ok(())
    }
    fn get_handle_message(&self) -> Option<HandleMessage> { self.handler.lock().ok().and_then(|g| g.clone()) }
    fn set_handle_message(&mut self, handler: Option<HandleMessage>) { let _ = self.handler.lock().map(|mut g| *g = handler); }
    fn set_handle_new_session(&mut self, _handler: Option<rust_comms::dtls::dtls_trait::HandleNewSession>) {}
    fn with_handle_message(self, handler: HandleMessage) -> Self { let _ = self.handler.lock().map(|mut g| *g = Some(handler)); self }
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
    fn set_app_layer_only_verification(&mut self, _enabled: bool) {}
    fn with_app_layer_only_verification(self, _enabled: bool) -> Self where Self: Sized { self }
    fn set_dangerous_debug(&mut self, _enabled: bool) {}
    fn with_dangerous_debug(self, _enabled: bool) -> Self where Self: Sized { self }
    fn set_null_encryption(&mut self, _enabled: bool) {}
    fn with_null_encryption(self, _enabled: bool) -> Self where Self: Sized { self }
    fn get_cipher_suite(&self, _endpoint: &rust_comms::api::bingle_api::NetworkEndpoint) -> Option<String> { None }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn engine_tracks_seen_endpoints() {
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
