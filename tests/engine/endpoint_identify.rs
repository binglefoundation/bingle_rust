use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use rust_comms::engine::{Engine, EngineState};

// This test drives the STUN-consistent path using a test-only helper rather than
// relying on live STUN servers. It validates that the engine records the public
// address and transitions to EndpointAvailable.
#[test]
fn engine_reaches_endpoint_available_and_saves_public_addr() {
    let mut engine = Engine::new();
    // Simulate that STUN determined our public endpoint on localhost port 55555
    let pub_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 55555);
    engine.test_force_stun_consistent(pub_addr);

    assert_eq!(engine.state(), EngineState::EndpointAvailable);
    assert_eq!(engine.last_public_addr(), Some(pub_addr));
}
