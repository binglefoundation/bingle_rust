use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use crate::util::reusable_mock_api::{to_weak_api_both, InnerBingleApi, MockApiBoth};
use rust_comms::api::bingle_api::{NetworkEndpoint, ProgressCallback, UserId};
use rust_comms::engine::RelayState;
use rust_comms::relay::relay_finder::{RelayFinder, RelayInfo, RelayFinderTrait, RelayFinderTestTrait};

#[path = "../test_util.rs"]
pub mod test_util;

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
                let states: Vec<serde_json::Value> = self.entries.iter()
                    .map(|(_, _, st)| serde_json::Value::String(st.clone()))
                    .collect();
                Ok(serde_json::json!({
                    "app": "ddb",
                    "type": "relaysStatusResponse",
                    "epochId": -1,
                    "treeOrder": 2,
                    "responderState": "available",
                    "relayIds": ids,
                    "relayEndpoints": eps,
                    "relayStates": states,
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

#[test]
#[cfg(not(target_os = "ios"))]
pub fn clear_state_cache_resets_and_reloads() {
    let a1 = addr(41201);
    let a2 = addr(41202);
    let a3 = addr(41203);

    // Valid Algorand addresses from test_util to satisfy base32 length checks
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
            RelayInfo::root(id1.clone(), a1),
            RelayInfo::root(id2.clone(), a2),
            RelayInfo::root(id3.clone(), a3),
        ];
        Arc::new(move || ids.clone())
    };

    let finder = RelayFinder::new(to_weak_api_both(MockApiBoth::new_with_api_override(api)), discover);

    // Initial load populates states
    let _ = finder.find_relay("MYID");
    let m1 = finder.relays_available();
    assert_eq!(m1.get(&RelayState::Available).cloned().unwrap_or(0), 1);
    assert_eq!(m1.get(&RelayState::Starting).cloned().unwrap_or(0), 1);
    assert_eq!(m1.get(&RelayState::Off).cloned().unwrap_or(0), 1);

    // Clear cache: states should be None and summary empty
    finder.clear_state_cache();
    let m2 = finder.relays_available();
    assert_eq!(m2.get(&RelayState::Available).cloned().unwrap_or(0), 0);
    assert_eq!(m2.get(&RelayState::Starting).cloned().unwrap_or(0), 0);
    assert_eq!(m2.get(&RelayState::Off).cloned().unwrap_or(0), 0);
    assert_eq!(m2.len(), 0);

    // Reload: states should be repopulated from API
    let _ = finder.find_relay("MYID");
    let m3 = finder.relays_available();
    assert_eq!(m3.get(&RelayState::Available).cloned().unwrap_or(0), 1);
    assert_eq!(m3.get(&RelayState::Starting).cloned().unwrap_or(0), 1);
    assert_eq!(m3.get(&RelayState::Off).cloned().unwrap_or(0), 1);
}
