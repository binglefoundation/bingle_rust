use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use rust_comms::dtls::dtls_trait::{Dtls, HandleMessage, HandlePeerCertificate, Result as DtlsResult};
use rust_comms::dtls::UdpNetworkMux;
use rust_comms::messages::*;
use rust_comms::messages::handlers::MessageHandler;
use rust_comms::api::bingle_api::{BingleApi, BingleApiBoth, StartOptions, Handle, NetworkEndpoint, UserId, ProgressCallback, OnMessageHandler, OnConnectHandler};

#[derive(Default, Clone)]
struct MockDtls {
    pub sent: Arc<std::sync::Mutex<Vec<(SocketAddr, Vec<u8>)>>>,
}

impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<UdpNetworkMux>) -> DtlsResult<()> { Ok(()) }
    fn stop(&mut self) -> DtlsResult<()> { Ok(()) }
    fn send(&self, to: &rust_comms::api::bingle_api::NetworkEndpoint, data: &[u8]) -> DtlsResult<()> {
        let mut g = self.sent.lock().unwrap();
        let addr = to.inet_socket_address().expect("MockDtls::send requires inet_socket_address");
        g.push((addr, data.to_vec()));
        Ok(())
    }
    fn get_handle_message(&self) -> Option<HandleMessage> { None }
    fn set_handle_message(&mut self, _handler: Option<HandleMessage>) {}
    fn with_handle_message(self, _handler: HandleMessage) -> Self { self }
    fn get_handle_peer_certificate(&self) -> Option<HandlePeerCertificate> { None }
    fn set_handle_peer_certificate(&mut self, _handler: Option<HandlePeerCertificate>) {}
    fn with_handle_peer_certificate(self, _handler: HandlePeerCertificate) -> Self { self }
    fn get_ca_cert(&self) -> Option<&[u8]> { None }
    fn set_ca_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_ca_cert(self, _pem: Vec<u8>) -> Self { self }
    fn get_client_cert(&self) -> Option<&[u8]> { None }
    fn set_client_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_cert(self, _pem: Vec<u8>) -> Self { self }
    fn get_client_private_key(&self) -> Option<&[u8]> { None }
    fn set_client_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_client_private_key(self, _pem: Vec<u8>) -> Self { self }
    fn get_server_signing_cert(&self) -> Option<&[u8]> { None }
    fn set_server_signing_cert(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_cert(self, _pem: Vec<u8>) -> Self { self }
    fn get_server_signing_private_key(&self) -> Option<&[u8]> { None }
    fn set_server_signing_private_key(&mut self, _pem: Option<Vec<u8>>) {}
    fn with_server_signing_private_key(self, _pem: Vec<u8>) -> Self { self }
}

