use std::sync::{Arc, Mutex};

use rust_comms::messages::types::*;
use rust_comms::messages::handlers::{MessageHandler, DefaultPrintingHandler};
use rust_comms::messages::router::Router;
use rust_comms::api::bingle_api::{BingleApi, StartOptions, Handle, NetworkEndpoint, UserId, ProgressCallback, OnMessageHandler, OnConnectHandler};

// Minimal API impl so router can pass an API into the handler
struct MockApi;
impl BingleApi for MockApi {
    fn get_my_id(&self) -> Option<String> { None }
    fn get_app_id(&self) -> Option<u64> { None }
    fn start(&mut self, _options: &StartOptions) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}
    fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _network_source_key: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_network_with_response(&self, _network_source_key: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
}

#[test]
fn ddb_init_resolve_triggers_snapshot_and_dump() {
    // Prepare router and context
    let router = Arc::new(Router::new(Arc::new(MockApi)));
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
    assert_eq!(j0.get("app").and_then(|v| v.as_str()), Some("ddb"));
    assert_eq!(j0.get("type").and_then(|v| v.as_str()), Some("initResponse"));
    assert_eq!(j0.get("dbCount").and_then(|v| v.as_i64()), Some(2));
    assert_eq!(j0.get("responseTag").and_then(|v| v.as_str()), Some("rt1"));

    // The remaining messages should be dumpResolve, count equals number of records (2)
    let dumps: Vec<&serde_json::Value> = sent.iter().skip(1).map(|(_, j)| j).collect();
    assert_eq!(dumps.len(), 2);
    for d in &dumps {
        assert_eq!(d.get("type").and_then(|v| v.as_str()), Some("dumpResolve"));
        assert_eq!(d.get("app").and_then(|v| v.as_str()), Some("ddb"));
        // Validate record.id exists
        let rec_id = d.get("record").and_then(|r| r.get("id")).and_then(|v| v.as_str());
        assert!(rec_id == Some("A") || rec_id == Some("B"));
    }
}
