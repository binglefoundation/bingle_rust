use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkSourceKey, StartOptions, UserId};
use rust_comms::relay::relay_finder::{RelayFinder, RootRelayInfo};

struct MockApi;
impl BingleApi for MockApi {
    fn get_my_id(&self) -> Option<String> { None }
    fn get_app_id(&self) -> Option<u64> { None }
    fn start(&mut self, _options: StartOptions) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}
    fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> bool { true }
    fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> bool { true }
    fn send_message_to_network(&self, _network_source_key: &NetworkSourceKey, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> bool { true }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("not needed".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("not needed".into()) }
    fn send_message_to_network_with_response(&self, _network_source_key: &NetworkSourceKey, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"app": null, "type": "CheckResponse", "available": true}))
    }
    fn set_on_message(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnConnectHandler>>) {}
}

#[test]
fn find_relay_delegates_to_find_root_relay() {
    let api: Arc<dyn BingleApi> = Arc::new(MockApi);
    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 40001);
    let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 40002);

    // Two deterministic relays so selection is well-defined; we don't assert which one, only consistency
    let discover = Arc::new(move || -> Vec<RootRelayInfo> {
        vec![
            RootRelayInfo { id: "AAAA".to_string(), address: addr1 },
            RootRelayInfo { id: "BBBB".to_string(), address: addr2 },
        ]
    });

    let finder = RelayFinder::new(api, std::time::Duration::from_secs(5), discover);
    let my_id = "SOME_ALGO_ADDR"; // value doesn't matter for delegation equivalence

    let root = finder.find_root_relay(my_id).expect("find_root_relay failed");
    let relay_addr = finder.find_relay(my_id).expect("find_relay failed");

    assert_eq!(root.address, relay_addr);
}
