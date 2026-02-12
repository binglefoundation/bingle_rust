use std::sync::{Arc, Mutex, Weak};
use serde_json::Value as JsonValue;
use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, OnConnectHandler, OnMessageHandler, ProgressCallback, StartOptions, UserId, OnListeningHandler};
use rust_comms::blockchain::algo_ops::AlgoChainConfig;

pub struct MockBingleApi;

pub fn mock_api_weak() -> Weak<Mutex<dyn BingleApi>> {
    to_weak(MockBingleApi)
}

pub fn to_weak<T: BingleApi + 'static>(api: T) -> Weak<Mutex<dyn BingleApi>> {
    let arc: Arc<Mutex<dyn BingleApi>> = Arc::new(Mutex::new(api));
    let weak = Arc::downgrade(&arc);
    // Leak the Arc to keep it alive for the duration of the test
    Box::leak(Box::new(arc));
    weak
}

pub fn arc_to_weak(api: Arc<dyn BingleApi>) -> Weak<Mutex<dyn BingleApi>> {
    to_weak(ArcAsApi(api))
}

struct ArcAsApi(Arc<dyn BingleApi>);
impl BingleApi for ArcAsApi {
    fn debug_print_options(&self) { self.0.debug_print_options() }
    fn get_my_id(&self) -> Option<String> { self.0.get_my_id() }
    fn get_user_id(&self) -> Option<String> { self.0.get_user_id() }
    fn get_handle(&self) -> Option<String> { self.0.get_handle() }
    fn get_app_id(&self) -> Option<u64> { self.0.get_app_id() }
    fn get_algo_provider_config(&self) -> Option<AlgoChainConfig> { self.0.get_algo_provider_config() }
    fn start(&mut self, _options: &StartOptions) -> Result<(), String> { panic!("not supported") }
    fn stop(&mut self) { }
    fn network_change(&mut self) { }
    fn send_message_to_id(&self, user_id: &UserId, message: JsonValue, progress: Option<Arc<ProgressCallback>>) -> bool { self.0.send_message_to_id(user_id, message, progress) }
    fn send_message_to_handle(&self, handle: &Handle, message: JsonValue, progress: Option<Arc<ProgressCallback>>) -> bool { self.0.send_message_to_handle(handle, message, progress) }
    fn send_message_to_network(&self, nsk: &NetworkEndpoint, user_id: &UserId, message: JsonValue, progress: Option<Arc<ProgressCallback>>) -> bool { self.0.send_message_to_network(nsk, user_id, message, progress) }
    fn send_message_to_id_with_response(&self, user_id: &UserId, message: JsonValue, progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { self.0.send_message_to_id_with_response(user_id, message, progress) }
    fn send_message_to_handle_with_response(&self, handle: &Handle, message: JsonValue, progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { self.0.send_message_to_handle_with_response(handle, message, progress) }
    fn send_message_to_network_with_response(&self, nsk: &NetworkEndpoint, user_id: &UserId, message: JsonValue, progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { self.0.send_message_to_network_with_response(nsk, user_id, message, progress) }
    fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) { }
    fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) { }
    fn set_on_listening(&mut self, _handler: Option<Arc<OnListeningHandler>>) { }
}

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
    fn send_message_to_id(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { Err("not implemented".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { Err("not implemented".into()) }
    fn send_message_to_network_with_response(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { Err("not implemented".into()) }
    fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
    fn set_on_listening(&mut self, _handler: Option<Arc<OnListeningHandler>>) {}
}
