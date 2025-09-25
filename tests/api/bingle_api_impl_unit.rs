use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use rust_comms::api::bingle_api::{NetworkSourceKey, BingleApi};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::dtls::{Dtls, HandleMessage, HandlePeerCertificate, Result, NetworkMux};

#[derive(Clone)]
struct MockDtls {
    pub sends: Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>,
}

impl MockDtls { fn new() -> (Self, Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>) { let v = Arc::new(Mutex::new(vec![])); (Self { sends: v.clone() }, v) } }

impl Dtls for MockDtls {
    fn start(&mut self, _addr: SocketAddr, _mux: Option<Arc<dyn NetworkMux + Send + Sync>>) -> Result<()> { Ok(()) }
    fn stop(&mut self) -> Result<()> { Ok(()) }
    fn send(&self, to: SocketAddr, _issuer: &str, data: &[u8]) -> Result<()> { self.sends.lock().unwrap().push((to, data.to_vec())); Ok(()) }
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
fn unit_send_message_to_network_calls_dtls_send() {
    let (mock, sent_vec) = MockDtls::new();
    let api = BingleApiImpl::new_with_dtls(Box::new(mock));
    let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
    let nsk = NetworkSourceKey { inet_socket_address: Some(addr), relay_channel: None, relay_address: None };
    let msg = serde_json::json!({"hello": "world"});
    let progress_calls: Arc<Mutex<Vec<(u8, String)>>> = Arc::new(Mutex::new(vec![]));
    let progress_calls_closure = progress_calls.clone();
    let ok = api.send_message_to_network(
        &nsk,
        &"user1".to_string(),
        msg.clone(),
        Some(Arc::new(move |p, s| { progress_calls_closure.lock().unwrap().push((p, s)); })),
    );
    assert!(ok, "send_message_to_network should return true on MockDtls::send Ok");
    let locked = sent_vec.lock().unwrap();
    assert_eq!(locked.len(), 1);
    assert_eq!(locked[0].0, addr);
    assert_eq!(locked[0].1, serde_json::to_vec(&msg).unwrap());
    assert!(progress_calls.lock().unwrap().iter().any(|(p, _)| *p == 100));
}
