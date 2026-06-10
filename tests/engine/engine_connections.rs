use crate::util::reusable_mock_api::MockApiBoth;
use rust_comms::api::bingle_api::{StartOptions};
use rust_comms::dtls::network_mux_udp::UdpNetworkMux;
use rust_comms::dtls::{Dtls, HandleMessage};
use rust_comms::engine::Engine;
use std::net::SocketAddr;

#[derive(Default)]
struct FakeDtls {
    last_send: std::sync::Mutex<Option<SocketAddr>>,
    handler: std::sync::Mutex<Option<HandleMessage>>,    
}

impl FakeDtls {
    fn new() -> Self { Self { last_send: std::sync::Mutex::new(None), handler: std::sync::Mutex::new(None) } }
}

impl Dtls for FakeDtls {
    fn start(&mut self, _mux: std::sync::Arc<UdpNetworkMux>) -> rust_comms::dtls::Result<()> { Ok(()) }
    fn stop(&mut self) -> rust_comms::dtls::Result<()> { Ok(()) }
    fn send(&self, to: &rust_comms::api::bingle_api::NetworkEndpoint, data: &[u8]) -> rust_comms::dtls::Result<()> {
        let _ = self.last_send.lock().map(|mut g| *g = to.inet_socket_address());
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

    fn set_app_layer_only_verification(&mut self, _enabled: bool) { }
    fn with_app_layer_only_verification(self, _enabled: bool) -> Self where Self: Sized { self }
    fn set_dangerous_debug(&mut self, _enabled: bool) {}
    fn with_dangerous_debug(self, _enabled: bool) -> Self where Self: Sized { self }
    fn set_null_encryption(&mut self, _enabled: bool) {}
    fn with_null_encryption(self, _enabled: bool) -> Self where Self: Sized { self }
    fn get_cipher_suite(&self, _endpoint: &rust_comms::api::bingle_api::NetworkEndpoint) -> Option<String> { None }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn engine_send_to_peer_tracks_connections_and_reuses() {
    let engine = Engine::new_with_dtls(&StartOptions::new("".into()), crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new()), Box::new(FakeDtls::new()));

    let a1: SocketAddr = "127.0.0.1:12345".parse().unwrap();
    let a2: SocketAddr = "127.0.0.1:23456".parse().unwrap();

    assert_eq!(engine.connections_len_for_tests(), 0);

    // First send should create entry
    let nsk1 = rust_comms::api::bingle_api::NetworkEndpoint::new_direct(a1);
    let r1 = engine.send_to_peer(&nsk1, b"hello");
    assert!(r1.is_ok());
    assert!(engine.has_connection(&rust_comms::api::bingle_api::NetworkEndpoint::new_direct(a1)));
    assert_eq!(engine.connections_len_for_tests(), 1);

    // Second send to same addr should not create a second entry
    let r2 = engine.send_to_peer(&nsk1, b"again");
    assert!(r2.is_ok());
    assert!(engine.has_connection(&rust_comms::api::bingle_api::NetworkEndpoint::new_direct(a1)));
    assert_eq!(engine.connections_len_for_tests(), 1);

    // Send to a different addr should add another entry
    let nsk2 = rust_comms::api::bingle_api::NetworkEndpoint::new_direct(a2);
    let r3 = engine.send_to_peer(&nsk2, b"new peer");
    assert!(r3.is_ok());
    assert!(engine.has_connection(&rust_comms::api::bingle_api::NetworkEndpoint::new_direct(a2)));
    assert_eq!(engine.connections_len_for_tests(), 2);
}
