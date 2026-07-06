use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::util::reusable_mock_api::{InnerBingleApi, MockApiBoth, to_weak_api_both};
use rust_comms::api::bingle_api::{
    BingleApi, BingleApiInternal, NetworkEndpoint, ProgressCallback, StartOptions, UserId,
};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::ddb::{DdbClient, DdbClientImpl};
use rust_comms::engine::BingleAccessUnsafeForTests;

#[path = "../../test_util.rs"]
pub mod test_util;

fn start_pair() -> (
    Arc<BingleApiImpl>,
    Arc<BingleApiImpl>,
    SocketAddr,
    SocketAddr,
) {
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
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: true,
        log_mode: rust_comms::util::logging::LogMode::Plain,
        wait_response_timeout: None,
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
        handle_cache_expiry: None,
        dangerous_debug: true,
        log_mode: rust_comms::util::logging::LogMode::Plain,
        wait_response_timeout: None,
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

    relay
        .access_unsafe_for_tests(|r| r.start(&relay_opts))
        .expect("relay start ok");
    if !test_util::wait_for_relay_available(&relay, Duration::from_secs(30)) {
        panic!("relay did not become Available within 30s");
    }
    client
        .access_unsafe_for_tests(|c| c.start(&client_opts))
        .expect("client start ok");
    (relay, client, relay_addr, client_addr)
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ddb_client_signoff_removes_entry() {
    let (relay, client, relay_addr, client_addr) = start_pair();

    // Proxy exposing the client's real send methods and identity to the DDB client.
    #[derive(Clone)]
    struct ApiProxy(Arc<BingleApiImpl>);
    impl InnerBingleApi for ApiProxy {
        fn get_my_id(&self) -> Option<String> {
            self.0.get_my_id()
        }
        fn get_signing_key(&self) -> Option<ed25519_dalek::SigningKey> {
            self.0.access_unsafe_for_tests(|a| a.get_signing_key())
        }
        fn send_message_to_network(
            &self,
            nsk: &NetworkEndpoint,
            uid: &UserId,
            msg: serde_json::Value,
            progress: Option<Arc<ProgressCallback>>,
        ) -> Result<bool, rust_comms::api::bingle_api::BingleError> {
            self.0.send_message_to_network(nsk, uid, msg, progress)
        }
        fn send_message_to_network_with_response(
            &self,
            nsk: &NetworkEndpoint,
            uid: &UserId,
            msg: serde_json::Value,
            progress: Option<Arc<ProgressCallback>>,
        ) -> Result<serde_json::Value, rust_comms::api::bingle_api::BingleError> {
            self.0
                .send_message_to_network_with_response(nsk, uid, msg, progress)
        }
    }

    let client_shared = client.clone();
    let api_arc: Arc<dyn InnerBingleApi + Send + Sync> = Arc::new(ApiProxy(client_shared.clone()));

    let relay_id = relay.get_my_id().expect("relay id");
    let relay_id_for_discovery = relay_id.clone();
    let discover = Arc::new(move || {
        vec![test_util::signed_root_relay(
            &relay_id_for_discovery,
            relay_addr,
        )]
    });
    let cli = DdbClientImpl::with_discovery(
        to_weak_api_both(MockApiBoth::new_with_api_override(api_arc.clone())),
        discover,
    );

    // Register the client's endpoint so a DDB entry exists on the relay.
    cli.registerIP(client_addr, false)
        .expect("registerIP should succeed");
    let client_id = api_arc.get_my_id().expect("client id");
    let nsk = cli
        .lookup(&client_id)
        .expect("lookup should find the freshly registered entry");
    assert_eq!(
        nsk.inet_socket_address().expect("direct endpoint"),
        client_addr
    );

    // Sign off; the relay processes the removal asynchronously.
    cli.signoff().expect("signoff should succeed");

    // Poll until the entry is gone from the relay's DDB.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut gone = false;
    while Instant::now() < deadline {
        if cli.lookup(&client_id).is_err() {
            gone = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(gone, "client's DDB entry should be removed after signoff");

    // Cleanup
    client_shared.access_unsafe_for_tests(|g| g.stop());
    relay.access_unsafe_for_tests(|rg| rg.stop());
}
