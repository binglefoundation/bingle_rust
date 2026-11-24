use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use serde_json::{Value, json};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth, to_weak_api_both};
use bingle_core::api::bingle_api::{BingleError, NetworkEndpoint, SendFailureKind};
use bingle_core::ddb::{
    AdvertRecord, DdbBackend, DdbClient, DdbClientImpl, InMemoryDdbBackend, InetSocketAddress,
};
use bingle_core::messages::marshal;
use bingle_core::messages::types::{DdbMessage, DdbUpsertResolve, Message};

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

fn new_identity() -> (SigningKey, String) {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let pk_bytes: [u8; 32] = signing_key.verifying_key().to_bytes();
    let address = algo_ops::byte_key_to_address(&pk_bytes).unwrap();
    (signing_key, address)
}

/// Look `address` up through a DDB client whose relay serves `advert_json`.
fn lookup_served(advert_json: Value, address: &str) -> Result<NetworkEndpoint, BingleError> {
    let mock_api = Arc::new(MockApi {
        advert_json,
        address: address.to_string(),
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
    cli.lookup(address)
}

/// Issue #493: an IPv6 `endpoint`, with or without brackets around the host, now parses.
#[test]
#[cfg(not(target_os = "ios"))]
fn ddb_client_lookup_accepts_ipv6() {
    let want: SocketAddr = "[2001:db8::1]:4433".parse().unwrap();
    for host in ["2001:db8::1", "[2001:db8::1]"] {
        let (signing_key, address) = new_identity();
        let advert = AdvertRecord::new(
            address.clone(),
            Some(InetSocketAddress {
                host: host.into(),
                port: 4433,
            }),
            None,
            None,
            None,
            "2025-01-01T00:00:00Z".into(),
            &signing_key,
        );
        let res = lookup_served(serde_json::to_value(&advert).unwrap(), &address);
        let nsk = res.unwrap_or_else(|e| panic!("host {:?} should be accepted: {:?}", host, e));
        assert_eq!(nsk.inet_socket_address(), Some(want), "host {:?}", host);
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn ddb_client_lookup_rejects_non_ip_host() {
    let (signing_key, address) = new_identity();
    let advert = AdvertRecord::new(
        address.clone(),
        Some(InetSocketAddress {
            host: "relay.example.com".into(),
            port: 4433,
        }),
        None,
        None,
        None,
        "2025-01-01T00:00:00Z".into(),
        &signing_key,
    );
    let res = lookup_served(serde_json::to_value(&advert).unwrap(), &address);
    match res {
        Err(BingleError::Send {
            kind: SendFailureKind::MalformedAdvert,
            detail,
        }) => assert!(detail.contains("invalid host"), "detail: {}", detail),
        other => panic!("Expected a MalformedAdvert send error, got {:?}", other),
    }
}

/// Issue #493: a record signed with `endpoint6` still verifies after a relay has received it,
/// stored it in its DDB backend and re-served it to a DDB client.
#[test]
#[cfg(not(target_os = "ios"))]
fn ddb_client_lookup_verifies_endpoint6_record_after_relay_store_and_reserve() {
    let (signing_key, address) = new_identity();
    let advert = AdvertRecord::new_unsigned(
        address.clone(),
        Some(InetSocketAddress {
            host: "203.0.113.9".into(),
            port: 4433,
        }),
        Some(false),
        None,
        None,
        "2025-01-01T00:00:00Z".into(),
    )
    .with_endpoint6(Some(InetSocketAddress {
        host: "2606:4700::1111".into(),
        port: 4434,
    }))
    .signed(&signing_key);

    // The relay receives the record as JSON inside a DDB upsert, gates it and stores it.
    let upsert = Message::Ddb(DdbMessage::UpsertResolve(DdbUpsertResolve {
        app: "ddb".into(),
        start_id: address.clone(),
        epoch: 1,
        record: advert.clone(),
        original_signature: "SIG".into(),
        rippled: false,
        tag: None,
        response_tag: None,
        text: None,
        data: None,
    }));
    let wire = marshal::to_json_value(&upsert);
    let received = match marshal::from_json_value(wire) {
        Ok(Message::Ddb(DdbMessage::UpsertResolve(up))) => up.record,
        other => panic!("expected an UpsertResolve, got {:?}", other),
    };
    assert_eq!(received.check_ingest(), Ok(()));
    let mut backend = InMemoryDdbBackend::new();
    backend.upsert(received);

    // The relay re-serves the stored record; the client verifies it.
    let stored = backend.lookup(&address).expect("relay stored the record");
    assert_eq!(
        stored.endpoint6, advert.endpoint6,
        "endpoint6 kept verbatim"
    );
    let served = serde_json::to_value(&stored).unwrap();
    assert_eq!(served["endpoint6"]["host"], "2606:4700::1111");
    let nsk = lookup_served(served, &address).expect("re-served record verifies");
    assert_eq!(
        nsk.inet_socket_address(),
        Some("203.0.113.9:4433".parse::<SocketAddr>().unwrap())
    );
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
