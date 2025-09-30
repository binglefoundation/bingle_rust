use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::EngineState;

fn find_unused_loopback_port() -> u16 {
    // Bind to 127.0.0.1:0 to let OS choose a free port, then return that port.
    // Drop the socket to free it for the test process to rebind shortly after.
    let sock = UdpSocket::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).expect("bind temp socket");
    let port = sock.local_addr().expect("local addr").port();
    drop(sock);
    port
}

// Option B integration test: use BingleApiImpl as the entry point, but mock out
// the discovery by forcing STUN consistent on the underlying Engine. We avoid
// a real Algorand localnet and real relays; instead, we start two relay instances
// (static endpoints) and two client instances, then validate that the clients reach
// EndpointAvailable with the expected public address.
#[test]
fn bingle_api_endpoint_identify_via_forced_stun() {
    // Set up two relay instances with static endpoints (127.0.0.1 with known, unused ports)
    let r1_port = find_unused_loopback_port();
    let r2_port = find_unused_loopback_port();
    assert_ne!(r1_port, 0);
    assert_ne!(r2_port, 0);
    let relay1_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r1_port);
    let relay2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r2_port);

    let mut relay1 = BingleApiImpl::new();
    let mut relay2 = BingleApiImpl::new();

    let r1_opts = StartOptions { handle: "relay1".into(), algo_passphrase: Some("b64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()), static_ip: Some(relay1_addr), am_relay: true, stun_servers: None };
    let r2_opts = StartOptions { handle: "relay2".into(), algo_passphrase: Some("b64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()), static_ip: Some(relay2_addr), am_relay: true, stun_servers: None };

    // Start relays (no assertions about DTLS; we use them only as placeholders)
    let _ = relay1.start(r1_opts);
    let _ = relay2.start(r2_opts);

    // Two client instances without staticEndpoint; provide a dummy STUN server list to satisfy Engine.start
    let mut client1 = BingleApiImpl::new();
    let mut client2 = BingleApiImpl::new();

    let dummy_stun = vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3478)];
    let c1_opts = StartOptions { handle: "client1".into(), algo_passphrase: Some("b64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()), static_ip: None, am_relay: false, stun_servers: Some(dummy_stun.clone()) };
    let c2_opts = StartOptions { handle: "client2".into(), algo_passphrase: Some("b64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()), static_ip: None, am_relay: false, stun_servers: Some(dummy_stun.clone()) };

    client1.start(c1_opts).expect("client1 start() failed");
    client2.start(c2_opts).expect("client2 start() failed");

    // Force STUN Consistent with known loopback ports to emulate the endpoint determination result
    // TODO: instead emulate a STUN server
    let pub1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 55551);
    let pub2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 55552);
    client1.engine_force_stun_consistent_for_tests(pub1);
    client2.engine_force_stun_consistent_for_tests(pub2);

    // Validate state and recorded public addresses
    // As of change: on_stun_consistent does nothing further if a public address is provided.
    // So state remains StunIdentify, but last_public_addr is recorded.
    assert_eq!(client1.engine_state_for_tests(), Some(EngineState::StunIdentify));
    assert_eq!(client1.engine_last_public_addr_for_tests(), Some(pub1));

    assert_eq!(client2.engine_state_for_tests(), Some(EngineState::StunIdentify));
    assert_eq!(client2.engine_last_public_addr_for_tests(), Some(pub2));

    // Stop instances
    relay1.stop();
    relay2.stop();
    client1.stop();
    client2.stop();
}
