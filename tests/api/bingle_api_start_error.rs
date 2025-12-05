use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

// Ensure that BingleApiImpl::start does not ignore Engine start errors
// and propagates them to the caller.
#[test]
fn bingle_api_start_propagates_engine_error() {
    let mut api = BingleApiImpl::new();
    // No static_ip and empty STUN server list will cause Engine::start to error
    let opts = StartOptions {
        handle: "tester".into(),
        algo_passphrase: None,
        static_ip: None,
        am_relay: false,
        stun_servers: Some(vec![]),
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
    };

    let res = api.start(opts);
    assert!(res.is_err(), "expected start() to propagate Engine error");
    let msg = res.err().unwrap().to_lowercase();
    assert!(msg.contains("stun") || msg.contains("no stun") || msg.contains("no stun servers"),
        "unexpected error message: {}", msg);
}
