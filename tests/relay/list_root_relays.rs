use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::time::Duration;

use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::relay::relay_finder::{RelayFinder, RootRelayInfo};

#[derive(Clone)]
struct MockApi;
impl BingleApi for MockApi {
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> { None }
    fn get_handle(&self) -> Option<String> { None }
    fn get_app_id(&self) -> Option<u64> { None }
    fn get_algo_provider_config(&self) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> { None }
    fn start(&mut self, _options: &StartOptions) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}
    fn send_message_to_id(&self, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &rust_comms::api::bingle_api::Handle, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _nsk: &rust_comms::api::bingle_api::NetworkEndpoint, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &rust_comms::api::bingle_api::Handle, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_network_with_response(&self, _nsk: &rust_comms::api::bingle_api::NetworkEndpoint, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn set_on_message(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnConnectHandler>>) {}
}

#[test]
fn list_root_relays_excludes_self_and_caches() {
    let api: Arc<dyn BingleApi> = Arc::new(MockApi);
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_clone = calls.clone();
    let discover = Arc::new(move || {
        calls_clone.fetch_add(1, Ordering::SeqCst);
        vec![
            RootRelayInfo { id: "AAA".into(), address: "127.0.0.1:5001".parse().unwrap(), state: None },
            RootRelayInfo { id: "BBB".into(), address: "127.0.0.1:5002".parse().unwrap(), state: None },
        ]
    });

    let finder = RelayFinder::new(api, Duration::from_millis(2000), discover);
    let list1 = finder.list_root_relays("AAA");
    assert_eq!(list1.len(), 1, "should exclude self");
    assert_eq!(list1[0].id, "BBB");

    // Second call should use cache (no extra discover invocations)
    let list2 = finder.list_root_relays("AAA");
    assert_eq!(list2.len(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "discover should be called only once due to cache");
}
