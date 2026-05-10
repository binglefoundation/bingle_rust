use std::sync::Arc;

use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::dtls::{Dtls, HandleMessage, HandlePeerCertificate, Result as DtlsResult};
use rust_comms::engine::Engine;
use rust_comms::relay::relay_finder::RootRelayInfo;

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
    fn set_app_layer_only_verification(&mut self, _enabled: bool) {}
    fn with_app_layer_only_verification(self, _enabled: bool) -> Self where Self: Sized { self }
    fn set_dangerous_debug(&mut self, _enabled: bool) {}
    fn with_dangerous_debug(self, _enabled: bool) -> Self where Self: Sized { self }
}

#[derive(Clone)]
struct MockApi;
impl BingleApi for MockApi { 
    fn list_all_relays(&self, _include_self: bool) -> Vec<rust_comms::relay::relay_finder::RelayInfo> { Vec::new() }
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
    fn handle_lookup_by_id(&self, _user_id: &rust_comms::api::bingle_api::UserId) -> Option<rust_comms::api::bingle_api::Handle> { None }
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
    fn get_relay_state(&self) -> String { "off".to_string() }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn engine_upserts_root_relays_into_backend() {
    // Engine with am_relay true
    let opts = StartOptions { handle: "eng".into(), algo_passphrase: None, static_ip: Some("127.0.0.1:0".parse().unwrap()), am_relay: true, stun_servers: None, algo_provider_config: None, algo_network: None, app_id: None, asset_id: None, log_level: None, handle_cache_expiry: None , dangerous_debug: true, log_mode: rust_comms::util::logging::LogMode::Plain };
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
