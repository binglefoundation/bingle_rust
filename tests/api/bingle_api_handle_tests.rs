use rust_comms::api::bingle_api::{BingleApi, BingleError, NetworkEndpoint};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::ddb::DdbClient;
use rust_comms::dtls::{Dtls, HandleMessage, HandlePeerCertificate, Result as DtlsResult};
use rust_comms::engine::BingleAccess;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use serde_json::json;

#[path = "../test_util.rs"]
pub mod test_util;

#[derive(Clone)]
struct MockDtls {
    pub sends: Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>,
}

impl MockDtls {
    fn new() -> (Self, Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>) {
        let v = Arc::new(Mutex::new(vec![]));
        (Self { sends: v.clone() }, v)
    }
}

impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<rust_comms::dtls::UdpNetworkMux>) -> DtlsResult<()> { Ok(()) }
    fn stop(&mut self) -> DtlsResult<()> { Ok(()) }
    fn send(&self, to: &NetworkEndpoint, data: &[u8]) -> DtlsResult<()> {
        let addr = to.inet_socket_address().expect("MockDtls::send requires inet_socket_address");
        self.sends.lock().unwrap().push((addr, data.to_vec()));
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
}

struct MockDdbClient {
    lookup_results: Mutex<std::collections::HashMap<String, NetworkEndpoint>>,
}

impl MockDdbClient {
    fn new() -> Self {
        Self { lookup_results: Mutex::new(std::collections::HashMap::new()) }
    }
    fn set_lookup(&self, id: String, nsk: NetworkEndpoint) {
        self.lookup_results.lock().unwrap().insert(id, nsk);
    }
}

impl DdbClient for MockDdbClient {
    fn register_ip(&self, _endpoint: SocketAddr, _am_relay: bool) -> Result<(), BingleError> { Ok(()) }
    fn register_relay(&self, _relay_id: String, _relay_sig: Option<String>) -> Result<(), BingleError> { Ok(()) }
    fn lookup(&self, id: &str) -> Result<NetworkEndpoint, BingleError> {
        self.lookup_results.lock().unwrap().get(id).cloned().ok_or_else(|| BingleError::Other("not found".to_string()))
    }
    fn start_load_from_peer(&self, _peer_id: &str) -> Result<usize, BingleError> { Ok(0) }
}

#[test]
fn test_send_message_to_handle_success() {
    let (mock_dtls, sends) = MockDtls::new();
    let api = BingleApiImpl::new_with_dtls(Box::new(mock_dtls));
    
    let handle = "test_handle".to_string();
    let user_id = test_util::ADDRESS_RECEIVE.to_string();
    let dest_addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
    
    // Setup handle lookup mock
    let user_id_clone = user_id.clone();
    api.set_handle_lookup_mock_for_tests(Box::new(move |h| {
        if h == "test_handle" {
            Ok(Some(user_id_clone.clone()))
        } else {
            Ok(None)
        }
    }));
    
    // Setup DDB lookup for the user_id using MockDdbClient
    let ddb = Arc::new(MockDdbClient::new());
    ddb.set_lookup(user_id.clone(), NetworkEndpoint::new_direct(dest_addr));
    api.engine_set_ddb_client_for_tests(ddb);
    
    let msg = json!({"hello": "world"});
    let ok = api.send_message_to_handle(&handle, msg.clone(), None).unwrap();
    
    assert!(ok);
    let locked_sends = sends.lock().unwrap();
    assert_eq!(locked_sends.len(), 1);
    assert_eq!(locked_sends[0].0, dest_addr);
    assert_eq!(
        test_util::maybe_unwrap_data_single(&locked_sends[0].1),
        serde_json::to_vec(&msg).unwrap().as_slice()
    );
}

#[test]
fn test_send_message_to_handle_not_found() {
    let (mock_dtls, _) = MockDtls::new();
    let api = BingleApiImpl::new_with_dtls(Box::new(mock_dtls));
    
    api.set_handle_lookup_mock_for_tests(Box::new(|_| Ok(None)));
    
    let msg = json!({"hello": "world"});
    let ok = api.send_message_to_handle(&"unknown_handle".to_string(), msg, None).unwrap();
    
    assert!(!ok);
}

#[test]
fn test_send_message_to_handle_lookup_error() {
    let (mock_dtls, _) = MockDtls::new();
    let api = BingleApiImpl::new_with_dtls(Box::new(mock_dtls));
    
    api.set_handle_lookup_mock_for_tests(Box::new(|_| Err("Lookup failed".to_string())));
    
    let msg = json!({"hello": "world"});
    let ok = api.send_message_to_handle(&"any_handle".to_string(), msg, None);
    
    assert!(ok.is_err());
}

#[test]
fn test_send_message_to_handle_with_response_success() {
    let (mock_dtls, sends) = MockDtls::new();
    let api = BingleApiImpl::new_with_dtls(Box::new(mock_dtls));
    
    let handle = "test_handle".to_string();
    let user_id = test_util::ADDRESS_RECEIVE.to_string();
    let dest_addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
    
    // Setup handle lookup mock
    let user_id_clone = user_id.clone();
    api.set_handle_lookup_mock_for_tests(Box::new(move |h| {
        if h == "test_handle" {
            Ok(Some(user_id_clone.clone()))
        } else {
            Ok(None)
        }
    }));
    
    // Setup DDB lookup
    let ddb = Arc::new(MockDdbClient::new());
    ddb.set_lookup(user_id.clone(), NetworkEndpoint::new_direct(dest_addr));
    api.engine_set_ddb_client_for_tests(ddb);
    
    let msg = json!({"hello": "world"});
    let api_clone = api.clone();
    let sends_clone = sends.clone();
    
    // Background thread to fulfill the pending response
    std::thread::spawn(move || {
        // Wait a bit for the message to be sent and waiter registered
        for _ in 0..20 {
            {
                let sends_vec = sends_clone.lock().unwrap();
                if sends_vec.len() == 1 {
                    let sent_msg: serde_json::Value = serde_json::from_slice(
                        test_util::maybe_unwrap_data_single(&sends_vec[0].1),
                    )
                    .unwrap();
                    if let Some(tag_str) = sent_msg.get("responseTag").and_then(|t| t.as_str()) {
                        let tag = uuid::Uuid::parse_str(tag_str).unwrap();
                        api_clone.engine_for_tests().access(|e: &rust_comms::engine::Engine| e.fulfill_pending(&tag, json!({"response": "ok"})));
                        return;
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    });

    let res = api.send_message_to_handle_with_response(&handle, msg, None);
    
    assert!(res.is_ok());
    assert_eq!(res.unwrap().get("response").unwrap(), "ok");
}
