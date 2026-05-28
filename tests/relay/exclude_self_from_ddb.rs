use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use crate::util::reusable_mock_api::{to_weak_api_both, InnerBingleApi, MockApiBoth};
use rust_comms::api::bingle_api::{NetworkEndpoint, ProgressCallback, UserId};
use rust_comms::relay::relay_finder::{RelayFinder, RelayInfo, RelayFinderTrait};

#[path = "../test_util.rs"]
pub mod test_util;

#[derive(Clone)]
struct MockApi {
    // vector of (id, addr)
    entries: Arc<Vec<(String, SocketAddr)>>,
}

impl MockApi {
    fn new(entries: Vec<(String, SocketAddr)>) -> Self {
        Self { entries: Arc::new(entries) }
    }
}

impl InnerBingleApi for MockApi { 
    fn send_message_to_network_with_response(&self, nsk: &NetworkEndpoint, _user_id: &UserId, message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> {
        let ty = message.get("type").and_then(|v: &serde_json::Value| v.as_str()).unwrap_or("");
        match (message.get("app"), ty) {
            (Some(app), "getRelaysStatus") if app.as_str() == Some("ddb") => {
                // Return aligned relayIds and relayEndpoints from entries
                let mut ids: Vec<serde_json::Value> = Vec::new();
                let mut eps: Vec<serde_json::Value> = Vec::new();
                for (id, addr) in self.entries.iter() {
                    ids.push(serde_json::Value::String(id.clone()));
                    eps.push(serde_json::json!({"host": addr.ip().to_string(), "port": addr.port()}));
                }
                Ok(serde_json::json!({
                    "app": "ddb",
                    "type": "relaysStatusResponse",
                    "epochId": -1,
                    "treeOrder": 2,
                    "relayIds": ids,
                    "relayEndpoints": eps,
                }))
            }
            (app, "Check") if app.is_none() || app.unwrap().is_null() => {
                // Respond available for RelayCheck
                let _addr = nsk.inet_socket_address().expect("direct endpoint required");
                Ok(serde_json::json!({ "app": null, "type": "CheckResponse", "state": "available" }))
            }
                _ => Err(rust_comms::api::bingle_api::BingleError::Other("unexpected message".into()))
        }
    }
}

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

#[cfg_attr(not(target_os = "ios"), test)]
pub fn list_all_relays_excludes_self_from_ddb() {
    // my id appears in DDB list; ensure list_all_relays excludes it
    let my_id = test_util::ADDRESS_SPEND.to_string();
    let other = test_util::ADDRESS_RECEIVE.to_string();
    let a1 = addr(42001);
    let a2 = addr(42002);

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(MockApi::new(vec![
        (my_id.clone(), a1),
        (other.clone(), a2),
    ]));

    // Root discovery can return any roots; ensure deterministic order
    let discover = {
        let roots = vec![
            RelayInfo::root(other.clone(), a2),
            RelayInfo::root(my_id.clone(), a1),
        ];
        Arc::new(move || roots.clone())
    };

    let finder = RelayFinder::new(to_weak_api_both(MockApiBoth::new_with_api_override(api)), Duration::from_millis(200), discover);

    // When include_self = false, our own id must not be present even if DDB provided it
    let list = finder.list_all_relays(&my_id, false);
    let ids: Vec<String> = list.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&other), "should contain other relay from DDB");
    assert!(!ids.contains(&my_id), "should NOT contain our own relay id in choices");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn find_relay_does_not_select_self_even_if_ddb_includes_self() {
    let my_id = test_util::ADDRESS_SPEND.to_string();
    let other = test_util::ADDRESS_RECEIVE.to_string();
    let a1 = addr(43001);
    let a2 = addr(43002);

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(MockApi::new(vec![
        (my_id.clone(), a1),
        (other.clone(), a2),
    ]));

    let discover = {
        let roots = vec![
            RelayInfo::root(other.clone(), a2),
            RelayInfo::root(my_id.clone(), a1),
        ];
        Arc::new(move || roots.clone())
    };

    let finder = RelayFinder::new(to_weak_api_both(MockApiBoth::new_with_api_override(api)), Duration::from_millis(200), discover);

    let picked = finder.find_relay(&my_id).expect("should pick a relay");
    assert_ne!(picked.id, my_id, "must not select self");
}