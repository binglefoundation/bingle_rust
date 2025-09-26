use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::EngineState;

// Option B integration test: use BingleApiImpl as the entry point, but mock out
// the discovery by forcing STUN consistent on the underlying Engine. We avoid
// a real Algorand localnet and real relays; instead, we start two relay instances
// (static endpoints) and two client instances, then validate that the clients reach
// EndpointAvailable with the expected public address.
#[test]
fn bingle_api_endpoint_identify_via_forced_stun() {
    // Set up two relay instances with static endpoints (127.0.0.1 on ephemeral ports)
    let relay1_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let relay2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

    let mut relay1 = BingleApiImpl::new();
    let mut relay2 = BingleApiImpl::new();

    let r1_opts = StartOptions { handle: "relay1".into(), algo_passphrase: Some("pass1".into()), static_ip: Some(relay1_addr), am_relay: true, stun_servers: None };
    let r2_opts = StartOptions { handle: "relay2".into(), algo_passphrase: Some("pass2".into()), static_ip: Some(relay2_addr), am_relay: true, stun_servers: None };

    // Start relays (no assertions about DTLS; we use them only as placeholders)
    let _ = relay1.start(r1_opts);
    let _ = relay2.start(r2_opts);

    // Two client instances without staticEndpoint; provide a dummy STUN server list to satisfy Engine.start
    let mut client1 = BingleApiImpl::new();
    let mut client2 = BingleApiImpl::new();

    let dummy_stun = vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3478)];
    let c1_opts = StartOptions { handle: "client1".into(), algo_passphrase: Some("pass3".into()), static_ip: None, am_relay: false, stun_servers: Some(dummy_stun.clone()) };
    let c2_opts = StartOptions { handle: "client2".into(), algo_passphrase: Some("pass4".into()), static_ip: None, am_relay: false, stun_servers: Some(dummy_stun.clone()) };

    let _ = client1.start(c1_opts);
    let _ = client2.start(c2_opts);

    // Force STUN Consistent with known loopback ports to emulate the endpoint determination result
    let pub1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 55551);
    let pub2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 55552);
    client1.engine_force_stun_consistent_for_tests(pub1);
    client2.engine_force_stun_consistent_for_tests(pub2);

    // Validate state and recorded public addresses
    assert_eq!(client1.engine_state_for_tests(), Some(EngineState::EndpointAvailable));
    assert_eq!(client1.engine_last_public_addr_for_tests(), Some(pub1));

    assert_eq!(client2.engine_state_for_tests(), Some(EngineState::EndpointAvailable));
    assert_eq!(client2.engine_last_public_addr_for_tests(), Some(pub2));

    // Stop instances
    relay1.stop();
    relay2.stop();
    client1.stop();
    client2.stop();
}
