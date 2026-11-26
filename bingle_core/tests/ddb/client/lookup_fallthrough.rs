// Issue #516: a lookup that gets `found: false` from one relay asks the other relays before it
// reports the recipient as not advertised.

use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth, to_weak_api_both};
use crate::util::test_util;
use bingle_core::api::bingle_api::{BingleError, NetworkEndpoint, SendFailureKind};
use bingle_core::ddb::{AdvertRecord, DdbClient, DdbClientImpl, InetSocketAddress};

const RELAY_A: &str = test_util::ADDRESS_RECEIVE;
const RELAY_B: &str = test_util::ADDRESS_SPEND;

fn relay_a_addr() -> SocketAddr {
    "127.0.0.1:1111".parse().unwrap()
}

fn relay_b_addr() -> SocketAddr {
    "127.0.0.2:2222".parse().unwrap()
}

/// How each queried relay answers a `queryResolve`.
#[derive(Clone, Copy)]
enum Answer {
    /// The first relay asked has no record; later ones have it.
    FirstMisses,
    /// No relay has the record.
    NoneHaveIt,
    /// The first relay asked has no record; later ones can't be reached.
    FirstMissesOthersUnreachable,
}

struct TwoRelays {
    answer: Answer,
    advert: Value,
    queried: Mutex<Vec<SocketAddr>>,
}

impl InnerBingleApi for TwoRelays {
    fn get_my_id(&self) -> Option<String> {
        Some("client-id".to_string())
    }

    fn send_message_to_network_with_response(
        &self,
        nsk: &NetworkEndpoint,
        _uid: &bingle_core::api::bingle_api::UserId,
        msg: Value,
        _progress: Option<Arc<bingle_core::api::bingle_api::ProgressCallback>>,
    ) -> Result<Value, BingleError> {
        let ty = msg.get("type").and_then(|v| v.as_str());
        if ty == Some("getRelaysStatus") {
            return Ok(json!({
                "app": "ddb",
                "type": "relaysStatusResponse",
                "responderState": "available",
                "epochId": 0,
                "treeOrder": 0,
                "relayIds": [RELAY_A, RELAY_B],
                "relayStates": ["available", "available"],
                "relayEndpoints": [
                    { "host": "127.0.0.1", "port": 1111 },
                    { "host": "127.0.0.2", "port": 2222 }
                ]
            }));
        }
        assert_eq!(ty, Some("queryResolve"), "unexpected message {msg}");
        let addr = nsk.inet_socket_address().expect("direct relay endpoint");
        let first = {
            let mut q = self.queried.lock().unwrap();
            q.push(addr);
            q.len() == 1
        };
        let found = match self.answer {
            Answer::FirstMisses => !first,
            Answer::NoneHaveIt => false,
            Answer::FirstMissesOthersUnreachable if first => false,
            Answer::FirstMissesOthersUnreachable => {
                return Err(BingleError::Other("relay unreachable".to_string()));
            }
        };
        if found {
            Ok(json!({"app": "ddb", "type": "queryResponse", "found": true, "advert": self.advert}))
        } else {
            Ok(json!({"app": "ddb", "type": "queryResponse", "found": false}))
        }
    }
}

/// A signed advert for a fresh recipient, and that recipient's id.
fn recipient_advert() -> (Value, String, SocketAddr) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let pk: [u8; 32] = signing_key.verifying_key().to_bytes();
    let id = algo_ops::byte_key_to_address(&pk).unwrap();
    let endpoint: SocketAddr = "198.51.100.7:4433".parse().unwrap();
    let advert = AdvertRecord::new(
        id.clone(),
        Some(InetSocketAddress::from(endpoint)),
        None,
        None,
        None,
        "2026-01-01T00:00:00Z".into(),
        &signing_key,
    );
    (serde_json::to_value(&advert).unwrap(), id, endpoint)
}

fn lookup_with(
    answer: Answer,
) -> (
    Result<NetworkEndpoint, BingleError>,
    Vec<SocketAddr>,
    SocketAddr,
) {
    let (advert, id, endpoint) = recipient_advert();
    let api = Arc::new(TwoRelays {
        answer,
        advert,
        queried: Mutex::new(Vec::new()),
    });
    let discover = Arc::new(|| {
        vec![
            test_util::signed_root_relay(RELAY_A, relay_a_addr()),
            test_util::signed_root_relay(RELAY_B, relay_b_addr()),
        ]
    });
    let cli = DdbClientImpl::with_discovery(
        to_weak_api_both(MockApiBoth::new_with_api_override(api.clone())),
        discover,
    );
    let result = cli.lookup(&id);
    let queried = api.queried.lock().unwrap().clone();
    (result, queried, endpoint)
}

#[test]
#[cfg(not(target_os = "ios"))]
fn lookup_falls_through_to_a_second_relay() {
    let (result, queried, endpoint) = lookup_with(Answer::FirstMisses);
    let nsk = result.expect("the second relay has the record");
    assert_eq!(nsk.inet_socket_address(), Some(endpoint));
    assert_eq!(queried.len(), 2, "asked two relays: {queried:?}");
    assert_ne!(
        queried[0], queried[1],
        "the second query went to the other relay"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn lookup_reports_not_advertised_when_no_relay_has_the_record() {
    let (result, queried, _) = lookup_with(Answer::NoneHaveIt);
    match result {
        Err(BingleError::Send {
            kind: SendFailureKind::RecipientNotAdvertised,
            ..
        }) => {}
        other => panic!("expected RecipientNotAdvertised, got {other:?}"),
    }
    assert_eq!(queried.len(), 2, "asked every relay once: {queried:?}");
    assert_ne!(queried[0], queried[1]);
}

/// A relay that can't be reached after a definite `found: false` leaves the answer as not
/// advertised, not as no relay available.
#[test]
#[cfg(not(target_os = "ios"))]
fn lookup_keeps_not_advertised_when_the_other_relay_is_unreachable() {
    let (result, queried, _) = lookup_with(Answer::FirstMissesOthersUnreachable);
    match result {
        Err(BingleError::Send {
            kind: SendFailureKind::RecipientNotAdvertised,
            ..
        }) => {}
        other => panic!("expected RecipientNotAdvertised, got {other:?}"),
    }
    assert_eq!(queried.len(), 2, "{queried:?}");
}
