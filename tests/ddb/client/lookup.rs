#![cfg(not(target_os = "ios"))]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use crate::util::reusable_mock_api::{to_weak_api_both, InnerBingleApi, MockApiBoth};
use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::BingleAccessUnsafeForTests;
use rust_comms::ddb::{DdbClient, DdbClientImpl};
use rust_comms::relay::relay_finder::RelayInfo;

#[path = "../../test_util.rs"]
mod test_util;

#[test]
fn ddb_client_lookup_returns_endpoint() {
    // Start relay and client
    let relay_port = test_util::find_unused_loopback_port();
    let client_port = test_util::find_unused_loopback_port();
    let relay_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), relay_port);
    let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), client_port);

    let relay = BingleApiImpl::new(&StartOptions::default());
    let client = BingleApiImpl::new(&StartOptions::default());

    let relay_opts = StartOptions { handle: "relay".into(), algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.to_string()), static_ip: Some(relay_addr), am_relay: true, stun_servers: None, algo_provider_config: None, algo_network: None, app_id: None, asset_id: None, log_level: None };
    let client_opts = StartOptions { handle: "client".into(), algo_passphrase: Some(test_util::PASSPHRASE_SPEND.to_string()), static_ip: Some(client_addr), am_relay: false, stun_servers: None, algo_provider_config: None, algo_network: None, app_id: None, asset_id: None, log_level: None };

    relay.access_unsafe_for_tests(|r| r.start(&relay_opts)).expect("relay start ok");
    client.access_unsafe_for_tests(|c| c.start(&client_opts)).expect("client start ok");

    // Use a DdbClientImpl with custom discovery that points to the relay
    #[derive(Clone)]
    struct ApiProxy(Arc<BingleApiImpl>);
    impl InnerBingleApi for ApiProxy { 
        fn get_my_id(&self) -> Option<String> { self.0.get_my_id() }
        fn send_message_to_network_with_response(&self, nsk: &rust_comms::api::bingle_api::NetworkEndpoint, uid: &rust_comms::api::bingle_api::UserId, msg: serde_json::Value, progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { self.0.send_message_to_network_with_response(nsk, uid, msg, progress) }
    }

    let client_shared = client.clone();
    let api_arc: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(ApiProxy(client_shared.clone()));
    let relay_id = relay.get_my_id().expect("relay id");
    let discover = Arc::new(move || vec![RelayInfo { id: relay_id.clone(), address: relay_addr, state: None }]);
    let cli = DdbClientImpl::with_discovery(to_weak_api_both(MockApiBoth::new_with_api_override(api_arc.clone())), discover);

    // First register our IP so the relay has an advert
    cli.registerIP(client_addr, false).expect("registerIP should succeed");

    // Now lookup our id
    let id = api_arc.get_my_id().expect("client id");
    let nsk = cli.lookup(&id).expect("lookup should return a NetworkSourceKey");

    // Ensure the network source key carries the direct address we registered
    let got = nsk.inet_socket_address().expect("inet_socket_address should be Some");
    assert_eq!(got, client_addr);

    relay.access_unsafe_for_tests(|r| r.stop());
    client_shared.access_unsafe_for_tests(|c| c.stop());
}
