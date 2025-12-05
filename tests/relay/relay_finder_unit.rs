use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkSourceKey, ProgressCallback, StartOptions, UserId};

// Minimal mock API that returns a positive CheckResponse for send_message_to_network_with_response
struct MockApi;
impl BingleApi for MockApi {
    fn get_my_id(&self) -> Option<String> { None }
    fn get_app_id(&self) -> Option<u64> { None }
    fn start(&mut self, _options: StartOptions) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}

    fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _nsk: &NetworkSourceKey, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { true }

    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("not used".to_string()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("not used".to_string()) }
    fn send_message_to_network_with_response(&self, _nsk: &NetworkSourceKey, _user_id: &UserId, message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> {
        // Validate that Option gets Some where required in helper calls
        let is_check = message.get("type").and_then(|v| v.as_str()) == Some("Check") && message.get("app").map(|v| v.is_null()).unwrap_or(false);
        assert!(is_check, "RelayFinder should perform a Check");
        let mut obj = serde_json::Map::new();
        obj.insert("app".to_string(), serde_json::Value::Null);
        obj.insert("type".to_string(), serde_json::Value::String("CheckResponse".into()));
        obj.insert("available".to_string(), serde_json::Value::Bool(true));
        Ok(serde_json::Value::Object(obj))
    }

    fn set_on_message(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnConnectHandler>>) {}
}

use rust_comms::relay::relay_finder::{RelayFinder, RootRelayInfo};

#[path = "../test_util.rs"]
mod test_util;

#[test]
fn find_root_relay_rejects_self() {
    let api: Arc<dyn BingleApi> = Arc::new(MockApi);
    let discover = Arc::new(|| -> Vec<RootRelayInfo> {
        vec![
            RootRelayInfo { id: test_util::ADDRESS_SPEND.to_string(), address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345) },
            RootRelayInfo { id: test_util::ADDRESS_RECEIVE.to_string(), address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12346) },
        ]
    });
    let finder = RelayFinder::new(api, std::time::Duration::from_secs(30), discover);
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
    let discover = Arc::new(|| -> Vec<RootRelayInfo> {
        vec![
            RootRelayInfo { id: test_util::ADDRESS_SPEND.to_string(), address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12000) },
        ]
    });
    let finder = RelayFinder::new(api, std::time::Duration::from_secs(30), discover);
    let res = finder.find_root_relay(test_util::ADDRESS_SPEND);
    assert!(res.is_err(), "should error when only self is present");
}
