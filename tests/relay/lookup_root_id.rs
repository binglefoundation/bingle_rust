use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, OnConnectHandler, OnMessageHandler, ProgressCallback, StartOptions, UserId};
use rust_comms::relay::relay_finder::{RelayFinder, RelayInfo};
use crate::util::mock_api::{to_weak, InnerBingleApi, MockApiBoth};

#[path = "../test_util.rs"]
mod test_util;

#[derive(Clone)]
struct MockApi;
impl InnerBingleApi for MockApi { 
    fn send_message_to_network_with_response(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
}

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

#[test]
fn lookup_known_root_returns_endpoint() {
    let a1 = addr(45001);
    let a2 = addr(45002);

    let id1 = test_util::ADDRESS_SPEND.to_string();
    let id2 = test_util::ADDRESS_RECEIVE.to_string();

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(MockApi);

    let discover = {
        let roots = vec![
            RelayInfo { id: id1.clone(), address: a1, state: None },
            RelayInfo { id: id2.clone(), address: a2, state: None },
        ];
        Arc::new(move || roots.clone())
    };

    let finder = RelayFinder::new(to_weak(MockApiBoth::new_with_api_override(api)), std::time::Duration::from_millis(200), discover);

    // Known id should resolve to Some(NetworkEndpoint::Direct(addr))
    let nsk_opt = finder.lookup_root_id(&id1);
    assert!(nsk_opt.is_some(), "expected Some endpoint for known root id");
    let nsk = nsk_opt.unwrap();
    let direct = nsk.inet_socket_address();
    assert!(direct.is_some(), "expected direct inet socket address");
    assert_eq!(direct.unwrap(), a1);
}

#[test]
fn lookup_unknown_root_returns_none() {
    let a1 = addr(45011);
    let id1 = test_util::ADDRESS_SPEND.to_string();
    let unknown = test_util::ADDRESS_10MIL.to_string();

    let api: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(MockApi);
    let discover = {
        let roots = vec![RelayInfo { id: id1.clone(), address: a1, state: None }];
        Arc::new(move || roots.clone())
    };

    let finder = RelayFinder::new(to_weak(MockApiBoth::new_with_api_override(api)), std::time::Duration::from_millis(200), discover);

    let nsk_opt = finder.lookup_root_id(&unknown);
    assert!(nsk_opt.is_none(), "expected None for unknown root id");
}
