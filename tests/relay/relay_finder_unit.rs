use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use rust_comms::api::bingle_api::{NetworkEndpoint, ProgressCallback, UserId};

// Minimal mock API that returns a positive CheckResponse for send_message_to_network_with_response
struct MockApi;
impl InnerBingleApi for MockApi { 
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
}

use crate::util::reusable_mock_api::{to_weak_api_both, InnerBingleApi, MockApiBoth};
use rust_comms::relay::relay_finder::{RelayFinder, RelayInfo};

#[path = "../test_util.rs"]
mod test_util;

#[test]
fn find_root_relay_rejects_self() {
    let discover = Arc::new(|| -> Vec<RelayInfo> {
        vec![
            RelayInfo { id: test_util::ADDRESS_SPEND.to_string(), address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345), state: None },
            RelayInfo { id: test_util::ADDRESS_RECEIVE.to_string(), address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12346), state: None },
        ]
    });
    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(MockApi);
    let finder = RelayFinder::new(to_weak_api_both(MockApiBoth::new_with_api_override(api)), std::time::Duration::from_secs(30), discover);
    // my_id is ADDRESS_SPEND, ensure we do not select ourselves and get ADDRESS_RECEIVE instead
    let res = finder.find_root_relay(test_util::ADDRESS_SPEND);
    assert!(res.is_ok(), "should find other relay");
    let info = res.unwrap();
    assert_eq!(info.id, test_util::ADDRESS_RECEIVE);
    assert_eq!(info.address, SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12346));
}

#[test]
fn find_root_relay_only_self_errors() {
    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(MockApi);
    let discover = Arc::new(|| -> Vec<RelayInfo> {
        vec![
            RelayInfo { id: test_util::ADDRESS_SPEND.to_string(), address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12000), state: None },
        ]
    });
    let finder = RelayFinder::new(to_weak_api_both(MockApiBoth::new_with_api_override(api)), std::time::Duration::from_secs(30), discover);
    let res = finder.find_root_relay(test_util::ADDRESS_SPEND);
    assert!(res.is_err(), "should error when only self is present");
}
