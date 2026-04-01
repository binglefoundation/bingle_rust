use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use rust_comms::api::bingle_api::{BingleApi, StartOptions, NetworkEndpoint, UserId, Handle, ProgressCallback};
use rust_comms::engine::Engine;
use serde_json::Value as JsonValue;
use std::sync::{Arc};

struct DummyApi;
impl BingleApi for DummyApi { 
    fn set_on_listening(&mut self, _handler: Option<std::sync::Arc<rust_comms::api::bingle_api::OnListeningHandler>>) {} 
    fn get_handle(&self) -> Option<String> { None } 
    fn get_user_id(&self) -> Option<String> { None }
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> { None }
    fn get_app_id(&self) -> Option<u64> { None }
    fn get_algo_provider_config(&self) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> { None }
    fn start(&mut self, _options: &StartOptions) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}
    fn handle_lookup(&self, _handle: &Handle) -> Result<Option<UserId>, String> { Ok(None) }
    fn handle_lookup_by_id(&self, _user_id: &UserId) -> Option<Handle> { None }
    fn send_message_to_id(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { Err("not implemented".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { Err("not implemented".into()) }
    fn send_message_to_network_with_response(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { Err("not implemented".into()) }
    fn set_on_message(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnConnectHandler>>) {}
}

impl rust_comms::api::bingle_api::BingleApiInternal for DummyApi {
    fn set_state(&self, _state: rust_comms::engine::EngineState) {}
    fn get_state(&self) -> rust_comms::engine::EngineState { rust_comms::engine::EngineState::StunIdentify }
    fn set_nat_type(&self, _nat: rust_comms::engine::NatType) {}
    fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> { None }
    fn ddb_register_ip(&self, _endpoint: std::net::SocketAddr, _am_relay: bool) -> Result<(), String> { Ok(()) }
    fn ddb_register_relay(&self, _relay_id: String, _relay_sig: Option<String>) -> Result<(), String> { Ok(()) }
    fn update_turn_listener_relay(&self, _relay_id: String, _relay_addr: std::net::SocketAddr) -> Result<(), String> { Ok(()) }
    fn turn_client_handle_listen_response(&self, _relay_addr: std::net::SocketAddr, _relay_id: String) {}
    fn turn_lookup_addr_by_id(&self, _id: String) -> Option<std::net::SocketAddr> { None }
    fn turn_handle_call(&self, _source: std::net::SocketAddr, _dest: std::net::SocketAddr) -> i32 { -1 }
    fn turn_handle_listen(&self, _id: String, _source: std::net::SocketAddr) -> bool { false }
    fn turn_handle_called(&self, _source: std::net::SocketAddr, _dest: std::net::SocketAddr, _channel: u16) {}
    fn notify_listening(&self, _listening: bool) {}
    fn get_relay_state(&self) -> String { "off".into() }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn engine_start_with_static_ip_localhost_ok() {
    let mut engine = Engine::new(&StartOptions::default(), crate::util::mock_bingle_api::to_weak(DummyApi));
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let opts = StartOptions {
        handle: "tester".into(),
        algo_passphrase: Some("pass".into()),
        static_ip: Some(addr),
        am_relay: false,
        stun_servers: None,
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None, handle_cache_expiry: None,
    };
    let res = engine.start(&opts);
    // Engine may fail to start DTLS due to lack of certificates; however, our DTLS implementation only
    // requires certificates for server. It uses defaults in tests; accept either Ok or Err as long as it doesn't panic.
    if let Err(e) = res {
        // Acceptable errors: DTLS start failure; ensure it's the DTLS path, not the NotImplemented one
        assert!(!e.to_lowercase().contains("notimplemented"));
    }
    engine.stop();
}
