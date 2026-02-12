use std::sync::{Arc, Mutex, Weak};

use rust_comms::api::bingle_api::BingleApi;

#[derive(Clone)]
pub struct MockApi;

impl rust_comms::api::bingle_api::BingleApi for MockApi {
    fn set_on_listening(
        &mut self,
        _handler: Option<std::sync::Arc<rust_comms::api::bingle_api::OnListeningHandler>>,
    ) {
    }
    fn get_algo_provider_config(
        &self,
    ) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> {
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
        &mut self,
        _options: &rust_comms::api::bingle_api::StartOptions,
    ) -> Result<(), String> {
        Ok(())
    }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}

    fn send_message_to_id(
        &self,
        _user_id: &rust_comms::api::bingle_api::UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> bool {
        false
    }
    fn send_message_to_handle(
        &self,
        _handle: &rust_comms::api::bingle_api::Handle,
        _message: serde_json::Value,
        _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> bool {
        false
    }
    fn send_message_to_network(
        &self,
        _network_source_key: &rust_comms::api::bingle_api::NetworkEndpoint,
        _user_id: &rust_comms::api::bingle_api::UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> bool {
        false
    }

    fn send_message_to_id_with_response(
        &self,
        _user_id: &rust_comms::api::bingle_api::UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, String> {
        Err("ni".into())
    }
    fn send_message_to_handle_with_response(
        &self,
        _handle: &rust_comms::api::bingle_api::Handle,
        _message: serde_json::Value,
        _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, String> {
        Err("ni".into())
    }
    fn send_message_to_network_with_response(
        &self,
        _network_source_key: &rust_comms::api::bingle_api::NetworkEndpoint,
        _user_id: &rust_comms::api::bingle_api::UserId,
        _message: serde_json::Value,
        _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, String> {
        Err("ni".into())
    }

    fn set_on_message(
        &mut self,
        _handler: Option<Arc<rust_comms::api::bingle_api::OnMessageHandler>>,
    ) {
    }
    fn set_on_connect(
        &mut self,
        _handler: Option<Arc<rust_comms::api::bingle_api::OnConnectHandler>>,
    ) {
    }
}

/// Test helper: wrap a concrete `BingleApi` into a leaked `Arc<Mutex<dyn BingleApi>>` and return a `Weak`.
/// This mirrors the helper used elsewhere in tests, but is scoped under `crate::util::mock_api`.
pub fn to_weak<T: BingleApi + 'static>(api: T) -> Weak<Mutex<dyn BingleApi>> {
    let arc: Arc<Mutex<dyn BingleApi>> = Arc::new(Mutex::new(api));
    let weak = Arc::downgrade(&arc);

    // Leak the Arc to keep it alive for the duration of the test process.
    Box::leak(Box::new(arc));

    weak
}
