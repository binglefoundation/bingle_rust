use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, OnConnectHandler, OnMessageHandler, ProgressCallback, StartOptions, UserId};
use rust_comms::relay::relay_finder::{RelayFinder, RelayInfo};

#[path = "../test_util.rs"]
mod test_util;

#[derive(Clone)]
struct MockApi;
impl BingleApi for MockApi { 
    fn set_on_listening(&mut self, _handler: Option<std::sync::Arc<rust_comms::api::bingle_api::OnListeningHandler>>) {} 
    fn get_algo_provider_config(&self) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> { None } 
    fn get_handle(&self) -> Option<String> { None } 
    fn get_user_id(&self) -> Option<String> { None }
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> { None }
    fn get_app_id(&self) -> Option<u64> { None }
    fn start(&mut self, _options: &StartOptions) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}

    fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }

    fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_network_with_response(&self, _nsk: &NetworkEndpoint, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }

    fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
}

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

#[test]
fn lookup_known_root_returns_endpoint() {
    let a1 = addr(45001);
    let a2 = addr(45002);

    let id1 = test_util::ADDRESS_SPEND.to_string();
    let id2 = test_util::ADDRESS_RECEIVE.to_string();

    let api: Arc<dyn BingleApi> = Arc::new(MockApi);

    let discover = {
        let roots = vec![
            RelayInfo { id: id1.clone(), address: a1, state: None },
            RelayInfo { id: id2.clone(), address: a2, state: None },
        ];
        Arc::new(move || roots.clone())
    };

    let finder = RelayFinder::new(api, std::time::Duration::from_millis(200), discover);

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

    let api: Arc<dyn BingleApi> = Arc::new(MockApi);
    let discover = {
        let roots = vec![RelayInfo { id: id1.clone(), address: a1, state: None }];
        Arc::new(move || roots.clone())
    };

    let finder = RelayFinder::new(api, std::time::Duration::from_millis(200), discover);

    let nsk_opt = finder.lookup_root_id(&unknown);
    assert!(nsk_opt.is_none(), "expected None for unknown root id");
}
