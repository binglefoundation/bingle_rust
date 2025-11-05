use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::EngineState;
use rust_comms::dtls::{NetworkMux, UdpNetworkMux};
use rust_comms::stun::{StunEndpointFinder, StunEndpointFinderImpl, StunState, SimpleStunServer, SimpleStunStartOptions};

#[path = "../test_util.rs"]
mod test_util;

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
    // let r1_port = find_unused_loopback_port();
    // let r2_port = find_unused_loopback_port();
    // assert_ne!(r1_port, 0);
    // assert_ne!(r2_port, 0);
    let r1_port = 12345;
    let r2_port = 12346;
    let relay1_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r1_port);
    let relay2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r2_port);

    // Print relay addresses for debugging
    println!("[Test] relay1_addr = {}", relay1_addr);
    println!("[Test] relay2_addr = {}", relay2_addr);

    let mut relay1 = BingleApiImpl::new();
    let mut relay2 = BingleApiImpl::new();

    let r1_opts = StartOptions { handle: "relay1".into(), algo_passphrase: Some(test_util::PASSPHRASE_SPEND.parse().unwrap()), static_ip: Some(relay1_addr), am_relay: true, stun_servers: None };
    let r2_opts = StartOptions { handle: "relay2".into(), algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.parse().unwrap()), static_ip: Some(relay2_addr), am_relay: true, stun_servers: None };

    // Start relays (no assertions about DTLS; we use them only as placeholders)
    let _ = relay1.start(r1_opts).expect("relay1 start() failed");
    let _ = relay2.start(r2_opts).expect("relay2 start() failed");

    // Start two local STUN servers we will use for consistency resolution
    let p1 = find_unused_loopback_port();
    let p2 = find_unused_loopback_port();
    let a1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p1);
    let a2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p2);

    let mut s1 = SimpleStunServer::start(SimpleStunStartOptions { bind_addr: a1, attach_to: None, broken_nat: false }).expect("start s1");
    let mut s2 = SimpleStunServer::start(SimpleStunStartOptions { bind_addr: a2, attach_to: None, broken_nat: false }).expect("start s2");

    // A client instance without staticEndpoint; provide the STUN server list to Engine.start
    let mut client1 = BingleApiImpl::new();

    let stun_list = vec![a1, a2];
    let c1_opts = StartOptions { handle: "client1".into(), algo_passphrase: Some(test_util::PASSPHRASE_10MIL.parse().unwrap()), static_ip: None, am_relay: false, stun_servers: Some(stun_list.clone()) };

    client1.start(c1_opts).expect("client1 start() failed");

    // Wait up to 10 seconds for client engine to enter EndpointAvailable
    let wait_start = Instant::now();
    while wait_start.elapsed() < Duration::from_secs(10) {
        match client1.engine_state_for_tests() {
            Some(EngineState::EndpointAvailable) => break,
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    // State is expected to be EndpointAvailable - do not change this!
    let s1_state = client1.engine_state_for_tests();
    assert!(matches!(s1_state, Some(EngineState::EndpointAvailable)  ), "unexpected client1 state: {:?}", s1_state);

    // Stop instances and STUN servers
    relay1.stop();
    relay2.stop();
    client1.stop();
    s1.stop();
    s2.stop();
}
