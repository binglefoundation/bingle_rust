use std::net::SocketAddr;
use rust_comms::api::bingle_api::{BingleApi, StartOptions, NetworkEndpoint, UserId, Handle, ProgressCallback};
use rust_comms::engine::Engine;
use serde_json::Value as JsonValue;
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

#[cfg(not(target_os = "ios"))]
use rust_comms::dtls::DtlsOpenSsl;

#[test]
fn engine_dtls_is_none_before_configuration() {
    let engine = Engine::new(&StartOptions::default(), crate::util::mock_bingle_api::to_weak(DummyApi));
    assert!(engine.dtls().is_none(), "Engine::dtls() should be None when DTLS not configured");
}

#[cfg(not(target_os = "ios"))]
#[test]
fn engine_dtls_send_without_start_fails() {
    let mut engine = Engine::new(&StartOptions::default(), crate::util::mock_bingle_api::to_weak(DummyApi));
    // Provide a DTLS instance but DO NOT call engine.start(); ensure direct send fails.
    engine.set_dtls(Box::new(DtlsOpenSsl::new()));

    let to: SocketAddr = "127.0.0.1:9".parse().unwrap(); // discard port
    let res = engine
        .dtls()
        .expect("dtls should be present")
        .send(&rust_comms::api::bingle_api::NetworkEndpoint::new_direct(to), b"hello");
    assert!(res.is_err(), "Dtls::send should error when DTLS was not started");
    let msg = res.err().unwrap();
    // Accept common error messages
    assert!(msg.contains("not started") || msg.to_lowercase().contains("requires start") || msg.to_lowercase().contains("bind"),
        "unexpected error message: {}", msg);
}
