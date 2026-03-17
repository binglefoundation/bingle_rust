

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::engine::Engine;
use rust_comms::relay::relay_finder::RootRelayInfo;
use rust_comms::dtls::{Dtls, HandleMessage, HandlePeerCertificate, Result as DtlsResult};

// Minimal DTLS mock
#[derive(Clone)]
struct MockDtls;
impl Dtls for MockDtls {
    fn start(&mut self, _mux: Arc<rust_comms::dtls::UdpNetworkMux>) -> DtlsResult<()> { Ok(()) }
    fn stop(&mut self) -> DtlsResult<()> { Ok(()) }
    fn send(&self, _to: &rust_comms::api::bingle_api::NetworkEndpoint, _data: &[u8]) -> DtlsResult<()> { Ok(()) }
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
    fn set_on_listening(&mut self, _handler: Option<std::sync::Arc<rust_comms::api::bingle_api::OnListeningHandler>>) {} 
    fn get_user_id(&self) -> Option<String> { None }
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> { None }
    fn get_handle(&self) -> Option<String> { None }
    fn get_app_id(&self) -> Option<u64> { None }
    fn get_algo_provider_config(&self) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> { None }
    fn start(&mut self, _options: &StartOptions) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}
    fn handle_lookup(&self, _handle: &rust_comms::api::bingle_api::Handle) -> Result<Option<rust_comms::api::bingle_api::UserId>, String> { Ok(None) }
    fn send_message_to_id(&self, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &rust_comms::api::bingle_api::Handle, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _nsk: &rust_comms::api::bingle_api::NetworkEndpoint, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &rust_comms::api::bingle_api::Handle, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_network_with_response(&self, _nsk: &rust_comms::api::bingle_api::NetworkEndpoint, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn set_on_message(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnConnectHandler>>) {}
}

impl rust_comms::api::bingle_api::BingleApiInternal for MockApi {
    fn set_state(&self, _s: rust_comms::engine::EngineState) {}
    fn get_state(&self) -> rust_comms::engine::EngineState { rust_comms::engine::EngineState::StunIdentify }
    fn set_nat_type(&self, _n: rust_comms::engine::NatType) {}
    fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> { None }
    fn ddb_register_ip(&self, _e: std::net::SocketAddr, _a: bool) -> Result<(), String> { Ok(()) }
    fn ddb_register_relay(&self, _r: String, _s: Option<String>) -> Result<(), String> { Ok(()) }
    fn update_turn_listener_relay(&self, _r: String, _a: std::net::SocketAddr) -> Result<(), String> { Ok(()) }
    fn turn_client_handle_listen_response(&self, _a: std::net::SocketAddr, _r: String) {}
    fn turn_lookup_addr_by_id(&self, _i: String) -> Option<std::net::SocketAddr> { None }
    fn turn_handle_call(&self, _s: std::net::SocketAddr, _d: std::net::SocketAddr) -> i32 { -1 }
    fn turn_handle_listen(&self, _i: String, _s: std::net::SocketAddr) -> bool { false }
    fn turn_handle_called(&self, _s: std::net::SocketAddr, _d: std::net::SocketAddr, _c: u16) {}
    fn notify_listening(&self, _l: bool) {}
    fn get_relay_state(&self) -> String { "off".into() }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn engine_upserts_root_relays_into_backend() {
    // Engine with am_relay true
    let opts = StartOptions { handle: "eng".into(), algo_passphrase: None, static_ip: Some("127.0.0.1:0".parse().unwrap()), am_relay: true, stun_servers: None, algo_provider_config: None, algo_network: None, app_id: None, asset_id: None, log_level: None };
    let mut eng = Engine::new(&opts, crate::util::mock_bingle_api::mock_api_weak());
    eng.set_dtls(Box::new(MockDtls));
    // Need a router to avoid nulls in start; use minimal MockApi
    let router = Arc::new(rust_comms::messages::router::Router::new(crate::util::mock_bingle_api::to_weak(MockApi)));
    eng.set_router(router);

    // Use test helper to upsert roots (avoid network/indexer dependencies)
    let roots = vec![
        RootRelayInfo { id: "RID1".into(), address: "127.0.0.1:6001".parse().unwrap(), state: None },
        RootRelayInfo { id: "RID2".into(), address: "127.0.0.1:6002".parse().unwrap(), state: None },
    ];
    eng.upsert_root_relays_for_tests(roots);

    // Validate DDB backend contains records
    let rec1 = eng.ddb_backend_lookup_for_tests("RID1");
    assert!(rec1.is_some());
    let rec2 = eng.ddb_backend_lookup_for_tests("RID2");
    assert!(rec2.is_some());
}
