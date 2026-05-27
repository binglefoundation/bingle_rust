use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use rust_comms::api::bingle_api::{NetworkEndpoint, ProgressCallback, UserId};

// Minimal mock API that returns a positive CheckResponse for send_message_to_network_with_response
struct MockApi;
impl InnerBingleApi for MockApi { 
    fn send_message_to_network_with_response(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> {
        let ty = message.get("type").and_then(|v: &serde_json::Value| v.as_str()).unwrap_or("");
        let app = message.get("app");
        if ty == "Check" && app.map(|v| v.is_null()).unwrap_or(false) {
            // Respond to RelayCheck
            let mut obj = serde_json::Map::new();
            obj.insert("app".to_string(), serde_json::Value::Null);
            obj.insert("type".to_string(), serde_json::Value::String("CheckResponse".into()));
            obj.insert("state".to_string(), serde_json::Value::String("available".into()));
            Ok(serde_json::Value::Object(obj))
        } else if ty == "getRelaysStatus" && app.and_then(|v: &serde_json::Value| v.as_str()) == Some("ddb") {
            // Respond to DdbGetRelaysStatus with minimal DdbRelaysStatusResponse; keep relayIds empty so client falls back to discovery
            Ok(serde_json::json!({
                "app": "ddb",
                "type": "relaysStatusResponse",
                "epochId": -1,
                "treeOrder": 2,
                "relayIds": []
            }))
        } else {
            Err(rust_comms::api::bingle_api::BingleError::Other("unexpected message".into()))
        }
    }
}

use crate::util::reusable_mock_api::{to_weak_api_both, InnerBingleApi, MockApiBoth};
use rust_comms::relay::relay_finder::{RelayFinder, RelayInfo, RelayFinderTestTrait};

#[path = "../test_util.rs"]
pub mod test_util;

#[cfg_attr(not(target_os = "ios"), test)]
pub fn find_root_relay_rejects_self() {
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

#[cfg_attr(not(target_os = "ios"), test)]
pub fn select_indices_partitions_for_multiple_ids() {
    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(MockApi);
    let discover = Arc::new(|| -> Vec<RelayInfo> { Vec::new() });
    let finder = RelayFinder::new(
        to_weak_api_both(MockApiBoth::new_with_api_override(api)),
        std::time::Duration::from_secs(30),
        discover,
    );

    let relays = vec![
        RelayInfo { id: "R1".into(), address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 10001), state: None },
        RelayInfo { id: "R2".into(), address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 10002), state: None },
        RelayInfo { id: "R3".into(), address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 10003), state: None },
        RelayInfo { id: "R4".into(), address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 10004), state: None },
    ];

    let ids = [
        "AALHSDXNRPARCE7OYMOIQDEEGKZA3QMVN3J2ONBTVHS66EBNACAQ4EKXRM",
        "IAOSUGCPN6WTPI3LCXLHXMJU3UT3VIGP3CKZ6H3P6XYZND4JYKZJSFYZ3I",
        "QASXBML72DKIJEJ5GLMEBBX33KCKW3TSJW7ETFOTLEREQCDMW5BXCLXSQU",
        "YA2UAJPUJZBY4KR2B4FBM57NSA7252PJQTVKJEGB2MOISRUECW4JGE4USM",
        ];

    for (i, id) in ids.iter().enumerate() {
        let (idx, alt) = finder.select_indices(&relays, id);
        tracing::info!("[RelayFinder] select_indices: id={} idx={} alt={}", id, idx, alt);
        assert_eq!(idx, i, "idx mismatch for id {}", id);
        assert_eq!(alt, (idx + 1) % 4, "alt mismatch for id {}", id);
    }
}
