use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration};

use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, OnConnectHandler, OnMessageHandler, ProgressCallback, StartOptions, UserId};
use rust_comms::relay::relay_finder::{RelayFinder, RootRelayInfo};
use serde_json::Value as JsonValue;

#[derive(Clone)]
struct MockApi {
    // Map addr -> available
    availability: Arc<Mutex<Vec<(SocketAddr, bool)>>>,
    calls: Arc<Mutex<usize>>,
}

impl MockApi {
    fn new() -> (Self, Arc<Mutex<usize>>) {
        let calls = Arc::new(Mutex::new(0));
        let api = Self { availability: Arc::new(Mutex::new(vec![])), calls: calls.clone() };
        (api, calls)
    }
    fn with_availability(self, pairs: Vec<(SocketAddr, bool)>) -> Self {
        if let Ok(mut v) = self.availability.lock() { *v = pairs; }
        self
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

    fn handle_lookup(&self, _handle: &Handle) -> Result<Option<UserId>, String> { Ok(None) }

    fn send_message_to_id(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _network_source_key: &NetworkEndpoint, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { Err("ni".to_string()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { Err("ni".to_string()) }

    fn send_message_to_network_with_response(&self, network_source_key: &NetworkEndpoint, _user_id: &UserId, message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> {
        // Count calls
        if let Ok(mut c) = self.calls.lock() { *c += 1; }
        let addr = network_source_key.inet_socket_address().expect("addr required");
        // Should receive a RelayCheck
        let is_check = message.get("type").and_then(|v: &serde_json::Value| v.as_str()) == Some("Check") && message.get("app").map(|v| v.is_null()).unwrap_or(true);
        if !is_check { return Err("unexpected message".to_string()); }
        // Lookup availability
        let available = self.availability.lock().ok()
            .and_then(|v| v.iter().find(|(a, _)| *a == addr).cloned())
            .map(|(_, ok)| ok)
            .unwrap_or(false);
        let resp = serde_json::json!({ "app": null, "type": "CheckResponse", "state": if available { "available" } else { "off" } });
        Ok(resp)
    }

    fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
}

impl rust_comms::api::bingle_api::BingleApiInternal for MockApi {
    fn set_state(&self, _state: rust_comms::engine::EngineState) {}
    fn get_state(&self) -> rust_comms::engine::EngineState { rust_comms::engine::EngineState::StunIdentify }
    fn set_nat_type(&self, _nat: rust_comms::engine::NatType) {}
    fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> { None }
    fn ddb_register_ip(&self, _endpoint: std::net::SocketAddr, _am_relay: bool) -> Result<(), String> { Ok(()) }
    fn ddb_register_relay(&self, _relay_id: String, _relay_sig: Option<String>) -> Result<(), String> { Ok(()) }
    fn update_turn_listener_relay(&self, _relay_id: String, _relay_addr: std::net::SocketAddr) -> Result<(), String> { Ok(()) }
    fn turn_client_handle_listen_response(&self, _relay_addr: std::net::SocketAddr, _relay_id: String) {}
    fn turn_lookup_addr_by_id(&self, _id: String) -> Option<std::net::SocketAddr> { None }
    fn turn_handle_call(&self, _source: std::net::SocketAddr, _dest: std::net::SocketAddr) -> i32 { -1 }
    fn turn_handle_listen(&self, _id: String, _source: std::net::SocketAddr) -> bool { false }
    fn turn_handle_called(&self, _source: std::net::SocketAddr, _dest: std::net::SocketAddr, _channel: u16) {}
    fn notify_listening(&self, _listening: bool) {}
    fn get_relay_state(&self) -> String { "off".into() }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn relay_finder_caches_successful_root_relay() {
    // Prepare two candidate relays
    let a1: SocketAddr = "127.0.0.1:40001".parse().unwrap();
    let a2: SocketAddr = "127.0.0.1:40002".parse().unwrap();

    let (api_raw, calls_counter) = MockApi::new();
    let api = api_raw.with_availability(vec![(a1, false), (a2, true)]);
    let api_arc: Arc<dyn rust_comms::api::bingle_api::BingleApiBoth> = Arc::new(api);
    let api_weak = Arc::downgrade(&api_arc);

    // Discovery closure
    let discover = Arc::new(move || vec![
        RootRelayInfo { id: "ADDR1".to_string(), address: a1, state: None },
        RootRelayInfo { id: "ADDR2".to_string(), address: a2, state: None },
    ]);

    let finder = RelayFinder::new(api_weak, Duration::from_millis(200), discover);

    // Any id will do; finder will test preferred then alternate and choose an available one.
    let picked = finder.find_root_relay("SOME-ID").expect("should pick an available relay");
    assert!(picked.address == a1 || picked.address == a2);

    // Second call within TTL should return cached without new RelayCheck
    let calls_before = *calls_counter.lock().unwrap();
    let _ = finder.find_root_relay("SOME-ID").expect("cached");
    let calls_after = *calls_counter.lock().unwrap();
    assert_eq!(calls_before, calls_after, "should not perform a second RelayCheck within TTL");

    // After TTL expires, a new RelayCheck should be attempted
    std::thread::sleep(Duration::from_millis(220));
    let _ = finder.find_root_relay("SOME-ID").expect("rechecked and cached again");
    let calls_final = *calls_counter.lock().unwrap();
    assert!(calls_final > calls_after, "should perform RelayCheck again after TTL");
}
