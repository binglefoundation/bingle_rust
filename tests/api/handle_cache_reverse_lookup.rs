use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::dtls::{Dtls, Result as DtlsResult, HandleMessage, HandlePeerCertificate};
use rust_comms::api::bingle_api::NetworkEndpoint;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
struct MockDtls;
impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<rust_comms::dtls::UdpNetworkMux>) -> DtlsResult<()> { Ok(()) }
    fn stop(&mut self) -> DtlsResult<()> { Ok(()) }
    fn send(&self, _to: &NetworkEndpoint, _data: &[u8]) -> DtlsResult<()> { Ok(()) }
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
fn reverse_lookup_after_handle_lookup_populates_cache() {
    // Short expiry to keep tests deterministic
    let mut opts = StartOptions::default();
    opts.handle_cache_expiry = Some(Duration::from_secs(60));
    let api = BingleApiImpl::new_with_dtls(Box::new(MockDtls));

    // Mock a mapping for a specific handle
    let handle = "handle_alice".to_string();
    let user_id = "ALICE_USER_ID_123".to_string();
    let user_id_clone = user_id.clone();
    let handle_for_mock = handle.clone();
    api.set_handle_lookup_mock_for_tests(Box::new(move |h| {
        if h == &handle_for_mock { Ok(Some(user_id_clone.clone())) } else { Ok(None) }
    }));

    // Trigger lookup by handle to fill cache
    let uid = api.handle_lookup(&handle).unwrap();
    assert_eq!(uid, Some(user_id.clone()));

    // Now reverse-lookup by id should retrieve the same handle
    let h = api.handle_lookup_by_id(&user_id).expect("reverse lookup should succeed");
    assert_eq!(h, handle);
}

#[test]
fn reverse_lookup_via_inbound_message_is_cached() {
    let api = BingleApiImpl::new_with_dtls(Box::new(MockDtls));
    let handle = "bob_handle".to_string();
    let user_id = "BOB_USER_ID_456".to_string();

    // Simulate an inbound message from bob with his handle
    api.handle_incoming_network_message(user_id.clone(), handle.clone(), serde_json::json!({"msg": "hi"}));

    // Reverse lookup should succeed without any blockchain/mock lookup
    let h = api.handle_lookup_by_id(&user_id).expect("reverse lookup should find bob's handle");
    assert_eq!(h, handle);
}

#[test]
fn reverse_lookup_respects_expiry() {
    let mut opts = StartOptions::default();
    opts.handle_cache_expiry = Some(Duration::from_millis(50));
    let api = BingleApiImpl::new(&opts);

    let handle = "carol_handle".to_string();
    let user_id = "CAROL_USER_ID_789".to_string();

    // Insert via inbound message path
    api.handle_incoming_network_message(user_id.clone(), handle.clone(), serde_json::json!({"x":1}));
    assert_eq!(api.handle_lookup_by_id(&user_id), Some(handle.clone()));

    // Wait beyond expiry and verify it is gone
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(api.handle_lookup_by_id(&user_id), None);
}
