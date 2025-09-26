use std::net::SocketAddr;
use std::sync::Arc;

use dtls_pki::dtls::{Dtls, HandleMessage, HandlePeerCertificate, NetworkMux};

#[derive(Default)]
struct DummyDtls;

impl Dtls for DummyDtls {
    fn start(&mut self, _addr: SocketAddr, _mux: Option<Arc<dyn NetworkMux + Send + Sync>>) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) -> Result<(), String> { Ok(()) }
    fn send(&self, _to: SocketAddr, _data: &[u8]) -> Result<(), String> { Ok(()) }
    fn get_handle_message(&self) -> Option<HandleMessage> { None }
    fn set_handle_message(&mut self, _handler: Option<HandleMessage>) {}
    fn with_handle_message(self, _handler: HandleMessage) -> Self where Self: Sized { self }
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
fn dtls_send_takes_no_issuer() {
    let d = DummyDtls::default();
    let to: SocketAddr = "127.0.0.1:9000".parse().unwrap();
    let data = b"hello";
    // Should compile and run: no issuer argument
    assert!(Dtls::send(&d, to, data).is_ok());
}
