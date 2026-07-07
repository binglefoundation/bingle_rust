use serde_json::{Value as JsonValue, json};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth, to_weak_api_both};
use bingle_core::api::bingle_api::{BingleError, NetworkEndpoint};
use bingle_core::ddb::{AdvertRecord, DdbClient, DdbClientImpl, InetSocketAddress};
use bingle_core::relay::relay_finder::RelayInfo;

#[path = "../../test_util.rs"]
pub mod test_util;

struct MockLookupApi {
    pub response: JsonValue,
}

impl InnerBingleApi for MockLookupApi {
    fn list_all_relays(&self, _include_self: bool) -> Vec<RelayInfo> {
        vec![test_util::signed_root_relay(
            test_util::ADDRESS_RECEIVE,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9999),
        )]
    }

    fn send_message_to_network_with_response(
        &self,
        _nsk: &NetworkEndpoint,
        _uid: &bingle_core::api::bingle_api::UserId,
        msg: JsonValue,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<JsonValue, BingleError> {
        if msg.get("type").and_then(|v| v.as_str()) == Some("getRelaysStatus") {
            return Ok(json!({
                "app": "ddb",
                "type": "relaysStatusResponse",
                "responderState": "available",
                "epochId": 0,
                "treeOrder": 0,
                "relayIds": [test_util::ADDRESS_RECEIVE],
                "relayEndpoints": [{"host": "127.0.0.1", "port": 9999}],
                "relayStates": ["available"]
            }));
        }
        Ok(self.response.clone())
    }

    fn get_my_id(&self) -> Option<String> {
        Some("client-id".to_string())
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn ddb_client_lookup_fails_on_invalid_signature() {
    // 1. Create an AdvertRecord with an invalid signature
    let mut record = AdvertRecord::new_unsigned(
        test_util::ADDRESS_RECEIVE.to_string(),
        Some(InetSocketAddress {
            host: "127.0.0.1".to_string(),
            port: 1234,
        }),
        None,
        None,
        None,
        "2023-01-01T00:00:00Z".to_string(),
    );
    record.sig = Some("invalid-signature".to_string());

    let response = json!({
        "app": "ddb",
        "type": "queryResponse",
        "found": true,
        "advert": record
    });

    let mock_api = Arc::new(MockLookupApi { response });
    let relay_id = test_util::ADDRESS_RECEIVE.to_string();
    let relay_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9999);
    let discover = Arc::new(move || vec![test_util::signed_root_relay(&relay_id, relay_addr)]);

    let cli = DdbClientImpl::with_discovery(
        to_weak_api_both(MockApiBoth::new_with_api_override(mock_api)),
        discover,
    );

    // 2. lookup should fail because of invalid signature
    let result = cli.lookup("some-id");

    match result {
        Err(BingleError::Other(e)) => {
            assert!(
                e.contains("signature"),
                "Error message should mention signature failure, got: {}",
                e
            );
        }
        _ => panic!("Expected signature verification error, got {:?}", result),
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn ddb_client_lookup_fails_on_missing_signature() {
    // 1. Create an AdvertRecord without a signature
    let record = AdvertRecord::new_unsigned(
        test_util::ADDRESS_RECEIVE.to_string(),
        Some(InetSocketAddress {
            host: "127.0.0.1".to_string(),
            port: 1234,
        }),
        None,
        None,
        None,
        "2023-01-01T00:00:00Z".to_string(),
    );

    let response = json!({
        "app": "ddb",
        "type": "queryResponse",
        "found": true,
        "advert": record
    });

    let mock_api = Arc::new(MockLookupApi { response });
    let relay_id = test_util::ADDRESS_RECEIVE.to_string();
    let relay_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9999);
    let discover = Arc::new(move || vec![test_util::signed_root_relay(&relay_id, relay_addr)]);

    let cli = DdbClientImpl::with_discovery(
        to_weak_api_both(MockApiBoth::new_with_api_override(mock_api)),
        discover,
    );

    // 2. lookup should fail because of missing signature
    let result = cli.lookup("some-id");

    match result {
        Err(BingleError::Other(e)) => {
            assert!(
                e.contains("missing signature") || e.contains("signature"),
                "Error message should mention signature, got: {}",
                e
            );
        }
        _ => panic!("Expected signature verification error, got {:?}", result),
    }
}
