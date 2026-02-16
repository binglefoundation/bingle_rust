#![cfg(not(target_os = "ios"))]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use crate::util::reusable_mock_api::{to_weak_api_both, InnerBingleApi, MockApiBoth};
use rust_comms::api::bingle_api::{BingleApi, NetworkEndpoint, ProgressCallback, StartOptions, UserId};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::BingleAccessUnsafeForTests;
use rust_comms::ddb::{DdbClient, DdbClientImpl};
use rust_comms::relay::relay_finder::RelayInfo;

#[path = "../../test_util.rs"]
mod test_util;

fn start_pair() -> (Arc<BingleApiImpl>, Arc<BingleApiImpl>, SocketAddr, SocketAddr) {
    let relay_port = test_util::find_unused_loopback_port();
    let client_port = test_util::find_unused_loopback_port();
    let relay_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), relay_port);
    let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), client_port);

    let relay = BingleApiImpl::new(&StartOptions::default());
    let client = BingleApiImpl::new(&StartOptions::default());

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
    relay.access_unsafe_for_tests(|r| r.start(&relay_opts)).expect("relay start ok");
    client.access_unsafe_for_tests(|c| c.start(&client_opts)).expect("client start ok");
    (relay, client, relay_addr, client_addr)
}

#[test]
fn ddb_client_register_relay_ok_and_persisted() {
    let (relay, client, relay_addr, _client_addr) = start_pair();

    // Proxy to use client's API methods for send-with-response against relay
    #[derive(Clone)]
    struct ApiProxy(Arc<BingleApiImpl>);
    impl InnerBingleApi for ApiProxy { 
        fn get_my_id(&self) -> Option<String> { self.0.get_my_id() }
        fn send_message_to_network_with_response(&self, nsk: &NetworkEndpoint, uid: &UserId, msg: serde_json::Value, progress: Option<Arc<ProgressCallback>>) -> Result<serde_json::Value, String> {
            self.0.send_message_to_network_with_response(nsk, uid, msg, progress)
        }
    }

    let client_shared = client.clone();
    let api_arc: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(ApiProxy(client_shared.clone()));

    // Discover closure returns the started relay
    let relay_id = relay.get_my_id().expect("relay id should be Some");
    let relay_id_for_discovery = relay_id.clone();
    let discover = Arc::new(move || vec![RelayInfo { id: relay_id_for_discovery.clone(), address: relay_addr, state: None }]);
    let cli = DdbClientImpl::with_discovery(to_weak_api_both(MockApiBoth::new_with_api_override(api_arc.clone())), discover);

    // Perform register_relay against the relay
    let res = cli.register_relay(relay_id.clone(), None);
    assert!(res.is_ok(), "register_relay should succeed: {:?}", res);

    // Verify via an explicit DDB QueryResolve request: advert.relayId should equal relay_id
    let client_id = api_arc.get_my_id().expect("client id should be Some");
    let q = rust_comms::messages::types::Message::Ddb(rust_comms::messages::types::DdbMessage::QueryResolve(
        rust_comms::messages::types::DdbQueryResolve { app: "ddb".to_string(), id: client_id.clone(), tag: None, response_tag: None, text: None, data: None }
    ));
    let json = rust_comms::messages::marshal::to_json_value(&q);
    let nsk = NetworkEndpoint::new_direct(relay_addr);
    let resp = api_arc.send_message_to_network_with_response(&nsk, &relay_id, json, None).expect("query response");

    assert_eq!(resp.get("type").and_then(|v: &serde_json::Value| v.as_str()), Some("queryResponse"));
    assert_eq!(resp.get("found").and_then(|v: &serde_json::Value| v.as_bool()), Some(true));
    let adv = resp.get("advert").and_then(|v| v.as_object()).expect("advert object");
    let relay_id_json = adv.get("relayId").and_then(|v: &serde_json::Value| v.as_str());
    assert_eq!(relay_id_json, Some(relay_id.as_str()));

    // Cleanup
    client_shared.access_unsafe_for_tests(|g| g.stop());
    relay.access_unsafe_for_tests(|rg| rg.stop());
}
