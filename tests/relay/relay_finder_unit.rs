use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, ProgressCallback, StartOptions, UserId};

// Minimal mock API that returns a positive CheckResponse for send_message_to_network_with_response
struct MockApi;
impl BingleApi for MockApi { 
    fn set_on_listening(&mut self, _handler: Option<std::sync::Arc<rust_comms::api::bingle_api::OnListeningHandler>>) {} 
    fn get_algo_provider_config(&self) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> { None } 
    fn get_handle(&self) -> Option<String> { None } 
    fn get_user_id(&self) -> Option<String> { None } 
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> { None }
    fn get_app_id(&self) -> Option<u64> { None }
    fn start(&mut self, _options: &StartOptions) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}

    fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { true }

    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("not used".to_string()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("not used".to_string()) }
    fn send_message_to_network_with_response(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> {
        let ty = message.get("type").and_then(|v: &serde_json::Value| v.as_str()).unwrap_or("");
        let app = message.get("app");
        if ty == "Check" && app.map(|v| v.is_null()).unwrap_or(false) {
            // Respond to RelayCheck
            let mut obj = serde_json::Map::new();
            obj.insert("app".to_string(), serde_json::Value::Null);
            obj.insert("type".to_string(), serde_json::Value::String("CheckResponse".into()));
            obj.insert("state".to_string(), serde_json::Value::String("available".into()));
            Ok(serde_json::Value::Object(obj))
        } else if ty == "getEpoch" && app.and_then(|v: &serde_json::Value| v.as_str()) == Some("ddb") {
            // Respond to DdbGetEpoch with minimal EpochInfo; keep relayIds empty so client falls back to discovery
            Ok(serde_json::json!({
                "app": "ddb",
                "type": "getEpochResponse",
                "epochId": -1,
                "treeOrder": 2,
                "relayIds": []
            }))
        } else {
            Err("unexpected message".into())
        }
    }

    fn set_on_message(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnConnectHandler>>) {}
}

use rust_comms::relay::relay_finder::{RelayFinder, RelayInfo};

#[path = "../test_util.rs"]
mod test_util;

#[test]
fn find_root_relay_rejects_self() {
    let api: Arc<dyn BingleApi> = Arc::new(MockApi);
    let discover = Arc::new(|| -> Vec<RelayInfo> {
        vec![
            RelayInfo { id: test_util::ADDRESS_SPEND.to_string(), address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345), state: None },
            RelayInfo { id: test_util::ADDRESS_RECEIVE.to_string(), address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12346), state: None },
        ]
    });
    let finder = RelayFinder::new(crate::util::mock_bingle_api::arc_to_weak(api), std::time::Duration::from_secs(30), discover);
    // my_id is ADDRESS_SPEND, ensure we do not select ourselves and get ADDRESS_RECEIVE instead
    let res = finder.find_root_relay(test_util::ADDRESS_SPEND);
    assert!(res.is_ok(), "should find other relay");
    let info = res.unwrap();
    assert_eq!(info.id, test_util::ADDRESS_RECEIVE);
    assert_eq!(info.address, SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12346));
}

#[test]
fn find_root_relay_only_self_errors() {
    let api: Arc<dyn BingleApi> = Arc::new(MockApi);
    let discover = Arc::new(|| -> Vec<RelayInfo> {
        vec![
            RelayInfo { id: test_util::ADDRESS_SPEND.to_string(), address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12000), state: None },
        ]
    });
    let finder = RelayFinder::new(crate::util::mock_bingle_api::arc_to_weak(api), std::time::Duration::from_secs(30), discover);
    let res = finder.find_root_relay(test_util::ADDRESS_SPEND);
    assert!(res.is_err(), "should error when only self is present");
}
