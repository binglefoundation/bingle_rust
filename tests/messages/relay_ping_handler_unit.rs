#![cfg(not(target_os = "ios"))]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkSourceKey, OnConnectHandler, OnMessageHandler, ProgressCallback, StartOptions, UserId};
use rust_comms::dtls::{Dtls, Result as DtlsResult, UdpNetworkMux, HandleMessage, HandlePeerCertificate};
use rust_comms::messages::handlers::MessageHandler;
use rust_comms::messages::relay_ping_handler::RelayPingHandler;
use rust_comms::messages::types::{Message, RelayMessage, RelayTriangleTest1};

#[derive(Clone)]
struct MockDtls {
    pub sends: Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>,
}
impl MockDtls { fn new() -> (Self, Arc<Mutex<Vec<(SocketAddr, Vec<u8>)>>>) { let v = Arc::new(Mutex::new(vec![])); (Self { sends: v.clone() }, v) } }
impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<UdpNetworkMux>) -> DtlsResult<()> { Ok(()) }
    fn stop(&mut self) -> DtlsResult<()> { Ok(()) }
    fn send(&self, to: SocketAddr, data: &[u8]) -> DtlsResult<()> { self.sends.lock().unwrap().push((to, data.to_vec())); Ok(()) }
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

#[derive(Clone)]
struct MockApi;
impl BingleApi for MockApi {
    fn get_my_id(&self) -> Option<String> { Some("MYID".to_string()) }
    fn start(&mut self, _options: StartOptions) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}
    fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _nsk: &NetworkSourceKey, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_network_with_response(&self, _nsk: &NetworkSourceKey, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
}

#[test]
fn relay_ping_handler_uses_api_get_my_id_for_checking_id() {
    // Arrange
    let (mock_dtls, sends) = MockDtls::new();
    let handler = RelayPingHandler::new(Arc::new(mock_dtls), Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 34567)));
    let api: Arc<dyn BingleApi> = Arc::new(MockApi);
    let t1 = RelayTriangleTest1 { app: None, checkingEndpoint: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345) };

    // Act: invoke handler directly
    handler.on_triangle_test1(api, "FROM-OTHER-ID", &t1);

    // Assert: one send to the configured peer, and TriangleTest2 has checkingId == "MYID"
    let locked = sends.lock().unwrap();
    assert_eq!(locked.len(), 1, "expected exactly one DTLS send");
    let (_to, data) = &locked[0];
    let txt = std::str::from_utf8(data).expect("utf8 json");
    let v: serde_json::Value = serde_json::from_str(txt).expect("json parse");
    assert_eq!(v.get("type").and_then(|vv| vv.as_str()), Some("TriangleTest2"));
    assert_eq!(v.get("checkingId").and_then(|vv| vv.as_str()), Some("MYID"));
}
