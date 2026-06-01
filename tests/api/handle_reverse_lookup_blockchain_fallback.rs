use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::api::bingle_api::{NetworkEndpoint, StartOptions};
use rust_comms::dtls::{Dtls, HandleMessage, HandlePeerCertificate, Result as DtlsResult};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct MockDtls;
impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<rust_comms::dtls::UdpNetworkMux>) -> DtlsResult<()> { Ok(()) }
    fn stop(&mut self) -> DtlsResult<()> { Ok(()) }
    fn send(&self, _to: &NetworkEndpoint, _data: &[u8]) -> DtlsResult<()> { Ok(()) }
    fn get_handle_message(&self) -> Option<HandleMessage> { None }
    fn set_handle_message(&mut self, _handler: Option<HandleMessage>) {}
    fn set_handle_new_session(&mut self, _handler: Option<rust_comms::dtls::dtls_trait::HandleNewSession>) {}
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
    fn set_null_encryption(&mut self, _enabled: bool) {}
    fn with_null_encryption(self, _enabled: bool) -> Self where Self: Sized { self }
}

#[test]
fn reverse_lookup_blockchain_fallback_success_via_mock_and_cache() {
    let _opts = StartOptions::default();
    let api = BingleApiImpl::new_with_dtls(Box::new(MockDtls));

    let user_id = "USER_X_123".to_string();
    let handle = "x_handle".to_string();

    let calls = Arc::new(Mutex::new(0usize));
    let calls_clone = calls.clone();
    let handle_clone = handle.clone();
    let user_clone = user_id.clone();
    api.set_id_to_handle_lookup_mock_for_tests(Box::new(move |uid| {
        let mut c = calls_clone.lock().unwrap();
        *c += 1;
        if uid == &user_clone { Ok(Some(handle_clone.clone())) } else { Ok(None) }
    }));

    // First call -> cache miss, mock used
    let h1 = api.handle_lookup_by_id(&user_id);
    assert_eq!(h1, Some(handle.clone()));
    assert_eq!(*calls.lock().unwrap(), 1);

    // Second call -> cache hit, mock not called again
    let h2 = api.handle_lookup_by_id(&user_id);
    assert_eq!(h2, Some(handle.clone()));
    assert_eq!(*calls.lock().unwrap(), 1);
}

#[test]
fn reverse_lookup_blockchain_fallback_none_via_mock() {
    let api = BingleApiImpl::new_with_dtls(Box::new(MockDtls));
    let user_id = "NO_HANDLE_USER".to_string();
    api.set_id_to_handle_lookup_mock_for_tests(Box::new(move |_uid| Ok(None)));

    let res = api.handle_lookup_by_id(&user_id);
    assert_eq!(res, None);
}

#[test]
fn reverse_lookup_blockchain_fallback_error_via_mock() {
    let api = BingleApiImpl::new_with_dtls(Box::new(MockDtls));
    let user_id = "ERR_USER".to_string();
    api.set_id_to_handle_lookup_mock_for_tests(Box::new(move |_uid| Err("boom".to_string())));

    let res = api.handle_lookup_by_id(&user_id);
    assert_eq!(res, None);
}
