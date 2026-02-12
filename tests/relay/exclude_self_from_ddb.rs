use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, OnConnectHandler, OnMessageHandler, ProgressCallback, StartOptions, UserId};
use rust_comms::relay::relay_finder::{RelayFinder, RelayInfo};

#[path = "../test_util.rs"]
mod test_util;

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
                // Return aligned relayIds and relayEndpoints from entries
                let mut ids: Vec<serde_json::Value> = Vec::new();
                let mut eps: Vec<serde_json::Value> = Vec::new();
                for (id, addr) in self.entries.iter() {
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
                // Respond available for RelayCheck
                let _addr = nsk.inet_socket_address().expect("direct endpoint required");
                Ok(serde_json::json!({ "app": null, "type": "CheckResponse", "state": "available" }))
            }
            _ => Err("unexpected message".into())
        }
    }

    fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
}

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

#[test]
fn list_all_relays_excludes_self_from_ddb() {
    // my id appears in DDB list; ensure list_all_relays excludes it
    let my_id = test_util::ADDRESS_SPEND.to_string();
    let other = test_util::ADDRESS_RECEIVE.to_string();
    let a1 = addr(42001);
    let a2 = addr(42002);

    let api: Arc<dyn BingleApi> = Arc::new(MockApi::new(vec![
        (my_id.clone(), a1),
        (other.clone(), a2),
    ]));

    // Root discovery can return any roots; ensure deterministic order
    let discover = {
        let roots = vec![
            RelayInfo { id: other.clone(), address: a2, state: None },
            RelayInfo { id: my_id.clone(), address: a1, state: None },
        ];
        Arc::new(move || roots.clone())
    };

    let finder = RelayFinder::new(crate::util::mock_bingle_api::arc_to_weak(api), Duration::from_millis(200), discover);

    // When include_self = false, our own id must not be present even if DDB provided it
    let list = finder.list_all_relays(&my_id, false);
    let ids: Vec<String> = list.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&other), "should contain other relay from DDB");
    assert!(!ids.contains(&my_id), "should NOT contain our own relay id in choices");
}

#[test]
fn find_relay_does_not_select_self_even_if_ddb_includes_self() {
    let my_id = test_util::ADDRESS_SPEND.to_string();
    let other = test_util::ADDRESS_RECEIVE.to_string();
    let a1 = addr(43001);
    let a2 = addr(43002);

    let api: Arc<dyn BingleApi> = Arc::new(MockApi::new(vec![
        (my_id.clone(), a1),
        (other.clone(), a2),
    ]));

    let discover = {
        let roots = vec![
            RelayInfo { id: other.clone(), address: a2, state: None },
            RelayInfo { id: my_id.clone(), address: a1, state: None },
        ];
        Arc::new(move || roots.clone())
    };

    let finder = RelayFinder::new(crate::util::mock_bingle_api::arc_to_weak(api), Duration::from_millis(200), discover);

    let picked = finder.find_relay(&my_id).expect("should pick a relay");
    assert_ne!(picked.id, my_id, "must not select self");
}