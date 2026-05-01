use std::sync::{Arc, Mutex};

use crate::util::reusable_mock_api::MockApiBoth;
use rust_comms::api::bingle_api::{BingleApi, BingleApiInternal, Handle, NetworkEndpoint, OnConnectHandler, OnMessageHandler, ProgressCallback, StartOptions, UserId};
use rust_comms::ddb::{AdvertRecord, DdbBackend, InetSocketAddress};
use rust_comms::messages::handlers::DefaultPrintingHandler;
use rust_comms::messages::router::Router;
use rust_comms::messages::types::*;

// Minimal API for router context
#[derive(Clone)]
struct MockApi;
impl BingleApi for MockApi { 
    fn list_all_relays(&self, _include_self: bool) -> Vec<rust_comms::relay::relay_finder::RelayInfo> { Vec::new() }
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
    fn handle_lookup_by_id(&self, _user_id: &UserId) -> Option<Handle> { None }
    fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _network_source_key: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_network_with_response(&self, _network_source_key: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
}

impl BingleApiInternal for MockApi {
    fn get_relay_state(&self) -> String { "off".into() }
}

struct InternalAvailable;
impl crate::util::reusable_mock_api::InnerBingleApiInternal for InternalAvailable {
    fn get_relay_state(&self) -> String { "available".into() }
}
impl BingleApiInternal for InternalAvailable {
    fn get_relay_state(&self) -> String { "available".into() }
}

struct InternalStarting;
impl crate::util::reusable_mock_api::InnerBingleApiInternal for InternalStarting {
    fn get_relay_state(&self) -> String { "starting".into() }
}
impl BingleApiInternal for InternalStarting {
    fn get_relay_state(&self) -> String { "starting".into() }
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn ddb_get_epoch_returns_epoch_info_when_relay_available() {
    // Arrange router as relay with internal state available and ddb backend
    let router = Arc::new(Router::new(crate::util::mock_bingle_api::to_weak(MockApi)));
    router.set_am_relay(true);
    router.set_bingle_api(Some(crate::util::mock_bingle_api::to_weak(MockApiBoth::new_with_internal_override(Arc::new(InternalAvailable)))));
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
    router.set_last_response_tag(Some("rt1".to_string()));
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

#[cfg_attr(not(target_os = "ios"), test)]
pub fn ddb_get_epoch_returns_fail_when_not_allowed() {
    // Case 1: not a relay
    let router = Arc::new(Router::new(crate::util::mock_bingle_api::to_weak(MockApi)));
    router.set_am_relay(false);
    // router.set_bingle_api_internal(Some(Arc::new(InternalStarting) as Arc<dyn BingleApiInternal>));
    let handler = DefaultPrintingHandler;
    let get = DdbGetEpoch { app: "ddb".into(), epoch_id: 0, tag: None, response_tag: Some("aaa".into()), text: None, data: None };
    let msg = Message::Ddb(DdbMessage::GetEpoch(get));
    Router::with_current_router(router.clone(), || { router.route(&handler, &msg, "SENDER."); });
    let out1 = router.take_outbound_response().expect("response");
    assert_eq!(out1.get("type").and_then(|v: &serde_json::Value| v.as_str()), Some("fail"));

    // Case 2: relay but not available
    let router2 = Arc::new(Router::new(crate::util::mock_bingle_api::to_weak(MockApi)));
    router2.set_am_relay(true);
    router2.set_bingle_api(Some(crate::util::mock_bingle_api::to_weak(MockApiBoth::new_with_internal_override(Arc::new(InternalStarting)))));
    Router::with_current_router(router2.clone(), || { router2.route(&handler, &msg, "SENDER."); });
    let out2 = router2.take_outbound_response().expect("response");
    assert_eq!(out2.get("type").and_then(|v: &serde_json::Value| v.as_str()), Some("fail"));
}
