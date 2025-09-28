use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use rust_comms::engine::{Engine, EngineState};

// This test previously assumed endpoint becomes available when DTLS isn't started.
// New behavior: this should panic because we cannot proceed to triangle ping without DTLS.
#[test]
#[should_panic(expected = "DTLS not started")]
fn engine_forced_stun_without_dtls_panics() {
    let mut engine = Engine::new();
    let pub_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 55555);
    engine.test_force_stun_consistent(pub_addr);
}
