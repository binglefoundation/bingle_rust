use bingle_core::api::bingle_api::{
    BingleApi, BingleError, Handle, NetworkEndpoint, OnConnectHandler, OnListeningHandler,
    OnMessageHandler, ProgressCallback, StartOptions, UserId,
};
use bingle_core::blockchain::algo_ops::AlgoChainConfig;
use serde_json::{Value as JsonValue, json};
use std::sync::{Arc, Mutex};

pub struct MockBingleApi;

impl BingleApi for MockBingleApi {
    fn debug_print_options(&self) {}
    fn list_all_relays(
        &self,
        _include_self: bool,
    ) -> Vec<bingle_core::relay::relay_finder::RelayInfo> {
        Vec::new()
    }
    fn get_my_id(&self) -> Option<String> {
        None
    }
    fn get_user_id(&self) -> Option<String> {
        None
    }
    fn get_handle(&self) -> Option<String> {
        None
    }
    fn get_app_id(&self) -> Option<u64> {
        None
    }
    fn get_algo_provider_config(&self) -> Option<AlgoChainConfig> {
        None
    }
    fn start(&self, _options: &StartOptions) -> Result<(), BingleError> {
        Ok(())
    }
    fn stop(&self) {}
    fn network_change(&self) {}
    fn handle_lookup(&self, handle: &Handle) -> Result<Option<UserId>, BingleError> {
        if handle == "notfound" {
            Ok(None)
        } else {
            Ok(Some(format!("mock-id-{}", handle)))
        }
    }
    fn handle_lookup_partial(
        &self,
        handle: &Handle,
    ) -> Result<Option<(UserId, Handle)>, BingleError> {
        if handle == "notfound" {
            Ok(None)
        } else {
            Ok(Some((format!("mock-id-{}", handle), handle.clone())))
        }
    }
    fn handle_lookup_by_id(&self, _user_id: &UserId) -> Option<Handle> {
        None
    }
    fn send_message_to_id(
        &self,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        Ok(true)
    }
    fn send_message_to_handle(
        &self,
        _handle: &Handle,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        Ok(true)
    }
    fn send_message_to_network(
        &self,
        _nsk: &NetworkEndpoint,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        Ok(true)
    }
    fn send_message_to_id_with_response(
        &self,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, BingleError> {
        Ok(json!({"text": "stub response"}))
    }
    fn send_message_to_handle_with_response(
        &self,
        _handle: &Handle,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, BingleError> {
        Ok(json!({"text": "stub response"}))
    }
    fn send_message_to_network_with_response(
        &self,
        _nsk: &NetworkEndpoint,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, BingleError> {
        Ok(json!({"text": "stub response"}))
    }
    fn set_on_message(&self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&self, _handler: Option<Arc<OnConnectHandler>>) {}
    fn set_on_listening(&self, _handler: Option<Arc<OnListeningHandler>>) {}
}

/// A mock BingleApi that returns a handle and can resolve handle by id.
pub struct HandleMockBingleApi {
    pub handle: String,
    pub id_to_handle: std::collections::HashMap<String, String>,
}

impl HandleMockBingleApi {
    pub fn new(handle: String, id_to_handle: std::collections::HashMap<String, String>) -> Self {
        Self {
            handle,
            id_to_handle,
        }
    }
}

impl BingleApi for HandleMockBingleApi {
    fn debug_print_options(&self) {}
    fn list_all_relays(
        &self,
        _include_self: bool,
    ) -> Vec<bingle_core::relay::relay_finder::RelayInfo> {
        Vec::new()
    }
    fn get_my_id(&self) -> Option<String> {
        None
    }
    fn get_user_id(&self) -> Option<String> {
        None
    }
    fn get_handle(&self) -> Option<String> {
        Some(self.handle.clone())
    }
    fn get_app_id(&self) -> Option<u64> {
        None
    }
    fn get_algo_provider_config(&self) -> Option<AlgoChainConfig> {
        None
    }
    fn start(&self, _options: &StartOptions) -> Result<(), BingleError> {
        Ok(())
    }
    fn stop(&self) {}
    fn network_change(&self) {}
    fn handle_lookup(&self, handle: &Handle) -> Result<Option<UserId>, BingleError> {
        if handle == "notfound" {
            Ok(None)
        } else {
            Ok(Some(format!("mock-id-{}", handle)))
        }
    }
    fn handle_lookup_partial(
        &self,
        handle: &Handle,
    ) -> Result<Option<(UserId, Handle)>, BingleError> {
        if handle == "notfound" {
            Ok(None)
        } else {
            Ok(Some((format!("mock-id-{}", handle), handle.clone())))
        }
    }
    fn handle_lookup_by_id(&self, user_id: &UserId) -> Option<Handle> {
        self.id_to_handle.get(user_id).cloned()
    }
    fn send_message_to_id(
        &self,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        Ok(true)
    }
    fn send_message_to_handle(
        &self,
        _handle: &Handle,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        Ok(true)
    }
    fn send_message_to_network(
        &self,
        _nsk: &NetworkEndpoint,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        Ok(true)
    }
    fn send_message_to_id_with_response(
        &self,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, BingleError> {
        Ok(json!({"text": "stub response"}))
    }
    fn send_message_to_handle_with_response(
        &self,
        _handle: &Handle,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, BingleError> {
        Ok(json!({"text": "stub response"}))
    }
    fn send_message_to_network_with_response(
        &self,
        _nsk: &NetworkEndpoint,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, BingleError> {
        Ok(json!({"text": "stub response"}))
    }
    fn set_on_message(&self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&self, _handler: Option<Arc<OnConnectHandler>>) {}
    fn set_on_listening(&self, _handler: Option<Arc<OnListeningHandler>>) {}
}

/// A mock BingleApi that tracks whether start() has been called.
pub struct TrackingMockBingleApi {
    pub started: Arc<Mutex<bool>>,
}

/// A mock BingleApi that captures the StartOptions passed to start().
pub struct CapturingMockBingleApi {
    pub started: Arc<Mutex<bool>>,
    pub captured_opts: Arc<Mutex<Option<StartOptions>>>,
}

impl CapturingMockBingleApi {
    pub fn new(started: Arc<Mutex<bool>>, captured_opts: Arc<Mutex<Option<StartOptions>>>) -> Self {
        Self {
            started,
            captured_opts,
        }
    }
}

impl BingleApi for CapturingMockBingleApi {
    fn debug_print_options(&self) {}
    fn list_all_relays(
        &self,
        _include_self: bool,
    ) -> Vec<bingle_core::relay::relay_finder::RelayInfo> {
        Vec::new()
    }
    fn get_my_id(&self) -> Option<String> {
        None
    }
    fn get_user_id(&self) -> Option<String> {
        None
    }
    fn get_handle(&self) -> Option<String> {
        None
    }
    fn get_app_id(&self) -> Option<u64> {
        None
    }
    fn get_algo_provider_config(&self) -> Option<AlgoChainConfig> {
        None
    }
    fn start(&self, options: &StartOptions) -> Result<(), BingleError> {
        let mut s = self.started.lock().unwrap();
        *s = true;
        let mut c = self.captured_opts.lock().unwrap();
        *c = Some(options.clone());
        Ok(())
    }
    fn stop(&self) {}
    fn network_change(&self) {}
    fn handle_lookup(&self, _handle: &Handle) -> Result<Option<UserId>, BingleError> {
        Ok(None)
    }
    fn handle_lookup_partial(
        &self,
        _handle: &Handle,
    ) -> Result<Option<(UserId, Handle)>, BingleError> {
        Ok(None)
    }
    fn handle_lookup_by_id(&self, _user_id: &UserId) -> Option<Handle> {
        None
    }
    fn send_message_to_id(
        &self,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        Ok(true)
    }
    fn send_message_to_handle(
        &self,
        _handle: &Handle,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        Ok(true)
    }
    fn send_message_to_network(
        &self,
        _nsk: &NetworkEndpoint,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        Ok(true)
    }
    fn send_message_to_id_with_response(
        &self,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, BingleError> {
        Ok(json!({"text": "stub response"}))
    }
    fn send_message_to_handle_with_response(
        &self,
        _handle: &Handle,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, BingleError> {
        Ok(json!({"text": "stub response"}))
    }
    fn send_message_to_network_with_response(
        &self,
        _nsk: &NetworkEndpoint,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, BingleError> {
        Ok(json!({"text": "stub response"}))
    }
    fn set_on_message(&self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&self, _handler: Option<Arc<OnConnectHandler>>) {}
    fn set_on_listening(&self, _handler: Option<Arc<OnListeningHandler>>) {}
}

impl TrackingMockBingleApi {
    pub fn new(started: Arc<Mutex<bool>>) -> Self {
        Self { started }
    }
}

impl BingleApi for TrackingMockBingleApi {
    fn debug_print_options(&self) {}
    fn list_all_relays(
        &self,
        _include_self: bool,
    ) -> Vec<bingle_core::relay::relay_finder::RelayInfo> {
        Vec::new()
    }
    fn get_my_id(&self) -> Option<String> {
        None
    }
    fn get_user_id(&self) -> Option<String> {
        None
    }
    fn get_handle(&self) -> Option<String> {
        None
    }
    fn get_app_id(&self) -> Option<u64> {
        None
    }
    fn get_algo_provider_config(&self) -> Option<AlgoChainConfig> {
        None
    }
    fn start(&self, _options: &StartOptions) -> Result<(), BingleError> {
        let mut s = self.started.lock().unwrap();
        *s = true;
        Ok(())
    }
    fn stop(&self) {}
    fn network_change(&self) {}
    fn handle_lookup(&self, _handle: &Handle) -> Result<Option<UserId>, BingleError> {
        Ok(None)
    }
    fn handle_lookup_partial(
        &self,
        _handle: &Handle,
    ) -> Result<Option<(UserId, Handle)>, BingleError> {
        Ok(None)
    }
    fn handle_lookup_by_id(&self, _user_id: &UserId) -> Option<Handle> {
        None
    }
    fn send_message_to_id(
        &self,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        Ok(true)
    }
    fn send_message_to_handle(
        &self,
        _handle: &Handle,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        Ok(true)
    }
    fn send_message_to_network(
        &self,
        _nsk: &NetworkEndpoint,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        Ok(true)
    }
    fn send_message_to_id_with_response(
        &self,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, BingleError> {
        Ok(json!({"text": "stub response"}))
    }
    fn send_message_to_handle_with_response(
        &self,
        _handle: &Handle,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, BingleError> {
        Ok(json!({"text": "stub response"}))
    }
    fn send_message_to_network_with_response(
        &self,
        _nsk: &NetworkEndpoint,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, BingleError> {
        Ok(json!({"text": "stub response"}))
    }
    fn set_on_message(&self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&self, _handler: Option<Arc<OnConnectHandler>>) {}
    fn set_on_listening(&self, _handler: Option<Arc<OnListeningHandler>>) {}
}
