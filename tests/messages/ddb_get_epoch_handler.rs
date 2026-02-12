use std::sync::{Arc, Mutex};

use rust_comms::messages::handlers::{DefaultPrintingHandler, MessageHandler};
use rust_comms::messages::router::Router;
use rust_comms::messages::types::*;
use rust_comms::api::bingle_api::{BingleApi, StartOptions, Handle, NetworkEndpoint, UserId, ProgressCallback, OnMessageHandler, OnConnectHandler, BingleApiInternal};

// Minimal API for router context
#[derive(Clone)]
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
    fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _network_source_key: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_network_with_response(&self, _network_source_key: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
}

struct InternalAvailable;
impl BingleApiInternal for InternalAvailable {
    fn get_relay_state(&self) -> String { "available".into() }
    fn set_state(&self, _state: rust_comms::engine::EngineState) {}
    fn get_state(&self) -> rust_comms::engine::EngineState { rust_comms::engine::EngineState::EndpointAvailable }
    fn set_nat_type(&self, _nat: rust_comms::engine::NatType) {}
    fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> { None }
    fn ddb_register_ip(&self, _endpoint: std::net::SocketAddr) -> Result<(), String> { Ok(()) }
    fn ddb_register_relay(&self, _relay_id: String, _relay_sig: Option<String>) -> Result<(), String> { Ok(()) }
    fn update_turn_listener_relay(&self, _relay_id: String, _relay_addr: std::net::SocketAddr) -> Result<(), String> { Ok(()) }
    fn turn_client_handle_listen_response(&self, _relay_addr: std::net::SocketAddr, _relay_id: String) { }
    fn turn_lookup_addr_by_id(&self, _id: String) -> Option<std::net::SocketAddr> { None }
    fn turn_handle_call(&self, _source: std::net::SocketAddr, _dest: std::net::SocketAddr) -> i32 { -1 }
    fn turn_handle_listen(&self, _id: String, _source: std::net::SocketAddr) -> bool { false }
    fn turn_handle_called(&self, _source: std::net::SocketAddr, _dest: std::net::SocketAddr, _channel: u16) { }
    fn notify_listening(&self, _listening: bool) {}
}

struct InternalStarting;
impl BingleApiInternal for InternalStarting {
    fn get_relay_state(&self) -> String { "starting".into() }
    fn set_state(&self, _state: rust_comms::engine::EngineState) {}
    fn get_state(&self) -> rust_comms::engine::EngineState { rust_comms::engine::EngineState::StunIdentify }
    fn set_nat_type(&self, _nat: rust_comms::engine::NatType) {}
    fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> { None }
    fn ddb_register_ip(&self, _endpoint: std::net::SocketAddr) -> Result<(), String> { Ok(()) }
    fn ddb_register_relay(&self, _relay_id: String, _relay_sig: Option<String>) -> Result<(), String> { Ok(()) }
    fn update_turn_listener_relay(&self, _relay_id: String, _relay_addr: std::net::SocketAddr) -> Result<(), String> { Ok(()) }
    fn turn_client_handle_listen_response(&self, _relay_addr: std::net::SocketAddr, _relay_id: String) { }
    fn turn_lookup_addr_by_id(&self, _id: String) -> Option<std::net::SocketAddr> { None }
    fn turn_handle_call(&self, _source: std::net::SocketAddr, _dest: std::net::SocketAddr) -> i32 { -1 }
    fn turn_handle_listen(&self, _id: String, _source: std::net::SocketAddr) -> bool { false }
    fn turn_handle_called(&self, _source: std::net::SocketAddr, _dest: std::net::SocketAddr, _channel: u16) { }
    fn notify_listening(&self, _listening: bool) {}
}

