use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use rust_comms::engine::{Engine, EngineState};

// With new behavior: on_stun_consistent should do nothing if we have a public address, even if DTLS isn't started.
#[test]
fn engine_forced_stun_without_dtls_does_nothing() {
    let mut engine = Engine::new();
    let pub_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 55555);
    engine.test_force_stun_consistent(pub_addr);
    // State remains StunIdentify and last_public_addr is recorded
    assert_eq!(engine.state(), EngineState::StunIdentify);
    assert_eq!(engine.last_public_addr(), Some(pub_addr));
}
