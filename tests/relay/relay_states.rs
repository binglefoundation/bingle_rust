use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use rust_comms::api::bingle_api::{NetworkEndpoint, ProgressCallback, UserId};
use rust_comms::relay::relay_finder::{RelayFinder, RelayInfo, RelayFinderTrait, RelayFinderTestTrait};

#[path = "../test_util.rs"]
pub mod test_util;
use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth};
use rust_comms::engine::RelayState;

#[derive(Clone)]
struct MockApi {
    // vector of (id, addr, state) for deterministic getRelaysStatus ordering
    entries: Arc<Vec<(String, SocketAddr, String)>>,
    // map of address -> state string used to respond to RelayCheck fast
    states: Arc<std::collections::HashMap<SocketAddr, String>>,
}

impl MockApi {
    fn new(entries: Vec<(String, SocketAddr, &str)>) -> Self {
        let mut map = std::collections::HashMap::new();
        let mut v: Vec<(String, SocketAddr, String)> = Vec::new();
        for (id, a, s) in entries {
            map.insert(a, s.to_string());
            v.push((id, a, s.to_string()));
        }
        Self { entries: Arc::new(v), states: Arc::new(map) }
    }
}

impl InnerBingleApi for MockApi { 
    fn send_message_to_network_with_response(&self, nsk: &NetworkEndpoint, _user_id: &UserId, message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> {
        let ty = message.get("type").and_then(|v: &serde_json::Value| v.as_str()).unwrap_or("");
        match (message.get("app"), ty) {
            (Some(app), "getRelaysStatus") if app.as_str() == Some("ddb") => {
                // Return a DDB RelaysStatusResponse with relayIds and aligned relayEndpoints based on entries
                let mut ids: Vec<serde_json::Value> = Vec::new();
                let mut eps: Vec<serde_json::Value> = Vec::new();
                for (id, addr, _st) in self.entries.iter() {
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
                // RelayCheck: reply with CheckResponse using configured state
                let addr = nsk.inet_socket_address().expect("direct endpoint required");
                let st = self.states.get(&addr).cloned().unwrap_or_else(|| "off".to_string());
                Ok(serde_json::json!({ "app": null, "type": "CheckResponse", "state": st }))
            }
            _ => Err(rust_comms::api::bingle_api::BingleError::Other("unexpected message".into()))
        }
    }
}

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

#[cfg_attr(not(target_os = "ios"), test)]
pub fn load_and_summarize_states() {
    let a1 = addr(41001);
    let a2 = addr(41002);
    let a3 = addr(41003);

    // Define states for three relays using valid Algorand addresses from test_util
    let id1 = test_util::ADDRESS_SPEND.to_string();
    let id2 = test_util::ADDRESS_RECEIVE.to_string();
    let id3 = test_util::ADDRESS_10MIL.to_string();
    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(MockApi::new(vec![
        (id1.clone(), a1, "available"),
        (id2.clone(), a2, "starting"),
        (id3.clone(), a3, "off"),
    ]));

    // Root discovery closure returns the same three as roots
    let discover = {
        let ids = vec![
            RelayInfo { id: id1.clone(), address: a1, state: None },
            RelayInfo { id: id2.clone(), address: a2, state: None },
            RelayInfo { id: id3.clone(), address: a3, state: None },
        ];
        Arc::new(move || ids.clone())
    };

    let finder = RelayFinder::new(crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new_with_api_override(api)), Duration::from_millis(500), discover);

    // Load states (will internally call list_all_relays which uses DDB getRelaysStatus from preferred root)
    finder.load_relay_states("MYID");

    // Summarize and assert counts
    let map = finder.relays_available();
    assert_eq!(map.get(&RelayState::Available).cloned().unwrap_or(0), 1);
    assert_eq!(map.get(&RelayState::Starting).cloned().unwrap_or(0), 1);
    assert_eq!(map.get(&RelayState::Off).cloned().unwrap_or(0), 1);
}
