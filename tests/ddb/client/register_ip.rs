

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use crate::util::reusable_mock_api::{to_weak_api_both, InnerBingleApi, MockApiBoth};
use rust_comms::api::bingle_api::{BingleApi, NetworkEndpoint, ProgressCallback, StartOptions, UserId};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::BingleAccessUnsafeForTests;
use rust_comms::ddb::{DdbClient, DdbClientImpl};
use rust_comms::relay::relay_finder::RelayInfo;

#[path = "../../test_util.rs"]
pub mod test_util;

fn start_pair() -> (Arc<BingleApiImpl>, Arc<BingleApiImpl>, SocketAddr, SocketAddr) {
    let relay_port = test_util::find_unused_loopback_port();
    let client_port = test_util::find_unused_loopback_port();
    let relay_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), relay_port);
    let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), client_port);
    
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
        log_level: None, handle_cache_expiry: None, dangerous_debug: true, log_mode: rust_comms::util::logging::LogMode::Plain,
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
        log_level: None, handle_cache_expiry: None, dangerous_debug: true, log_mode: rust_comms::util::logging::LogMode::Plain,
    };
    
    let relay = BingleApiImpl::new(&relay_opts);
    let client = BingleApiImpl::new(&client_opts);

    relay.set_id_to_handle_lookup_mock_for_tests(Box::new(|user_id| {
        if user_id.is_empty() {
            Ok(None)
        } else {
            Ok(Some("mock-client".to_string()))
        }
    }));
    client.set_id_to_handle_lookup_mock_for_tests(Box::new(|user_id| {
        if user_id.is_empty() {
            Ok(None)
        } else {
            Ok(Some("mock-relay".to_string()))
        }
    }));

    relay.access_unsafe_for_tests(|r| r.start(&relay_opts)).expect("relay start ok");
    if !test_util::wait_for_relay_available(&relay, std::time::Duration::from_secs(30)) {
        panic!("relay did not become Available within 30s");
    }
    client.access_unsafe_for_tests(|c| c.start(&client_opts)).expect("client start ok");
    (relay, client, relay_addr, client_addr)
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn ddb_client_register_ip_ok() {
    let (relay, client, relay_addr, client_addr) = start_pair();

    // Build a proxy API that delegates to the started client instance for the methods we need
    #[derive(Clone)]
    struct ApiProxy(Arc<BingleApiImpl>);
    impl InnerBingleApi for ApiProxy { 
        fn get_my_id(&self) -> Option<String> { self.0.get_my_id() }
        fn send_message_to_network_with_response(&self, nsk: &NetworkEndpoint, uid: &UserId, msg: serde_json::Value, progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> {
            self.0.send_message_to_network_with_response(nsk, uid, msg, progress)
        }
    }

    let client_shared = client.clone();
    let api_arc: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(ApiProxy(client_shared.clone()));

    // Build a discovery closure that returns the started relay
    let relay_id = relay.get_my_id().expect("relay id");
    let discover = Arc::new(move || vec![RelayInfo { id: relay_id.clone(), address: relay_addr, state: None, ttl: None }]);
    let cli = DdbClientImpl::with_discovery(to_weak_api_both(MockApiBoth::new_with_api_override(api_arc.clone())), discover);

    // Register the client's endpoint on the relay
    cli.registerIP(client_addr, false).expect("registerIP should succeed");

    // Verify via lookup using the same DDB client
    let id = api_arc.get_my_id().expect("client id");
    let nsk = cli.lookup(&id).expect("lookup should succeed");
    let ep = nsk.inet_socket_address().expect("direct endpoint");
    assert_eq!(ep, client_addr);

    // Cleanup
    client_shared.access_unsafe_for_tests(|g| g.stop());
    relay.access_unsafe_for_tests(|rg| rg.stop());
}