#[test]
fn on_triangle_test1_sends_triangle_test2_to_peer() {
    let mock = Arc::new(MockDtls::default());
    let peer: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let handler = RelayPingHandler::new(mock.clone(), Some(peer));

    let t1 = RelayTriangleTest1 { app: None, checking_endpoint: "127.0.0.1:12345".parse().unwrap() };
    // Construct minimal API and FromStruct
    struct MockApi;
    impl rust_comms::api::bingle_api::BingleApiInternal for MockApi {
        fn get_relay_state(&self) -> String { "off".to_string() }
        fn set_state(&self, _s: rust_comms::engine::EngineState) {}
        fn get_state(&self) -> rust_comms::engine::EngineState { rust_comms::engine::EngineState::StunIdentify }
        fn set_nat_type(&self, _nat: rust_comms::engine::NatType) {}
        fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> { None }
        fn ddb_register_ip(&self, _endpoint: std::net::SocketAddr) -> Result<(), String> { Err("ni".into()) }
        fn ddb_register_relay(&self, _relay_id: String, _relay_sig: Option<String>) -> Result<(), String> { Err("ni".into()) }
        fn update_turn_listener_relay(&self, _relay_id: String, _relay_addr: std::net::SocketAddr) -> Result<(), String> { Err("ni".into()) }
        fn turn_client_handle_listen_response(&self, _relay_addr: std::net::SocketAddr, _relay_id: String) { }
        fn turn_lookup_addr_by_id(&self, _id: String) -> Option<std::net::SocketAddr> { None }
        fn turn_handle_call(&self, _source: std::net::SocketAddr, _dest: std::net::SocketAddr) -> i32 { -1 }
        fn turn_handle_listen(&self, _id: String, _source: std::net::SocketAddr) -> bool { false }
        fn turn_handle_called(&self, _source: std::net::SocketAddr, _dest: std::net::SocketAddr, _channel: u16) {}
        fn notify_listening(&self, _listening: bool) {}
    }
    impl BingleApi for MockApi { 
    fn get_handle(&self) -> Option<String> { None } 
    fn set_on_listening(&mut self, _handler: Option<std::sync::Arc<rust_comms::api::bingle_api::OnListeningHandler>>) {} 
    fn get_algo_provider_config(&self) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> { None } 
    fn get_user_id(&self) -> Option<String> { None } 
    fn debug_print_options(&self) {}
        fn get_my_id(&self) -> Option<String> { Some("ME".into()) }
        fn get_app_id(&self) -> Option<u64> { None }
        fn start(&mut self, _options: &StartOptions) -> Result<(), String> { Ok(()) }
        fn stop(&mut self) {}
        fn network_change(&mut self) {}
        fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
        fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
        fn send_message_to_network(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
        fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
        fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
        fn send_message_to_network_with_response(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
        fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
        fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
    }
    let api: Arc<dyn BingleApiBoth> = Arc::new(MockApi);
    let from = rust_comms::messages::handlers::FromStruct { id: "FROM".into(), network_source_key: NetworkEndpoint::new_direct("127.0.0.1:1".parse().unwrap()) };
    handler.on_triangle_test1(api, &from, &t1);

    let records = mock.sent.lock().unwrap().clone();
    assert_eq!(records.len(), 1);
    let (to, data) = &records[0];
    assert_eq!(*to, peer);

    let text = std::str::from_utf8(&data).expect("utf8");
    let msg = from_json_str(text).expect("decode");
    match msg {
        Message::Relay(RelayMessage::TriangleTest2(m)) => {
            assert_eq!(m.checking_endpoint.to_string(), "127.0.0.1:12345");
        }
        other => panic!("expected TriangleTest2, got {:?}", other),
    }
}

#[test]
fn on_triangle_test2_sends_triangle_test3_to_endpoint() {
    let mock = Arc::new(MockDtls::default());
    let handler = RelayPingHandler::new(mock.clone(), None);

    let endpoint: SocketAddr = "127.0.0.1:33333".parse().unwrap();
    let t2 = RelayTriangleTest2 { app: None, checking_id: "id-abc".into(), checking_endpoint: endpoint };
    // Minimal API and FromStruct
    struct MockApi;
    impl rust_comms::api::bingle_api::BingleApiInternal for MockApi {
        fn get_relay_state(&self) -> String { "off".to_string() }
        fn set_state(&self, _s: rust_comms::engine::EngineState) {}
        fn get_state(&self) -> rust_comms::engine::EngineState { rust_comms::engine::EngineState::StunIdentify }
        fn set_nat_type(&self, _nat: rust_comms::engine::NatType) {}
        fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> { None }
        fn ddb_register_ip(&self, _endpoint: std::net::SocketAddr) -> Result<(), String> { Err("ni".into()) }
        fn ddb_register_relay(&self, _relay_id: String, _relay_sig: Option<String>) -> Result<(), String> { Err("ni".into()) }
        fn update_turn_listener_relay(&self, _relay_id: String, _relay_addr: std::net::SocketAddr) -> Result<(), String> { Err("ni".into()) }
        fn turn_client_handle_listen_response(&self, _relay_addr: std::net::SocketAddr, _relay_id: String) { }
        fn turn_lookup_addr_by_id(&self, _id: String) -> Option<std::net::SocketAddr> { None }
        fn turn_handle_call(&self, _source: std::net::SocketAddr, _dest: std::net::SocketAddr) -> i32 { -1 }
        fn turn_handle_listen(&self, _id: String, _source: std::net::SocketAddr) -> bool { false }
        fn turn_handle_called(&self, _source: std::net::SocketAddr, _dest: std::net::SocketAddr, _channel: u16) {}
        fn notify_listening(&self, _listening: bool) {}
    }
    impl BingleApi for MockApi { 
    fn get_handle(&self) -> Option<String> { None } 
    fn set_on_listening(&mut self, _handler: Option<std::sync::Arc<rust_comms::api::bingle_api::OnListeningHandler>>) {} 
    fn get_algo_provider_config(&self) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> { None } 
    fn get_user_id(&self) -> Option<String> { None } 
    fn debug_print_options(&self) {}
        fn get_my_id(&self) -> Option<String> { Some("ME".into()) }
        fn get_app_id(&self) -> Option<u64> { None }
        fn start(&mut self, _options: &StartOptions) -> Result<(), String> { Ok(()) }
        fn stop(&mut self) {}
        fn network_change(&mut self) {}
        fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
        fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
        fn send_message_to_network(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
        fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
        fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
        fn send_message_to_network_with_response(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
        fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
        fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
    }
    let api: Arc<dyn BingleApiBoth> = Arc::new(MockApi);
    let from = rust_comms::messages::handlers::FromStruct { id: "FROM".into(), network_source_key: NetworkEndpoint::new_direct("127.0.0.1:1".parse().unwrap()) };
    handler.on_triangle_test2(api, &from, &t2);

    let records = mock.sent.lock().unwrap().clone();
    assert_eq!(records.len(), 1);
    let (to, data) = &records[0];
    assert_eq!(*to, endpoint);

    let text = std::str::from_utf8(&data).expect("utf8");
    let msg = from_json_str(text).expect("decode");
    match msg {
        Message::Relay(RelayMessage::TriangleTest3(_)) => {}
        other => panic!("expected TriangleTest3, got {:?}", other),
    }
}
