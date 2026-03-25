

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::BingleAccessUnsafeForTests;

#[path = "../test_util.rs"]
pub mod test_util;

fn find_unused_loopback_port() -> u16 {
    let sock = UdpSocket::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).expect("bind temp socket");
    let port = sock.local_addr().expect("local addr").port();
    drop(sock);
    port
}

#[ntest::timeout(15_000)]
#[cfg_attr(not(target_os = "ios"), test)]
pub fn engine_binds_to_unspecified_ip_when_static_addr_is_provided() {
    // Choose a random available port by probing loopback; we use that port for static_ip.
    let port = find_unused_loopback_port();
    assert_ne!(port, 0, "probe should yield a non-zero port");

    // Provide a loopback static address; engine should still bind to 0.0.0.0:<port>.
    let static_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

    let api = BingleApiImpl::new(&StartOptions::default());

    let opts = StartOptions {
        handle: "bind-test".into(),
        algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.to_string()),
        static_ip: Some(static_addr),
        am_relay: false,
        stun_servers: None,
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None, handle_cache_expiry: None,
    };

    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts)).expect("api.start should succeed");

    // Retrieve the actual bound address and assert it's 0.0.0.0 with the same port
    let local = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.engine_local_bind_addr_for_tests());
    assert!(local.is_some(), "engine should expose local bind addr");
    let local = local.unwrap();
    assert_eq!(local.port(), port, "bound port should match requested static port");
    assert_eq!(local.ip(), IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), "engine test accessor should return 127.0.0.1 for unspecified bind");

    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.stop());
}
