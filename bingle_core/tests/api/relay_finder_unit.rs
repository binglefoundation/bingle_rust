use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use bingle_core::api::bingle_api::{
    BingleApi, Handle, NetworkEndpoint, OnConnectHandler, OnMessageHandler, ProgressCallback,
    StartOptions, UserId,
};
use bingle_core::ddb::InetSocketAddress;
use bingle_core::engine::RelayState;
use bingle_core::messages::marshal::to_json_value;
use bingle_core::messages::types::{DdbMessage, DdbRelaysStatusResponse, Message};
use bingle_core::relay::relay_finder::{RelayFinder, RelayFinderTestTrait};
use serde_json::Value as JsonValue;

#[path = "../test_util.rs"]
pub mod test_util;

#[derive(Clone)]
struct MockApi {
    calls: Arc<Mutex<usize>>,
    relays: Vec<(String, SocketAddr, RelayState)>,
}

impl MockApi {
    fn new(relays: Vec<(String, SocketAddr, RelayState)>) -> (Self, Arc<Mutex<usize>>) {
        let calls = Arc::new(Mutex::new(0));
        let api = Self {
            calls: calls.clone(),
            relays,
        };
        (api, calls)
    }
}

impl BingleApi for MockApi {
    fn list_all_relays(
        &self,
        _include_self: bool,
    ) -> Vec<bingle_core::relay::relay_finder::RelayInfo> {
        Vec::new()
    }
    fn set_on_listening(
        &self,
        _handler: Option<std::sync::Arc<bingle_core::api::bingle_api::OnListeningHandler>>,
    ) {
    }
    fn get_algo_provider_config(
        &self,
    ) -> Option<bingle_core::blockchain::algo_ops::AlgoChainConfig> {
        None
    }
    fn get_handle(&self) -> Option<String> {
        None
    }
    fn get_user_id(&self) -> Option<String> {
        None
    }
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> {
        None
    }
    fn get_app_id(&self) -> Option<u64> {
        None
    }
    fn start(
        &self,
        _options: &StartOptions,
    ) -> Result<(), bingle_core::api::bingle_api::BingleError> {
        Ok(())
    }
    fn stop(&self) {}
    fn network_change(&self) {}
    fn handle_lookup(
        &self,
        _handle: &Handle,
    ) -> Result<Option<UserId>, bingle_core::api::bingle_api::BingleError> {
        Ok(None)
    }
    fn handle_lookup_partial(
        &self,
        _handle: &Handle,
    ) -> Result<Option<(UserId, Handle)>, bingle_core::api::bingle_api::BingleError> {
        Ok(None)
    }
    fn handle_lookup_by_id(&self, _user_id: &UserId) -> Option<Handle> {
        None
    }
    fn send_message_to_id(
        &self,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
        Ok(false)
    }
    fn send_message_to_handle(
        &self,
        _handle: &Handle,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
        Ok(false)
    }
    fn send_message_to_network(
        &self,
        _network_source_key: &NetworkEndpoint,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<bool, bingle_core::api::bingle_api::BingleError> {
        Ok(false)
    }
    fn send_message_to_id_with_response(
        &self,
        _user_id: &UserId,
        _message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, bingle_core::api::bingle_api::BingleError> {
        Err(bingle_core::api::bingle_api::BingleError::Other(
            "ni".to_string(),
        ))
    }
    fn send_message_to_handle_with_response(
        &self,
        _handle: &Handle,
        _message: serde_json::Value,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, bingle_core::api::bingle_api::BingleError> {
        Err(bingle_core::api::bingle_api::BingleError::Other(
            "ni".to_string(),
        ))
    }

    fn send_message_to_network_with_response(
        &self,
        _network_source_key: &NetworkEndpoint,
        _user_id: &UserId,
        message: JsonValue,
        _progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, bingle_core::api::bingle_api::BingleError> {
        let ty = message
            .get("type")
            .and_then(|v: &serde_json::Value| v.as_str())
            .unwrap_or("");
        let app = message.get("app").and_then(|v| v.as_str()).unwrap_or("");
        if ty == "getRelaysStatus" && app == "ddb" {
            if let Ok(mut c) = self.calls.lock() {
                *c += 1;
            }
            let response = DdbRelaysStatusResponse {
                app: "ddb".to_string(),
                responder_state: RelayState::Available,
                epoch_id: 1,
                tree_order: 2,
                relay_ids: self.relays.iter().map(|(id, _, _)| id.clone()).collect(),
                relay_endpoints: Some(
                    self.relays
                        .iter()
                        .map(|(_, addr, _)| InetSocketAddress::from(*addr))
                        .collect(),
                ),
                relay_states: self.relays.iter().map(|(_, _, state)| *state).collect(),
                response_tag: None,
                text: None,
                data: None,
            };
            Ok(to_json_value(&Message::Ddb(
                DdbMessage::RelaysStatusResponse(response),
            )))
        } else {
            Err(bingle_core::api::bingle_api::BingleError::Other(
                "unexpected message".to_string(),
            ))
        }
    }

    fn set_on_message(&self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&self, _handler: Option<Arc<OnConnectHandler>>) {}
}

impl bingle_core::api::bingle_api::BingleApiInternal for MockApi {
    fn get_relay_state(&self) -> String {
        "off".to_string()
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_finder_picks_available_relay_via_get_relays_status() {
    let a1: SocketAddr = "127.0.0.1:40001".parse().unwrap();
    let a2: SocketAddr = "127.0.0.1:40002".parse().unwrap();
    let id1 = "OO3BIFZDJPGMNXZ74NOVH5KZ5WBL3KCPLPELAF32P7HDCQGQIBID7PJC7A".to_string();
    let id2 = "4TKGNGRAUHMQI4EOQ34L2AIDX2VGS4OZNZIOE6BLEQFZUDRSB6RJRBPVRE".to_string();

    // Both relays report themselves as Available (responder_state), with both listed as Available in relay_states
    let (api_raw, calls_counter) = MockApi::new(vec![
        (id1.clone(), a1, RelayState::Available),
        (id2.clone(), a2, RelayState::Available),
    ]);
    let api_arc: Arc<dyn bingle_core::api::bingle_api::BingleApiBoth> = Arc::new(api_raw);
    let api_weak = Arc::downgrade(&api_arc);

    let discover = Arc::new(move || {
        vec![
            test_util::signed_root_relay(&id1, a1),
            test_util::signed_root_relay(&id2, a2),
        ]
    });

    let finder = RelayFinder::new(api_weak, discover);

    let picked = finder
        .find_root_relay("SOME-ID")
        .expect("should pick an available relay");
    assert!(
        picked.address() == a1 || picked.address() == a2,
        "picked relay should be one of the candidates"
    );
    assert_eq!(
        picked.state,
        Some(RelayState::Available),
        "picked relay should be available"
    );

    let calls = *calls_counter.lock().expect("calls lock should succeed");
    assert!(
        calls >= 1,
        "at least one GetRelaysStatus call should have been made"
    );
}
