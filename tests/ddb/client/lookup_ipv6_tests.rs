use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use serde_json::json;

use crate::util::reusable_mock_api::{to_weak_api_both, InnerBingleApi, MockApiBoth};
use rust_comms::api::bingle_api::{BingleError, NetworkEndpoint};
use rust_comms::ddb::{DdbClient, DdbClientImpl};
use rust_comms::relay::relay_finder::RelayInfo;

const RELAY_ID: &str = "OO3BIFZDJPGMNXZ74NOVH5KZ5WBL3KCPLPELAF32P7HDCQGQIBID7PJC7A";

struct MockApi {
    ipv6: bool,
}

impl InnerBingleApi for MockApi {
    fn get_my_id(&self) -> Option<String> {
        Some("4TKGNGRAUHMQI4EOQ34L2AIDX2VGS4OZNZIOE6BLEQFZUDRSB6RJRBPVRE".to_string())
    }
    fn send_message_to_network_with_response(
        &self,
        _nsk: &NetworkEndpoint,
        _uid: &rust_comms::api::bingle_api::UserId,
        msg: serde_json::Value,
        _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>,
    ) -> Result<serde_json::Value, BingleError> {
        let ty = msg.get("type").and_then(|v| v.as_str());
        if ty == Some("getRelaysStatus") {
            return Ok(json!({
                "app": "ddb",
                "type": "relaysStatusResponse",
                "responderState": "available",
                "epochId": 0,
                "treeOrder": 0,
                "relayIds": [RELAY_ID],
                "relayStates": ["available"],
                "relayEndpoints": [
                    { "host": "127.0.0.1", "port": 1234 }
                ]
            }));
        }
        if ty == Some("queryResolve") {
            if self.ipv6 {
                return Ok(json!({
                    "app": "ddb",
                    "type": "queryResponse",
                    "found": true,
                    "advert": {
                        "id": "SOMEID",
                        "endpoint": {
                            "host": "::1",
                            "port": 4433
                        },
                        "date": "2025-01-01T00:00:00Z"
                    }
                }));
            } else {
                return Ok(json!({
                    "app": "ddb",
                    "type": "queryResponse",
                    "found": true,
                    "advert": {
                        "id": "SOMEID",
                        "endpoint": {
                            "host": "1.2.3.4",
                            "port": 4433
                        },
                        "date": "2025-01-01T00:00:00Z"
                    }
                }));
            }
        }
        Err(BingleError::Other(format!("Unexpected message type: {:?}", ty)))
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn ddb_client_lookup_rejects_ipv6() {
    let mock_api = Arc::new(MockApi { ipv6: true });
    let relay_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234);
    let discover = Arc::new(move || vec![RelayInfo::root(RELAY_ID.to_string(), relay_addr)]);
    
    let cli = DdbClientImpl::with_discovery(
        to_weak_api_both(MockApiBoth::new_with_api_override(mock_api)),
        discover
    );

    let res = cli.lookup("SOMEID");
    
    assert!(res.is_err(), "DDB lookup should reject IPv6 addresses: {:?}", res);
    if let Err(BingleError::Other(e)) = res {
        assert!(e.contains("IPv6") || e.contains("invalid host"), "Error message should mention IPv6 or invalid host: {}", e);
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn ddb_client_lookup_accepts_ipv4() {
    let mock_api = Arc::new(MockApi { ipv6: false });
    let relay_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234);
    let discover = Arc::new(move || vec![RelayInfo::root(RELAY_ID.to_string(), relay_addr)]);
    
    let cli = DdbClientImpl::with_discovery(
        to_weak_api_both(MockApiBoth::new_with_api_override(mock_api)),
        discover
    );

    let res = cli.lookup("SOMEID");
    assert!(res.is_ok(), "DDB lookup should accept IPv4 addresses: {:?}", res);
    let nsk = res.unwrap();
    let addr = nsk.inet_socket_address().expect("should have direct address");
    assert_eq!(addr, "1.2.3.4:4433".parse::<SocketAddr>().unwrap());
}
