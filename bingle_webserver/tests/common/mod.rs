use std::sync::Arc;
use serde_json::{json, Value as JsonValue};
use rust_comms::api::bingle_api::{BingleApi, StartOptions, UserId, Handle, NetworkEndpoint, ProgressCallback, OnMessageHandler, OnConnectHandler, OnListeningHandler};
use rust_comms::blockchain::algo_ops::AlgoChainConfig;

pub struct MockBingleApi;

impl BingleApi for MockBingleApi {
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> { None }
    fn get_user_id(&self) -> Option<String> { None }
    fn get_handle(&self) -> Option<String> { None }
    fn get_app_id(&self) -> Option<u64> { None }
    fn get_algo_provider_config(&self) -> Option<AlgoChainConfig> { None }
    fn start(&mut self, _options: &StartOptions) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}
    fn handle_lookup(&self, _handle: &Handle) -> Result<Option<UserId>, String> { Ok(None) }
    fn handle_lookup_by_id(&self, _user_id: &UserId) -> Option<Handle> { None }
    fn send_message_to_id(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool { true }
    fn send_message_to_handle(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool { true }
    fn send_message_to_network(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool { true }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { Ok(json!({"text": "stub response"})) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { Ok(json!({"text": "stub response"})) }
    fn send_message_to_network_with_response(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { Ok(json!({"text": "stub response"})) }
    fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
    fn set_on_listening(&mut self, _handler: Option<Arc<OnListeningHandler>>) {}
}
