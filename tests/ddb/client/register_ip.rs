#![cfg(not(target_os = "ios"))]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use rust_comms::api::bingle_api::{BingleApi, NetworkEndpoint, StartOptions, Handle, UserId, ProgressCallback, OnMessageHandler, OnConnectHandler};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::ddb::{DdbClient, DdbClientImpl};
use rust_comms::relay::relay_finder::RelayInfo;

#[path = "../../test_util.rs"]
mod test_util;

fn start_pair() -> (Arc<std::sync::Mutex<BingleApiImpl>>, Arc<std::sync::Mutex<BingleApiImpl>>, SocketAddr, SocketAddr) {
    let relay_port = test_util::find_unused_loopback_port();
    let client_port = test_util::find_unused_loopback_port();
    let relay_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), relay_port);
    let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), client_port);

    let relay = Arc::new(std::sync::Mutex::new(BingleApiImpl::new(&StartOptions::default())));
    let client = Arc::new(std::sync::Mutex::new(BingleApiImpl::new(&StartOptions::default())));

    let relay_opts = StartOptions {
        handle: "relay".into(),
        algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.to_string()),
        static_ip: Some(relay_addr),
        am_relay: true,
        stun_servers: None,
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None,
    };
    let client_opts = StartOptions {
        handle: "client".into(),
        algo_passphrase: Some(test_util::PASSPHRASE_SPEND.to_string()),
        static_ip: Some(client_addr),
        am_relay: false,
        stun_servers: None,
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None,
    };
    relay.lock().unwrap().start(&relay_opts).expect("relay start ok");
    client.lock().unwrap().start(&client_opts).expect("client start ok");
    (relay, client, relay_addr, client_addr)
}

#[test]
fn ddb_client_register_ip_ok() {
    let (relay, client, relay_addr, client_addr) = start_pair();

    // Build a proxy API that delegates to the started client instance for the methods we need
    #[derive(Clone)]
    struct ApiProxy(Arc<std::sync::Mutex<BingleApiImpl>>);
    impl BingleApi for ApiProxy { 
    fn set_on_listening(&mut self, _handler: Option<std::sync::Arc<rust_comms::api::bingle_api::OnListeningHandler>>) {} 
    fn get_algo_provider_config(&self) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> { None } 
    fn get_handle(&self) -> Option<String> { None } 
    fn get_user_id(&self) -> Option<String> { None } 
    fn debug_print_options(&self) {}
        fn get_my_id(&self) -> Option<String> { self.0.lock().ok().and_then(|g| g.get_my_id()) }
        fn get_app_id(&self) -> Option<u64> { None }
        fn start(&mut self, _options: &StartOptions) -> Result<(), String> { Ok(()) }
        fn stop(&mut self) {}
        fn network_change(&mut self) {}
        fn send_message_to_id(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
        fn send_message_to_handle(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
        fn send_message_to_network(&self, _nsk: &NetworkEndpoint, _uid: &UserId, _msg: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> bool { false }
        fn send_message_to_id_with_response(&self, _user_id: &UserId, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
        fn send_message_to_handle_with_response(&self, _handle: &Handle, _message: serde_json::Value, _progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
        fn send_message_to_network_with_response(&self, nsk: &NetworkEndpoint, uid: &UserId, msg: serde_json::Value, progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> {
            self.0.lock().map_err(|_| "lock".to_string()).and_then(|g| g.send_message_to_network_with_response(nsk, uid, msg, progress))
        }
        fn set_on_message(&mut self, _handler: Option<Arc<OnMessageHandler>>) {}
        fn set_on_connect(&mut self, _handler: Option<Arc<OnConnectHandler>>) {}
    }

    let client_shared = client.clone();
    let api_arc: Arc<dyn BingleApi> = Arc::new(ApiProxy(client_shared.clone()));

    // Build a discovery closure that returns the started relay
    let relay_id = relay.lock().unwrap().get_my_id().expect("relay id");
    let discover = Arc::new(move || vec![RelayInfo { id: relay_id.clone(), address: relay_addr, state: None }]);
    let cli = DdbClientImpl::with_discovery(api_arc.clone(), discover);

    // Register the client's endpoint on the relay
    cli.registerIP(client_addr).expect("registerIP should succeed");

    // Verify via lookup using the same DDB client
    let id = api_arc.get_my_id().expect("client id");
    let nsk = cli.lookup(&id).expect("lookup should succeed");
    let ep = nsk.inet_socket_address().expect("direct endpoint");
    assert_eq!(ep, client_addr);

    // Cleanup
    if let Ok(mut g) = client_shared.lock() { g.stop(); }
    if let Ok(mut rg) = relay.lock() { rg.stop(); }
}
