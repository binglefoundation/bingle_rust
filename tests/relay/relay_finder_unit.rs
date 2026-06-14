use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use rust_comms::api::bingle_api::{NetworkEndpoint, ProgressCallback, UserId};
use rust_comms::ddb::InetSocketAddress;
use rust_comms::engine::RelayState;
use rust_comms::messages::marshal::to_json_value;
use rust_comms::messages::types::{DdbMessage, DdbRelaysStatusResponse, Message};

// Minimal mock API that responds to GetRelaysStatus with both test relays
struct MockApi;
impl InnerBingleApi for MockApi {
    fn send_message_to_network_with_response(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> {
        let ty = message.get("type").and_then(|v: &serde_json::Value| v.as_str()).unwrap_or("");
        let app = message.get("app");
        if ty == "getRelaysStatus" && app.and_then(|v: &serde_json::Value| v.as_str()) == Some("ddb") {
            // Respond to DdbGetRelaysStatus with both test relays; ADDRESS_RECEIVE is Available
            let response = DdbRelaysStatusResponse {
                app: "ddb".to_string(),
                responder_state: RelayState::Available,
                epoch_id: 1,
                tree_order: 2,
                relay_ids: vec![
                    test_util::ADDRESS_SPEND.to_string(),
                    test_util::ADDRESS_RECEIVE.to_string(),
                ],
                relay_endpoints: Some(vec![
                    InetSocketAddress::from(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345)),
                    InetSocketAddress::from(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12346)),
                ]),
                relay_states: vec![RelayState::Own, RelayState::Available],
                response_tag: None,
                text: None,
                data: None,
            };
            Ok(to_json_value(&Message::Ddb(DdbMessage::RelaysStatusResponse(response))))
        } else {
            Err(rust_comms::api::bingle_api::BingleError::Other("unexpected message".into()))
        }
    }
}

use crate::util::reusable_mock_api::{to_weak_api_both, InnerBingleApi, MockApiBoth};
use rust_comms::relay::relay_finder::{RelayFinder, RelayInfo, RelayFinderTestTrait};

#[path = "../test_util.rs"]
pub mod test_util;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn find_root_relay_rejects_self() {
    let discover = Arc::new(|| -> Vec<RelayInfo> {
        vec![
            RelayInfo::root(
                test_util::ADDRESS_SPEND.to_string(),
                SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345),
            ),
            RelayInfo::root(
                test_util::ADDRESS_RECEIVE.to_string(),
                SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12346),
            ),
        ]
    });
    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(MockApi);
    let finder = RelayFinder::new(to_weak_api_both(MockApiBoth::new_with_api_override(api)), discover);
    // my_id is ADDRESS_SPEND, ensure we do not select ourselves and get ADDRESS_RECEIVE instead
    let res = finder.find_root_relay(test_util::ADDRESS_SPEND);
    assert!(res.is_ok(), "should find other relay");
    let info = res.unwrap();
    assert_eq!(info.id, test_util::ADDRESS_RECEIVE);
    assert_eq!(info.address, SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12346));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn select_indices_partitions_for_multiple_ids() {
    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(MockApi);
    let discover = Arc::new(|| -> Vec<RelayInfo> { Vec::new() });
    let finder = RelayFinder::new(
        to_weak_api_both(MockApiBoth::new_with_api_override(api)),
        discover,
    );

    let relays = vec![
        RelayInfo::root("R1", SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 10001)),
        RelayInfo::root("R2", SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 10002)),
        RelayInfo::root("R3", SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 10003)),
        RelayInfo::root("R4", SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 10004)),
    ];

    let ids = [
        "AALHSDXNRPARCE7OYMOIQDEEGKZA3QMVN3J2ONBTVHS66EBNACAQ4EKXRM",
        "IAOSUGCPN6WTPI3LCXLHXMJU3UT3VIGP3CKZ6H3P6XYZND4JYKZJSFYZ3I",
        "QASXBML72DKIJEJ5GLMEBBX33KCKW3TSJW7ETFOTLEREQCDMW5BXCLXSQU",
        "YA2UAJPUJZBY4KR2B4FBM57NSA7252PJQTVKJEGB2MOISRUECW4JGE4USM",
        ];

    for (i, id) in ids.iter().enumerate() {
        let (idx, alt) = finder.select_indices(&relays, id);
        tracing::info!("[RelayFinder] select_indices: id={} idx={} alt={}", id, idx, alt);
        assert_eq!(idx, i, "idx mismatch for id {}", id);
        assert_eq!(alt, (idx + 1) % 4, "alt mismatch for id {}", id);
    }
}
