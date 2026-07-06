use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth, to_weak_api_both};
use rust_comms::api::bingle_api::{NetworkEndpoint, ProgressCallback, UserId};
use rust_comms::relay::relay_finder::{RelayFinder, RelayFinderTrait, RelayInfo};
use std::sync::Arc;

#[path = "../test_util.rs"]
pub mod test_util;

struct GetRelaysMockApi {
    pub call_count: Arc<std::sync::atomic::AtomicUsize>,
}

impl InnerBingleApi for GetRelaysMockApi {
    fn send_message_to_network_with_response(
        &self,
        _nsk: &NetworkEndpoint,
        _user_id: &UserId,
        message: serde_json::Value,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> {
        let ty = message
            .get("type")
            .and_then(|v: &serde_json::Value| v.as_str())
            .unwrap_or("");
        let app = message
            .get("app")
            .and_then(|v: &serde_json::Value| v.as_str())
            .unwrap_or("");

        if ty == "getRelaysStatus" && app == "ddb" {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            // Return two relays, different from the root.
            Ok(serde_json::json!({
                "app": "ddb",
                "type": "relaysStatusResponse",
                "epochId": 1,
                "treeOrder": 0,
                "responderState": "available",
                "relayIds": ["R-SUB-1", "R-SUB-2"],
                "relayEndpoints": [
                    {"host": "127.0.0.1", "port": 20001},
                    {"host": "127.0.0.1", "port": 20002}
                ],
                "relayStates": ["available", "available"]
            }))
        } else {
            Err(rust_comms::api::bingle_api::BingleError::Other(
                "unexpected message".into(),
            ))
        }
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn list_all_relays_queries_root_even_if_only_one() {
    let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let api_inner = Arc::new(GetRelaysMockApi {
        call_count: call_count.clone(),
    });
    let api = to_weak_api_both(MockApiBoth::new_with_api_override(api_inner));

    // Valid 58-char base32 ID
    let root_id = "IAOSUGCPN6WTPI3LCXLHXMJU3UT3VIGP3CKZ6H3P6XYZND4JYKZJSFYZ3I";

    let discover = Arc::new(move || -> Vec<RelayInfo> {
        vec![test_util::signed_root_relay(
            root_id,
            "127.0.0.1:10000".parse().unwrap(),
        )]
    });

    let finder = RelayFinder::new(api, discover);

    // include_self = false, my_id = some other ID
    let relays = finder.list_all_relays("ME-ID", false);

    // If the bug is present, call_count will be 0 and relays will contain only the root.
    // If fixed, call_count will be 1 and relays will contain R-SUB-1 and R-SUB-2.

    assert_eq!(
        call_count.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "Should have queried the single root relay"
    );
    assert_eq!(relays.len(), 2);
    assert!(relays.iter().any(|r| r.id() == "R-SUB-1"));
    assert!(relays.iter().any(|r| r.id() == "R-SUB-2"));
}
