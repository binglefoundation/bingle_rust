use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::dtls::{Dtls, Result as DtlsResult, HandleMessage, HandlePeerCertificate};
use rust_comms::api::bingle_api::NetworkEndpoint;
use std::sync::{Arc, Mutex};
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
    fn set_app_layer_only_verification(&mut self, _enabled: bool) {}
    fn with_app_layer_only_verification(self, _enabled: bool) -> Self where Self: Sized { self }
    fn set_dangerous_debug(&mut self, _enabled: bool) {}
    fn with_dangerous_debug(self, _enabled: bool) -> Self where Self: Sized { self }
    fn get_cipher_suite(&self, _endpoint: &rust_comms::api::bingle_api::NetworkEndpoint) -> Option<String> { None }
}

#[test]
fn test_handle_lookup_cache_hit() {
    let api = BingleApiImpl::new_with_dtls(Box::new(MockDtls));
    
    let call_count = Arc::new(Mutex::new(0));
    let call_count_clone = call_count.clone();
    
    api.set_handle_lookup_mock_for_tests(Box::new(move |_| {
        let mut count = call_count_clone.lock().unwrap();
        *count += 1;
        Ok(Some("test_user_id".to_string()))
    }));
    
    // First call: should be a miss (in cache) but mock will be called and cache it
    let res1 = api.handle_lookup(&"test_handle".to_string()).unwrap();
    assert_eq!(res1, Some("test_user_id".to_string()));
    assert_eq!(*call_count.lock().unwrap(), 1);
    
    // Second call: should be a cache hit
    let res2 = api.handle_lookup(&"test_handle".to_string()).unwrap();
    assert_eq!(res2, Some("test_user_id".to_string()));
    assert_eq!(*call_count.lock().unwrap(), 1); // Mock NOT called again
}

#[test]
fn test_handle_lookup_cache_expiry() {
    let mut opts = StartOptions::default();
    opts.handle_cache_expiry = Some(Duration::from_millis(100));
    let api = BingleApiImpl::new(&opts);
    api.with_engine_mut(|_| {}); // Ensure engine has a dtls instance if needed, but new() does that now
    
    let call_count = Arc::new(Mutex::new(0));
    let call_count_clone = call_count.clone();
    
    api.set_handle_lookup_mock_for_tests(Box::new(move |_| {
        let mut count = call_count_clone.lock().unwrap();
        *count += 1;
        Ok(Some("test_user_id".to_string()))
    }));
    
    // First call: misses cache, mock called, cached
    let _ = api.handle_lookup(&"test_handle".to_string()).unwrap();
    assert_eq!(*call_count.lock().unwrap(), 1);
    
    // Second call (immediate): hit cache
    let _ = api.handle_lookup(&"test_handle".to_string()).unwrap();
    assert_eq!(*call_count.lock().unwrap(), 1);
    
    // Wait for expiry
    std::thread::sleep(Duration::from_millis(200));
    
    // Third call: expired, mock called again
    let _ = api.handle_lookup(&"test_handle".to_string()).unwrap();
    assert_eq!(*call_count.lock().unwrap(), 2);
}
