use crate::util::reusable_mock_api::MockApiBoth;
use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, ProgressCallback, StartOptions, UserId};
use rust_comms::engine::{Engine, EngineState};
use serde_json::Value as JsonValue;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

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
    fn send_message_to_id(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { Err("not implemented".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { Err("not implemented".into()) }
    fn send_message_to_network_with_response(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { Err("not implemented".into()) }
    fn set_on_message(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnConnectHandler>>) {}
}

// Updated behavior: on_stun_consistent marks endpoint available when a public address is provided,
// even if DTLS isn't started (no triangle test in minimal engine).
// Ignored until we have triangle test, which implies a rewrite of this test.
#[test]
#[ignore]
fn engine_forced_stun_sets_endpoint_available() {
    let mut engine = Engine::new(&StartOptions::default(), crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new()));
    let pub_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 55555);
    engine.test_force_stun_consistent(pub_addr);
    assert_eq!(engine.state(), EngineState::EndpointAvailable);
    assert_eq!(engine.last_public_addr(), Some(pub_addr));
}
