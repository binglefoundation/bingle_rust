use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use serde_json::{Value, json};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth, to_weak_api_both};
use bingle_core::api::bingle_api::{BingleError, NetworkEndpoint, SendFailureKind};
use bingle_core::ddb::{AdvertRecord, DdbClient, DdbClientImpl, InetSocketAddress};

const RELAY_ID: &str = "OO3BIFZDJPGMNXZ74NOVH5KZ5WBL3KCPLPELAF32P7HDCQGQIBID7PJC7A";

struct MockApi {
    advert_json: Value,
    address: String,
}

impl InnerBingleApi for MockApi {
    fn get_my_id(&self) -> Option<String> {
        Some(self.address.clone())
    }
    fn send_message_to_network_with_response(
        &self,
        _nsk: &NetworkEndpoint,
        _uid: &bingle_core::api::bingle_api::UserId,
        msg: serde_json::Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
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
            return Ok(json!({
                "app": "ddb",
                "type": "queryResponse",
                "found": true,
                "advert": self.advert_json
            }));
        }
        Err(BingleError::Other(format!(
            "Unexpected message type: {:?}",
            ty
        )))
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn ddb_client_lookup_rejects_ipv6() {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let public_key = signing_key.verifying_key();
    let pk_bytes: [u8; 32] = public_key.to_bytes();
    let address = algo_ops::byte_key_to_address(&pk_bytes).unwrap();

    let advert = AdvertRecord::new(
        address.clone(),
        Some(InetSocketAddress {
            host: "::1".into(),
            port: 4433,
        }),
        None,
        None,
        None,
        "2025-01-01T00:00:00Z".into(),
        &signing_key,
    );
    let advert_json = serde_json::to_value(&advert).unwrap();

    let mock_api = Arc::new(MockApi {
        advert_json,
        address: address.clone(),
    });
    let relay_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234);
    let discover = Arc::new(move || {
        vec![crate::util::test_util::signed_root_relay(
            RELAY_ID, relay_addr,
        )]
    });

    let cli = DdbClientImpl::with_discovery(
        to_weak_api_both(MockApiBoth::new_with_api_override(mock_api)),
        discover,
    );

    let res = cli.lookup(&address);

    assert!(
        res.is_err(),
        "DDB lookup should reject IPv6 addresses: {:?}",
        res
    );
    // An unusable advert endpoint is now a typed MalformedAdvert cause (issue #99).
    if let Err(BingleError::Send {
        kind: SendFailureKind::MalformedAdvert,
        detail,
    }) = res
    {
        assert!(
            detail.contains("IPv6") || detail.contains("invalid host"),
            "Error message should mention IPv6 or invalid host: {}",
            detail
        );
    } else {
        panic!("Expected a MalformedAdvert send error, got {:?}", res);
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn ddb_client_lookup_accepts_ipv4() {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let public_key = signing_key.verifying_key();
    let pk_bytes: [u8; 32] = public_key.to_bytes();
    let address = algo_ops::byte_key_to_address(&pk_bytes).unwrap();

    let advert = AdvertRecord::new(
        address.clone(),
        Some(InetSocketAddress {
            host: "1.2.3.4".into(),
            port: 4433,
        }),
        None,
        None,
        None,
        "2025-01-01T00:00:00Z".into(),
        &signing_key,
    );
    let advert_json = serde_json::to_value(&advert).unwrap();

    let mock_api = Arc::new(MockApi {
        advert_json,
        address: address.clone(),
    });
    let relay_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234);
    let discover = Arc::new(move || {
        vec![crate::util::test_util::signed_root_relay(
            RELAY_ID, relay_addr,
        )]
    });

    let cli = DdbClientImpl::with_discovery(
        to_weak_api_both(MockApiBoth::new_with_api_override(mock_api)),
        discover,
    );

    let res = cli.lookup(&address);
    assert!(
        res.is_ok(),
        "DDB lookup should accept IPv4 addresses: {:?}",
        res
    );
    let nsk = res.unwrap();
    let addr = nsk
        .inet_socket_address()
        .expect("should have direct address");
    assert_eq!(addr, "1.2.3.4:4433".parse::<SocketAddr>().unwrap());
}
