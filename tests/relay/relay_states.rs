use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, OnConnectHandler, OnMessageHandler, ProgressCallback, StartOptions, UserId};
use rust_comms::relay::relay_finder::{RelayFinder, RelayInfo};

#[path = "../test_util.rs"]
mod test_util;
use rust_comms::engine::RelayState;

#[derive(Clone)]
struct MockApi {
    // vector of (id, addr, state) for deterministic getEpoch ordering
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
    fn send_message_to_network(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }

    fn send_message_to_network_with_response(&self, nsk: &NetworkEndpoint, _user_id: &UserId, message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> {
        let ty = message.get("type").and_then(|v: &serde_json::Value| v.as_str()).unwrap_or("");
        match (message.get("app"), ty) {
            (Some(app), "getEpoch") if app.as_str() == Some("ddb") => {
                // Return a DDB getEpochResponse with relayIds and aligned relayEndpoints based on entries
                let mut ids: Vec<serde_json::Value> = Vec::new();
                let mut eps: Vec<serde_json::Value> = Vec::new();
                for (id, addr, _st) in self.entries.iter() {
                    ids.push(serde_json::Value::String(id.clone()));
                    eps.push(serde_json::json!({"host": addr.ip().to_string(), "port": addr.port()}));
                }
                Ok(serde_json::json!({
                    "app": "ddb",
                    "type": "getEpochResponse",
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
            _ => Err("unexpected message".into())
        }
    }

    fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
}

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

#[test]
fn load_and_summarize_states() {
    let a1 = addr(41001);
    let a2 = addr(41002);
    let a3 = addr(41003);

    // Define states for three relays using valid Algorand addresses from test_util
    let id1 = test_util::ADDRESS_SPEND.to_string();
    let id2 = test_util::ADDRESS_RECEIVE.to_string();
    let id3 = test_util::ADDRESS_10MIL.to_string();
    let api: Arc<dyn BingleApi> = Arc::new(MockApi::new(vec![
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

    let finder = RelayFinder::new(crate::util::mock_bingle_api::arc_to_weak(api), Duration::from_millis(500), discover);

    // Load states (will internally call list_all_relays which uses DDB getEpoch from preferred root)
    finder.load_relay_states("MYID");

    // Summarize and assert counts
    let map = finder.relays_available();
    assert_eq!(map.get(&RelayState::Available).cloned().unwrap_or(0), 1);
    assert_eq!(map.get(&RelayState::Starting).cloned().unwrap_or(0), 1);
    assert_eq!(map.get(&RelayState::Off).cloned().unwrap_or(0), 1);
}
