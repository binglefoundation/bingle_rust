use std::sync::{Arc, Mutex};

use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, OnConnectHandler, OnMessageHandler, ProgressCallback, StartOptions, UserId};
use rust_comms::ddb::DdbBackend;
use rust_comms::messages::handlers::DefaultPrintingHandler;
use rust_comms::messages::router::Router;
use rust_comms::messages::types::*;

// Minimal API impl so router can pass an API into the handler
struct MockApi;
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
    fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _network_source_key: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_network_with_response(&self, _network_source_key: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
}

impl rust_comms::api::bingle_api::BingleApiInternal for MockApi {
    fn set_state(&self, _s: rust_comms::engine::EngineState) {}
    fn get_state(&self) -> rust_comms::engine::EngineState { rust_comms::engine::EngineState::StunIdentify }
    fn set_nat_type(&self, _n: rust_comms::engine::NatType) {}
    fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> { None }
    fn ddb_register_ip(&self, _e: std::net::SocketAddr, _a: bool) -> Result<(), String> { Ok(()) }
    fn ddb_register_relay(&self, _r: String, _s: Option<String>) -> Result<(), String> { Ok(()) }
    fn update_turn_listener_relay(&self, _r: String, _a: std::net::SocketAddr) -> Result<(), String> { Ok(()) }
    fn turn_client_handle_listen_response(&self, _a: std::net::SocketAddr, _r: String) {}
    fn turn_lookup_addr_by_id(&self, _i: String) -> Option<std::net::SocketAddr> { None }
    fn turn_handle_call(&self, _s: std::net::SocketAddr, _d: std::net::SocketAddr) -> i32 { -1 }
    fn turn_handle_listen(&self, _i: String, _s: std::net::SocketAddr) -> bool { false }
    fn turn_handle_called(&self, _s: std::net::SocketAddr, _d: std::net::SocketAddr, _c: u16) {}
    fn notify_listening(&self, _l: bool) {}
    fn get_relay_state(&self) -> String { "off".into() }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn ddb_init_resolve_triggers_snapshot_and_dump() {
    // Prepare router and context
    let router = Arc::new(Router::new(crate::util::mock_bingle_api::to_weak(MockApi)));
    // Capture sent messages
    let captured: Arc<Mutex<Vec<(String, serde_json::Value)>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_clone = captured.clone();
    router.set_sender(Some(Arc::new(move |_nsk, user_id, json| {
        // Record the destination user_id and the JSON payload
        captured_clone.lock().unwrap().push((user_id.to_string(), json));
        true
    })));

    // Mark as relay and provide DDB backend with 2 records
    router.set_am_relay(true);
    let backend = Arc::new(Mutex::new(rust_comms::ddb::InMemoryDdbBackend::new()));
    {
        let mut b = backend.lock().unwrap();
        b.upsert(AdvertRecord { id: "A".into(), endpoint: None, am_relay: Some(false), relay_id: None, relay_sig: None, date: "2025-01-01T00:00:00Z".into(), sig: None });
        b.upsert(AdvertRecord { id: "B".into(), endpoint: None, am_relay: None, relay_id: None, relay_sig: None, date: "2025-01-02T00:00:00Z".into(), sig: None });
    }
    router.set_ddb_backend(Some(backend.clone()));

    // Route an InitResolve message from issuer "NEWPEER." (note trailing dot)
    let init = DdbInitResolve { app: "ddb".into(), tag: None, response_tag: Some("rt1".into()), text: None, data: None };
    let msg = Message::Ddb(DdbMessage::InitResolve(init));

    let handler = DefaultPrintingHandler;
    Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "NEWPEER.");
    });

    // Verify captured messages
    let sent = captured.lock().unwrap().clone();
    assert!(sent.len() >= 1, "no messages were sent by handler");

    // First should be initResponse
    let (uid0, j0) = &sent[0];
    assert_eq!(uid0, "NEWPEER");
    assert_eq!(j0.get("app").and_then(|v: &serde_json::Value| v.as_str()), Some("ddb"));
    assert_eq!(j0.get("type").and_then(|v: &serde_json::Value| v.as_str()), Some("initResponse"));
    assert_eq!(j0.get("dbCount").and_then(|v| v.as_i64()), Some(2));
    assert_eq!(j0.get("responseTag").and_then(|v: &serde_json::Value| v.as_str()), Some("rt1"));

    // The remaining messages should be dumpResolve, count equals number of records (2)
    let dumps: Vec<&serde_json::Value> = sent.iter().skip(1).map(|(_, j)| j).collect();
    assert_eq!(dumps.len(), 2);
    for d in &dumps {
        assert_eq!(d.get("type").and_then(|v: &serde_json::Value| v.as_str()), Some("dumpResolve"));
        assert_eq!(d.get("app").and_then(|v: &serde_json::Value| v.as_str()), Some("ddb"));
        // Validate record.id exists
        let rec_id = d.get("record").and_then(|r| r.get("id")).and_then(|v: &serde_json::Value| v.as_str());
        assert!(rec_id == Some("A") || rec_id == Some("B"));
    }
}
