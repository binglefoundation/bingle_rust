use crate::util::reusable_mock_api::MockApiBoth;
use rust_comms::api::bingle_api::{StartOptions};
use rust_comms::engine::{Engine, EngineState};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

// Updated behavior: on_stun_consistent marks endpoint available when a public address is provided,
// even if DTLS isn't started (no triangle test in minimal engine).
// Ignored until we have triangle test, which implies a rewrite of this test.
#[test]
#[ignore]
fn engine_forced_stun_sets_endpoint_available() {
    let mut engine = Engine::new(&StartOptions::default(), crate::util::reusable_mock_api::to_weak_api_both(MockApiBoth::new()));
    let pub_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 55555);
    engine.test_force_stun_consistent(pub_addr);
    assert_eq!(engine.state(), EngineState::EndpointAvailable);
    assert_eq!(engine.last_public_addr(), Some(pub_addr));
}
