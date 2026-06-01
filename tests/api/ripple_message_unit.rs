use rust_comms::api::bingle_api::{BingleApiInternal, NetworkEndpoint};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::dtls::{Dtls, HandleMessage, HandlePeerCertificate, Result};
use rust_comms::ddb::{AdvertRecord, DdbBackend, InMemoryDdbBackend, InetSocketAddress};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

#[path = "../test_util.rs"]
pub mod test_util;

#[derive(Clone)]
struct MockDtls {
    pub sends: Arc<Mutex<Vec<(SocketAddr, String, serde_json::Value)>>>,
}

impl MockDtls {
    fn new() -> Self {
        Self {
            sends: Arc::new(Mutex::new(vec![])),
        }
    }
}

impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<rust_comms::dtls::UdpNetworkMux>) -> Result<()> { Ok(()) }
    fn stop(&mut self) -> Result<()> { Ok(()) }
    fn send(&self, to: &NetworkEndpoint, data: &[u8]) -> Result<()> {
        let addr = to.inet_socket_address().expect("MockDtls::send requires inet_socket_address");
        let json: serde_json::Value = serde_json::from_slice(test_util::maybe_unwrap_data_single(data))
            .expect("valid json");
        self.sends.lock().unwrap().push((addr, "unknown".to_string(), json));
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
    fn set_null_encryption(&mut self, _enabled: bool) {}
    fn with_null_encryption(self, _enabled: bool) -> Self where Self: Sized { self }
    fn get_cipher_suite(&self, _endpoint: &rust_comms::api::bingle_api::NetworkEndpoint) -> Option<String> { None }
}

#[test]
pub fn ripple_message_reaches_relays_in_backend() {
    test_util::init_test_logging();
    let mock = MockDtls::new();
    let sent_vec = mock.sends.clone();
    let api = BingleApiImpl::new_with_dtls(Box::new(mock));

    // Setup issuer for self
    let my_id = test_util::ADDRESS_SPEND.to_string();
    api.set_issuer_for_tests(format!("{}{}", my_id, rust_comms::protocol::ISSUER_SUFFIX));

    // Setup DDB backend with some relays
    let mut backend = InMemoryDdbBackend::new();
    
    let relay1_id = test_util::ADDRESS_RECEIVE.to_string();
    let relay1_addr: SocketAddr = "127.0.0.1:1111".parse().unwrap();
    backend.upsert(AdvertRecord {
        id: relay1_id.clone(),
        endpoint: Some(InetSocketAddress::from(relay1_addr)),
        am_relay: Some(true),
        relay_id: None,
        relay_sig: None,
        date: "2023-01-01T00:00:00Z".to_string(),
        sig: None,
    });

    let relay2_id = test_util::ADDRESS_10MIL.to_string();
    let relay2_addr: SocketAddr = "127.0.0.1:2222".parse().unwrap();
    backend.upsert(AdvertRecord {
        id: relay2_id.clone(),
        endpoint: Some(InetSocketAddress::from(relay2_addr)),
        am_relay: Some(true),
        relay_id: None,
        relay_sig: None,
        date: "2023-01-01T00:00:00Z".to_string(),
        sig: None,
    });

    let originator_id = "V332YQYPFY5D3U7P36YV3Z6W3L7S6U2T5Q6X4Z5S6U7T8Y9Z0A1B2C3D4E".to_string(); // Just another valid-ish looking address
    // Wait, better use another constant from test_util if available.
    // Actually let's use originator_id from test_util if it has more.
    // Use Alice/Bob if they are valid.
    
    let ripple_msg = serde_json::json!({"ripple": "data"});

    // Call ripple_message
    api.ripple_message(ripple_msg.clone(), originator_id.clone(), &backend);

    // Verify
    let sends = sent_vec.lock().unwrap();
    assert_eq!(sends.len(), 2, "Should have rippled to 2 relays");
    
    let mut targets: Vec<SocketAddr> = sends.iter().map(|s| s.0).collect();
    targets.sort_by_key(|a| a.port());
    
    assert_eq!(targets[0], relay1_addr);
    assert_eq!(targets[1], relay2_addr);
    
    assert_eq!(sends[0].2, ripple_msg);
    assert_eq!(sends[1].2, ripple_msg);
}

#[test]
pub fn ripple_message_skips_originator_and_self() {
    test_util::init_test_logging();
    let mock = MockDtls::new();
    let sent_vec = mock.sends.clone();
    let api = BingleApiImpl::new_with_dtls(Box::new(mock));

    // Setup issuer for self
    let my_id = test_util::ADDRESS_SPEND.to_string();
    api.set_issuer_for_tests(format!("{}{}", my_id, rust_comms::protocol::ISSUER_SUFFIX));

    // Setup DDB backend
    let mut backend = InMemoryDdbBackend::new();
    
    // Relay 1 (Normal)
    let relay1_id = test_util::ADDRESS_RECEIVE.to_string();
    let relay1_addr: SocketAddr = "127.0.0.1:1111".parse().unwrap();
    backend.upsert(AdvertRecord {
        id: relay1_id.clone(),
        endpoint: Some(InetSocketAddress::from(relay1_addr)),
        am_relay: Some(true),
        relay_id: None,
        relay_sig: None,
        date: "2023-01-01T00:00:00Z".to_string(),
        sig: None,
    });

    // Relay 2 (Originator)
    let relay2_id = test_util::ADDRESS_10MIL.to_string();
    let relay2_addr: SocketAddr = "127.0.0.1:2222".parse().unwrap();
    backend.upsert(AdvertRecord {
        id: relay2_id.clone(),
        endpoint: Some(InetSocketAddress::from(relay2_addr)),
        am_relay: Some(true),
        relay_id: None,
        relay_sig: None,
        date: "2023-01-01T00:00:00Z".to_string(),
        sig: None,
    });

    // Relay 3 (Self)
    let relay3_id = test_util::ADDRESS_SPEND.to_string();
    let relay3_addr: SocketAddr = "127.0.0.1:3333".parse().unwrap();
    backend.upsert(AdvertRecord {
        id: relay3_id.clone(),
        endpoint: Some(InetSocketAddress::from(relay3_addr)),
        am_relay: Some(true),
        relay_id: None,
        relay_sig: None,
        date: "2023-01-01T00:00:00Z".to_string(),
        sig: None,
    });

    let ripple_msg = serde_json::json!({"ripple": "data"});

    // Call ripple_message - originator is Relay 2
    api.ripple_message(ripple_msg.clone(), test_util::ADDRESS_10MIL.to_string(), &backend);

    // Verify
    let sends = sent_vec.lock().unwrap();
    assert_eq!(sends.len(), 1, "Should have rippled only to RELAY1 (RECEIVE)");
    assert_eq!(sends[0].0, relay1_addr);
}
