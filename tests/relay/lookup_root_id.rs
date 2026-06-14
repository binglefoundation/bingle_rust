use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use crate::util::reusable_mock_api::{to_weak_api_both, InnerBingleApi, MockApiBoth};
use rust_comms::api::bingle_api::{NetworkEndpoint, ProgressCallback, UserId};
use rust_comms::relay::relay_finder::{RelayFinder, RelayInfo, RelayFinderTrait};

#[path = "../test_util.rs"]
pub mod test_util;

#[derive(Clone)]
struct MockApi;
impl InnerBingleApi for MockApi { 
    fn send_message_to_network_with_response(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> { Err(rust_comms::api::bingle_api::BingleError::Other("ni".into())) }
}

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

#[test]
#[cfg(not(target_os = "ios"))]
pub fn lookup_known_root_returns_endpoint() {
    let a1 = addr(45001);
    let a2 = addr(45002);

    let id1 = test_util::ADDRESS_SPEND.to_string();
    let id2 = test_util::ADDRESS_RECEIVE.to_string();

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(MockApi);

    let discover = {
        let roots = vec![RelayInfo::root(id1.clone(), a1), RelayInfo::root(id2.clone(), a2)];
        Arc::new(move || roots.clone())
    };

    let finder = RelayFinder::new(to_weak_api_both(MockApiBoth::new_with_api_override(api)), discover);

    // Known id should resolve to Some(NetworkEndpoint::Direct(addr))
    let nsk_opt = finder.lookup_root_id(&id1);
    assert!(nsk_opt.is_some(), "expected Some endpoint for known root id");
    let nsk = nsk_opt.unwrap();
    let direct = nsk.inet_socket_address();
    assert!(direct.is_some(), "expected direct inet socket address");
    assert_eq!(direct.unwrap(), a1);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn lookup_unknown_root_returns_none() {
    let a1 = addr(45011);
    let id1 = test_util::ADDRESS_SPEND.to_string();
    let unknown = test_util::ADDRESS_10MIL.to_string();

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(MockApi);
    let discover = {
        let roots = vec![RelayInfo::root(id1.clone(), a1)];
        Arc::new(move || roots.clone())
    };

    let finder = RelayFinder::new(to_weak_api_both(MockApiBoth::new_with_api_override(api)), discover);

    let nsk_opt = finder.lookup_root_id(&unknown);
    assert!(nsk_opt.is_none(), "expected None for unknown root id");
}
