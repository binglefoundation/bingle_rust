use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkSourceKey, OnConnectHandler, OnMessageHandler, ProgressCallback, StartOptions, UserId};
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
        (Self { availability: Arc::new(Mutex::new(vec![])), calls: Arc::new(Mutex::new(0)) }, Arc::new(Mutex::new(0)))
    }
    fn with_availability(mut self, pairs: Vec<(SocketAddr, bool)>) -> Self {
        if let Ok(mut v) = self.availability.lock() { *v = pairs; }
        self
    }
}

impl BingleApi for MockApi {
    fn start(&mut self, _options: StartOptions) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}

    fn send_message_to_id(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _network_source_key: &NetworkSourceKey, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { Err("ni".to_string()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> { Err("ni".to_string()) }

    fn send_message_to_network_with_response(&self, network_source_key: &NetworkSourceKey, _user_id: &UserId, message: JsonValue, _progress: Option<Arc<ProgressCallback>>) -> Result<JsonValue, String> {
        // Count calls
        if let Ok(mut c) = self.calls.lock() { *c += 1; }
        let addr = network_source_key.inet_socket_address.expect("addr required");
        // Should receive a RelayCheck
        let is_check = message.get("type").and_then(|v| v.as_str()) == Some("Check") && message.get("app").map(|v| v.is_null()).unwrap_or(true);
        if !is_check { return Err("unexpected message".to_string()); }
        // Lookup availability
        let available = self.availability.lock().ok()
            .and_then(|v| v.iter().find(|(a, _)| *a == addr).cloned())
            .map(|(_, ok)| ok)
            .unwrap_or(false);
        let resp = serde_json::json!({ "app": null, "type": "CheckResponse", "available": available });
        Ok(resp)
    }

    fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
}

#[test]
fn relay_finder_caches_successful_root_relay() {
    // Prepare two candidate relays
    let a1: SocketAddr = "127.0.0.1:40001".parse().unwrap();
    let a2: SocketAddr = "127.0.0.1:40002".parse().unwrap();

    let api = MockApi::new().0.with_availability(vec![(a1, false), (a2, true)]);
    let api_arc: Arc<dyn BingleApi + Send + Sync> = Arc::new(api);

    // Discovery closure
    let discover = Arc::new(move || vec![
        RootRelayInfo { id: "ADDR1".to_string(), address: a1 },
        RootRelayInfo { id: "ADDR2".to_string(), address: a2 },
    ]);

    let finder = RelayFinder::new(api_arc.clone(), Duration::from_millis(200), discover);

    // Any id will do; finder will test preferred then alternate and choose an available one.
    let picked = finder.find_root_relay("SOME-ID").expect("should pick an available relay");
    assert!(picked.address == a1 || picked.address == a2);

    // Second call within TTL should return cached without new RelayCheck
    let calls_before = *api_arc.as_any().downcast_ref::<MockApi>().unwrap().calls.lock().unwrap();
    let _ = finder.find_root_relay("SOME-ID").expect("cached");
    let calls_after = *api_arc.as_any().downcast_ref::<MockApi>().unwrap().calls.lock().unwrap();
    assert_eq!(calls_before, calls_after, "should not perform a second RelayCheck within TTL");

    // After TTL expires, a new RelayCheck should be attempted
    std::thread::sleep(Duration::from_millis(220));
    let _ = finder.find_root_relay("SOME-ID").expect("rechecked and cached again");
    let calls_final = *api_arc.as_any().downcast_ref::<MockApi>().unwrap().calls.lock().unwrap();
    assert!(calls_final > calls_after, "should perform RelayCheck again after TTL");
}