#[test]
fn ddb_get_epoch_returns_epoch_info_when_relay_available() {
    // Arrange router as relay with internal state available and ddb backend
    let router = Arc::new(Router::new(crate::util::mock_bingle_api::to_weak(MockApi)));
    router.set_am_relay(true);
    router.set_bingle_api_internal(Some(Arc::new(InternalAvailable) as Arc<dyn BingleApiInternal>));
    let backend = Arc::new(Mutex::new(rust_comms::ddb::InMemoryDdbBackend::new()));
    {
        let mut b = backend.lock().unwrap();
        b.upsert(AdvertRecord { id: "RID1".into(), endpoint: Some(InetSocketAddress { host: "127.0.0.1".into(), port: 4001 }), am_relay: Some(true), relay_id: None, relay_sig: None, date: "1970-01-01T00:00:00Z".into(), sig: None });
        b.upsert(AdvertRecord { id: "RID2".into(), endpoint: Some(InetSocketAddress { host: "127.0.0.1".into(), port: 4002 }), am_relay: Some(true), relay_id: None, relay_sig: None, date: "1970-01-01T00:00:00Z".into(), sig: None });
        b.upsert(AdvertRecord { id: "NODE".into(), endpoint: None, am_relay: Some(false), relay_id: None, relay_sig: None, date: "1970-01-01T00:00:00Z".into(), sig: None });
    }
    router.set_ddb_backend(Some(backend.clone()));

    // Act: route GetEpoch
    let get = DdbGetEpoch { app: "ddb".into(), epoch_id: -1, tag: None, response_tag: Some("rt1".into()), text: None, data: None };
    let msg = Message::Ddb(DdbMessage::GetEpoch(get));

    let handler = DefaultPrintingHandler;
    Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "SENDER.");
    });

    // Assert: outbound response exists and is EpochInfo
    let out = router.take_outbound_response().expect("expected response");
    assert_eq!(out.get("app").and_then(|v: &serde_json::Value| v.as_str()), Some("ddb"));
    assert_eq!(out.get("type").and_then(|v: &serde_json::Value| v.as_str()), Some("getEpochResponse"));
    assert_eq!(out.get("responseTag").and_then(|v: &serde_json::Value| v.as_str()), Some("rt1"));
    let ids = out.get("relayIds").and_then(|v| v.as_array()).expect("relayIds array");
    let id_list: Vec<String> = ids.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
    assert!(id_list.contains(&"RID1".to_string()));
    assert!(id_list.contains(&"RID2".to_string()));
    let eps = out.get("relayEndpoints").and_then(|v| v.as_array()).expect("relayEndpoints array");
    assert!(eps.len() >= 2);
}

#[test]
fn ddb_get_epoch_returns_fail_when_not_allowed() {
    // Case 1: not a relay
    let router = Arc::new(Router::new(crate::util::mock_bingle_api::to_weak(MockApi)));
    router.set_am_relay(false);
    router.set_bingle_api_internal(Some(Arc::new(InternalStarting) as Arc<dyn BingleApiInternal>));
    let handler = DefaultPrintingHandler;
    let get = DdbGetEpoch { app: "ddb".into(), epoch_id: 0, tag: None, response_tag: Some("aaa".into()), text: None, data: None };
    let msg = Message::Ddb(DdbMessage::GetEpoch(get));
    Router::with_current_router(router.clone(), || { router.route(&handler, &msg, "SENDER."); });
    let out1 = router.take_outbound_response().expect("response");
    assert_eq!(out1.get("type").and_then(|v: &serde_json::Value| v.as_str()), Some("fail"));

    // Case 2: relay but not available
    let router2 = Arc::new(Router::new(crate::util::mock_bingle_api::to_weak(MockApi)));
    router2.set_am_relay(true);
    router2.set_bingle_api_internal(Some(Arc::new(InternalStarting) as Arc<dyn BingleApiInternal>));
    Router::with_current_router(router2.clone(), || { router2.route(&handler, &msg, "SENDER."); });
    let out2 = router2.take_outbound_response().expect("response");
    assert_eq!(out2.get("type").and_then(|v: &serde_json::Value| v.as_str()), Some("fail"));
}
