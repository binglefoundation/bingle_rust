use std::sync::Arc;

use bingle_core::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, StartOptions, UserId};

#[allow(dead_code)]
struct MockApi;
impl BingleApi for MockApi {
    fn list_all_relays(
        &self,
        _include_self: bool,
    ) -> Vec<bingle_core::relay::relay_finder::RelayInfo> {
        Vec::new()
    }
    fn set_on_listening(
        &self,
        _handler: Option<std::sync::Arc<bingle_core::api::bingle_api::OnListeningHandler>>,
    ) {
    }
    fn get_algo_provider_config(&self) -> Option<algo_ops::AlgoChainConfig> {
        None
    }
    fn get_handle(&self) -> Option<String> {
        None
    }
    fn get_user_id(&self) -> Option<String> {
        None
    }
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> {
        None
    }
    fn get_app_id(&self) -> Option<u64> {
        None
    }
    fn start(
        &self,
        _options: &StartOptions,
    ) -> Result<(), bingle_core::api::bingle_api::BingleError> {
        Ok(())
    }
    fn stop(&self) {}
    fn network_change(&self) {}
    fn handle_lookup(
        &self,
        _handle: &Handle,
    ) -> Result<Option<UserId>, bingle_core::api::bingle_api::BingleError> {
        Ok(None)
    }
    fn handle_lookup_partial(
        &self,
        _handle: &Handle,
    ) -> Result<Option<(UserId, Handle)>, bingle_core::api::bingle_api::BingleError> {
        Ok(None)
    }
    fn handle_lookup_by_id(&self, _user_id: &UserId) -> Option<Handle> {
        None
    }
    fn send_message_to_id(
        &self,
        _user_id: &UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
        Ok(true)
    }
    fn send_message_to_handle(
        &self,
        _handle: &Handle,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
        Ok(true)
    }
    fn send_message_to_network(
        &self,
        _network_source_key: &NetworkEndpoint,
        _user_id: &UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
        Ok(true)
    }
    fn send_message_to_id_with_response(
        &self,
        _user_id: &UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
        Err(bingle_core::api::bingle_api::BingleError::Other(
            "not needed".into(),
        ))
    }
    fn send_message_to_handle_with_response(
        &self,
        _handle: &Handle,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
        Err(bingle_core::api::bingle_api::BingleError::Other(
            "not needed".into(),
        ))
    }
    fn send_message_to_network_with_response(
        &self,
        _network_source_key: &NetworkEndpoint,
        _user_id: &UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, bingle_core::api::bingle_api::BingleError> {
        Ok(serde_json::json!({"app": null, "type": "CheckResponse", "state": "available"}))
    }
    fn set_on_message(
        &self,
        _handler: Option<Arc<bingle_core::api::bingle_api::OnMessageHandler>>,
    ) {
    }
    fn set_on_connect(
        &self,
        _handler: Option<Arc<bingle_core::api::bingle_api::OnConnectHandler>>,
    ) {
    }
}
