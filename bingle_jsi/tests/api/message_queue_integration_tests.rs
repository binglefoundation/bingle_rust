use std::sync::{Arc, Mutex};
use std::time::Duration;
use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::BingleJsiApiImpl;
use rust_comms::api::bingle_api::{BingleApi, BingleApiInternal, BingleError, ProgressCallback, UserId, Handle, StartOptions, OnMessageHandler, OnConnectHandler, OnListeningHandler};
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use rust_comms::api::network_endpoint::NetworkEndpoint;

struct MockBingleApi {
    pub progress_steps: Vec<u8>,
    pub on_listening: Mutex<Option<Arc<OnListeningHandler>>>,
}

impl BingleApiInternal for MockBingleApi {
    fn get_relay_state(&self) -> String { "off".to_string() }
    fn notify_listening(&self, listening: bool, nat_type: rust_comms::engine::NatType) {
        if let Ok(guard) = self.on_listening.lock() {
            if let Some(handler) = guard.as_ref() {
                handler(listening, nat_type);
            }
        }
    }
}

impl BingleApi for MockBingleApi {
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> { Some("test-id".to_string()) }
    fn get_user_id(&self) -> Option<String> { Some("test-id".to_string()) }
    fn get_handle(&self) -> Option<String> { Some("testuser".to_string()) }
    fn get_app_id(&self) -> Option<u64> { None }
    fn get_algo_provider_config(&self) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> { None }
    fn start(&mut self, _: &StartOptions) -> Result<(), BingleError> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}

    fn list_all_relays(&self, _: bool) -> Vec<rust_comms::relay::relay_finder::RelayInfo> { vec![] }
    fn handle_lookup(&self, _: &Handle) -> Result<Option<UserId>, BingleError> { Ok(Some("test-id".to_string())) }
    fn handle_lookup_by_id(&self, _: &UserId) -> Option<Handle> { Some("testuser".to_string()) }

    fn send_message_to_handle(
        &self,
        _handle: &Handle,
        _payload: serde_json::Value,
        progress_callback: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, BingleError> {
        if let Some(cb) = progress_callback {
            for &step in &self.progress_steps {
                cb(step, format!("Step {}%", step));
            }
        }
        Ok(true)
    }

    fn send_message_to_id(&self, _: &UserId, _: serde_json::Value, _: Option<Arc<ProgressCallback>>) -> Result<bool, BingleError> { Ok(true) }
    fn send_message_to_network(&self, _: &NetworkEndpoint, _: &UserId, _: serde_json::Value, _: Option<Arc<ProgressCallback>>) -> Result<bool, BingleError> { Ok(true) }
    
    fn send_message_to_id_with_response(&self, _: &UserId, _: serde_json::Value, _: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, BingleError> { Ok(serde_json::json!({})) }
    fn send_message_to_handle_with_response(&self, _: &Handle, _: serde_json::Value, _: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, BingleError> { Ok(serde_json::json!({})) }
    fn send_message_to_network_with_response(&self, _: &NetworkEndpoint, _: &UserId, _: serde_json::Value, _: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, BingleError> { Ok(serde_json::json!({})) }

    fn set_on_message(&mut self, _: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _: Option<Arc<OnConnectHandler>>) {}
    fn set_on_listening(&mut self, handler: Option<Arc<OnListeningHandler>>) {
        if let Ok(mut guard) = self.on_listening.lock() {
            *guard = handler;
        }
    }
}

#[test]
fn test_message_queue_with_mock_progress() {
    let mock_api = Arc::new(MockBingleApi {
        progress_steps: vec![10, 50, 90],
        on_listening: Mutex::new(None),
    });
    
    let local_api: Arc<Mutex<Box<dyn BingleLocalApi>>> = Arc::new(Mutex::new(Box::new(BingleApiLocalImpl::new(LocalApiConfig::default()))));
    
    let jsi = BingleJsiApiImpl::init_for_tests(mock_api, Some(local_api.clone()));
    
    // 1. Manually add a message and make it pending
    let timestamp = 999i64;
    {
        let mut guard = local_api.lock().unwrap();
        guard.add_message("testuser".to_string(), vec!["recipient".to_string()], timestamp, "Hello".to_string(), None).unwrap();
        guard.update_message_status(timestamp, 0.0, None).unwrap();
    }

    // 2. Start the JSI (starts background loop)
    jsi.start().unwrap();
    
    // 3. Simulate listening state
    jsi.api_for_tests().notify_listening(true, rust_comms::engine::NatType::Restricted);

    // 4. Wait for processing loop. It sleeps for 5s, so we need some time.
    // We expect progress to go through 0.1, 0.5, 0.9 and finally 1.0.
    
    let mut reached_0_5 = false;
    let mut reached_1_0 = false;
    
    for _ in 0..15 {
        std::thread::sleep(Duration::from_millis(1000));
        let msgs = jsi.get_messages().unwrap();
        if let Some(msg) = msgs.iter().find(|m| m.timestamp == timestamp) {
            if msg.progress.map_or(false, |p| p >= 0.5) {
                reached_0_5 = true;
            }
            if msg.progress == Some(1.0) {
                reached_1_0 = true;
                break;
            }
        }
    }

    assert!(reached_0_5, "Message never reached 50% progress");
    assert!(reached_1_0, "Message never reached 100% progress");
    
    jsi.stop().unwrap();
}
