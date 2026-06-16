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
    fn start(&mut self, _options: &StartOptions) -> Result<(), rust_comms::api::bingle_api::BingleError> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}
    fn handle_lookup(&self, _handle: &Handle) -> Result<Option<UserId>, rust_comms::api::bingle_api::BingleError> { Ok(None) }
    fn handle_lookup_by_id(&self, _user_id: &UserId) -> Option<Handle> { None }
    fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<bool, rust_comms::api::bingle_api::BingleError> { Ok(false) }
    fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<bool, rust_comms::api::bingle_api::BingleError> { Ok(false) }
    fn send_message_to_network(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<bool, rust_comms::api::bingle_api::BingleError> { Ok(false) }
    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> { Err(rust_comms::api::bingle_api::BingleError::Other("ni".into())) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> { Err(rust_comms::api::bingle_api::BingleError::Other("ni".into())) }
    fn send_message_to_network_with_response(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> { Err(rust_comms::api::bingle_api::BingleError::Other("ni".into())) }
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

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ddb_get_relays_status_returns_response_when_relay_available() {
    // Arrange router as relay with internal state available and ddb backend
    let router = Arc::new(Router::new(crate::util::mock_bingle_api::to_weak(MockApi)));
    router.set_am_relay(true);
    router.set_bingle_api(Some(crate::util::mock_bingle_api::to_weak(MockApiBoth::new_with_internal_override(Arc::new(InternalAvailable)))));
    let backend = Arc::new(Mutex::new(rust_comms::ddb::InMemoryDdbBackend::new()));
    {
        let mut b = backend.lock().unwrap();
        b.upsert(AdvertRecord::new_unsigned("RID1".into(), Some(InetSocketAddress { host: "127.0.0.1".into(), port: 4001 }), Some(true), None, None, "1970-01-01T00:00:00Z".into()));
        b.upsert(AdvertRecord::new_unsigned("RID2".into(), Some(InetSocketAddress { host: "127.0.0.1".into(), port: 4002 }), Some(true), None, None, "1970-01-01T00:00:00Z".into()));
        b.upsert(AdvertRecord::new_unsigned("NODE".into(), None, Some(false), None, None, "1970-01-01T00:00:00Z".into()));
    }
    router.set_ddb_backend(Some(backend.clone()));

    // Act: route getRelaysStatus to router
    let get = DdbGetRelaysStatus { app: "ddb".into(), epoch_id: -1, tag: Some("get_relays_status_tag".to_string()), text: None, data: None };
    router.set_last_response_tag(Some("rt1".to_string()));
    let msg = Message::Ddb(DdbMessage::GetRelaysStatus(get));

    let handler = DefaultPrintingHandler;
    let responses = Router::with_current_router(router.clone(), || {
        router.route(&handler, &msg, "SENDER.")
    });

    // Assert: outbound response exists and is relays status response
    let out = responses.into_iter().next().expect("expected response");
    assert_eq!(out.get("app").and_then(|v: &serde_json::Value| v.as_str()), Some("ddb"));
    assert_eq!(out.get("type").and_then(|v: &serde_json::Value| v.as_str()), Some("relaysStatusResponse"));
    assert_eq!(out.get("responseTag").and_then(|v: &serde_json::Value| v.as_str()), Some("get_relays_status_tag"));
    assert_eq!(out.get("responderState").and_then(|v| v.as_str()), Some("available"));
    let ids = out.get("relayIds").and_then(|v| v.as_array()).expect("relayIds array");
    let id_list: Vec<String> = ids.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
    assert!(id_list.contains(&"RID1".to_string()));
    assert!(id_list.contains(&"RID2".to_string()));
    let relay_states = out
        .get("relayStates")
        .and_then(|v| v.as_array())
        .expect("relayStates array");
    assert_eq!(relay_states.len(), ids.len());
    assert!(relay_states.iter().all(|s| s.as_str() == Some("available")));
    let eps = out.get("relayEndpoints").and_then(|v| v.as_array()).expect("relayEndpoints array");
    assert!(eps.len() >= 2);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ddb_get_relays_status_returns_fail_when_not_allowed() {
    // Case 1: not a relay
    let router = Arc::new(Router::new(crate::util::mock_bingle_api::to_weak(MockApi)));
    router.set_am_relay(false);
    // router.set_bingle_api_internal(Some(Arc::new(InternalStarting) as Arc<dyn BingleApiInternal>));
    let handler = DefaultPrintingHandler;
    let get = DdbGetRelaysStatus { app: "ddb".into(), epoch_id: 0, tag: None, text: None, data: None };
    let msg = Message::Ddb(DdbMessage::GetRelaysStatus(get));
    let responses1 = Router::with_current_router(router.clone(), || { router.route(&handler, &msg, "SENDER.") });
    let out1 = responses1.into_iter().next().expect("response");
    assert_eq!(out1.get("type").and_then(|v: &serde_json::Value| v.as_str()), Some("fail"));

    // Case 2: relay but not available
    let router2 = Arc::new(Router::new(crate::util::mock_bingle_api::to_weak(MockApi)));
    router2.set_am_relay(true);
    router2.set_bingle_api(Some(crate::util::mock_bingle_api::to_weak(MockApiBoth::new_with_internal_override(Arc::new(InternalStarting)))));
    let responses2 = Router::with_current_router(router2.clone(), || { router2.route(&handler, &msg, "SENDER.") });
    let out2 = responses2.into_iter().next().expect("response");
    assert_eq!(out2.get("type").and_then(|v: &serde_json::Value| v.as_str()), Some("fail"));
}
