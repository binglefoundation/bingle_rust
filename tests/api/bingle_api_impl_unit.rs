use rust_comms::engine::BingleAccessUnsafeForTests;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use rust_comms::api::bingle_api::{NetworkEndpoint, BingleApi};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::dtls::{Dtls, HandleMessage, HandlePeerCertificate, Result};
use rust_comms::blockchain::algo_ops::byte_key_to_address;
#[path = "../test_util.rs"]
pub mod test_util;

#[derive(Clone)]
struct MockDtls {
    pub sends: Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>,
}

impl MockDtls { fn new() -> (Self, Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>) { let v = Arc::new(Mutex::new(vec![])); (Self { sends: v.clone() }, v) } }

impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<rust_comms::dtls::UdpNetworkMux>) -> Result<()> { Ok(()) }
    fn stop(&mut self) -> Result<()> { Ok(()) }
    fn send(&self, to: &rust_comms::api::bingle_api::NetworkEndpoint, data: &[u8]) -> Result<()> { let addr = to.inet_socket_address().expect("MockDtls::send requires inet_socket_address"); self.sends.lock().unwrap().push((addr, data.to_vec())); Ok(()) }
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
    fn set_app_layer_only_verification(&mut self, _enabled: bool) {}
    fn with_app_layer_only_verification(self, _enabled: bool) -> Self where Self: Sized { self }
    fn set_dangerous_debug(&mut self, _enabled: bool) {}
    fn with_dangerous_debug(self, _enabled: bool) -> Self where Self: Sized { self }
    fn get_cipher_suite(&self, _endpoint: &rust_comms::api::bingle_api::NetworkEndpoint) -> Option<String> { None }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn unit_send_message_to_network_calls_dtls_send() {
    let (mock, sent_vec) = MockDtls::new();
    let api = BingleApiImpl::new_with_dtls(Box::new(mock));
    let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
    let nsk = NetworkEndpoint::new_direct(addr);
    let msg = serde_json::json!({"hello": "world"});
    let progress_calls: Arc<Mutex<Vec<(u8, String)>>> = Arc::new(Mutex::new(vec![]));
    let progress_calls_closure = progress_calls.clone();
    let uid = test_util::ADDRESS_SPEND.to_string();
    let ok = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.send_message_to_network(
        &nsk,
        &uid,
        msg.clone(),
        Some(Arc::new(move |p, s| { progress_calls_closure.lock().unwrap().push((p, s)); })),
    )).unwrap();
    assert!(ok, "send_message_to_network should return true on MockDtls::send Ok");
    let locked = sent_vec.lock().unwrap();
    assert_eq!(locked.len(), 1);
    assert_eq!(locked[0].0, addr);
    assert_eq!(locked[0].1, serde_json::to_vec(&msg).unwrap());
    assert!(progress_calls.lock().unwrap().iter().any(|(p, _)| *p == 100));
}


#[cfg_attr(not(target_os = "ios"), test)]
pub fn start_sets_issuer_and_passes_to_dtls_send() {
    // Mock DTLS that captures the issuer parameter
    #[derive(Clone)]
    struct MockDtlsCapture {
        pub captured: Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>,
    }
    impl MockDtlsCapture { fn new() -> (Self, Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>) { let v = Arc::new(Mutex::new(vec![])); (Self { captured: v.clone() }, v) } }
    impl Dtls for MockDtlsCapture {
        fn start(&mut self, _mux: Arc<rust_comms::dtls::UdpNetworkMux>) -> Result<()> { Ok(()) }
        fn stop(&mut self) -> Result<()> { Ok(()) }
        fn send(&self, to: &rust_comms::api::bingle_api::NetworkEndpoint, data: &[u8]) -> Result<()> {
            let addr = to.inet_socket_address().expect("MockDtlsCapture::send requires inet_socket_address");
            self.captured.lock().unwrap().push((addr, data.to_vec()));
            Ok(())
        }
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
        fn set_app_layer_only_verification(&mut self, _enabled: bool) {}
        fn with_app_layer_only_verification(self, _enabled: bool) -> Self where Self: Sized { self }
        fn set_dangerous_debug(&mut self, _enabled: bool) {}
        fn with_dangerous_debug(self, _enabled: bool) -> Self where Self: Sized { self }
        fn get_cipher_suite(&self, _endpoint: &rust_comms::api::bingle_api::NetworkEndpoint) -> Option<String> { None }
    }

    // Deterministic 32-byte secret and its Algorand address
    let sk: [u8; 32] = [7u8; 32];
    let addr = byte_key_to_address(&ed25519_dalek::SigningKey::from_bytes(&sk).verifying_key().to_bytes()).expect("addr");
    let issuer_expected = format!("{}{}", addr, rust_comms::protocol::ISSUER_SUFFIX);

    let (mock, captured) = MockDtlsCapture::new();
    let api = BingleApiImpl::new_with_dtls(Box::new(mock));

    // Inject issuer directly via test-only helper
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.set_issuer_for_tests(issuer_expected.clone()));

    // Send a message and ensure issuer is passed through
    let addr_send: SocketAddr = "127.0.0.1:45678".parse().unwrap();
    let nsk = NetworkEndpoint::new_direct(addr_send);
    let uid2 = test_util::ADDRESS_RECEIVE.to_string();
    let ok = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.send_message_to_network(&nsk, &uid2, serde_json::json!({"k": 1}), None)).unwrap();
    assert!(ok);
    let cap = captured.lock().unwrap();
    assert_eq!(cap.len(), 1);
    assert_eq!(cap[0].0, addr_send);
}