/// Integration test: client starts with no relays available (NoConnection + StunIdentify),
/// then a relay becomes available and a second stun-consistent response triggers a successful
/// transition to TrianglePing.
use std::net::SocketAddr;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use rust_comms::api::bingle_api::StartOptions;
use rust_comms::engine::{Engine, EngineState, NatType};

fn make_options(handle: &str) -> StartOptions {
    StartOptions {
        handle: handle.to_string(),
        ..StartOptions::new("".into())
    }
}

/// Phase 1: no relays → NoConnection + StunIdentify.
/// Phase 2: relay available → transitions to TrianglePing (relay found successfully).
#[test]
fn test_no_relay_then_relay_becomes_available() {
    let options = make_options("retry_client");
    let mut eng = Engine::new(&options, crate::util::mock_bingle_api::mock_api_weak());

    let client_public_addr: SocketAddr = "203.0.113.10:4000".parse().expect("parse public addr");

    // Phase 1: no relays → should go to NoConnection + StunIdentify
    eng.test_stun_consistent_process_with_relays(client_public_addr, vec![]);

    assert_eq!(
        eng.nat_type(),
        NatType::NoConnection,
        "phase 1: nat_type should be NoConnection when no relay is available"
    );
    assert_eq!(
        eng.state(),
        EngineState::StunIdentify,
        "phase 1: state should be StunIdentify so the engine will retry on next stun response"
    );

    // Phase 2: a relay becomes available; simulate next stun-consistent response
    let relay_addr: SocketAddr = "10.0.0.1:7000".parse().expect("parse relay addr");
    let relay = crate::util::test_util::signed_root_relay("RELAYONE", relay_addr);
    eng.test_stun_consistent_process_with_relays(client_public_addr, vec![relay]);

    assert_eq!(
        eng.state(),
        EngineState::TrianglePing,
        "phase 2: state should be TrianglePing after relay becomes available"
    );
}

/// on_listening callback is called with false in phase 1 and not with true (no relay yet).
#[test]
fn test_no_relay_then_relay_on_listening_callback() {
    let options = make_options("retry_client_cb");

    let listening_false_count = Arc::new(AtomicU32::new(0));
    let listening_true_count = Arc::new(AtomicU32::new(0));

    let count_false = listening_false_count.clone();
    let count_true = listening_true_count.clone();

    let mut eng = Engine::new(&options, crate::util::mock_bingle_api::mock_api_weak());
    eng.set_on_listening_handler(Some(Arc::new(move |listening, _nat| {
        if listening {
            count_true.fetch_add(1, Ordering::SeqCst);
        } else {
            count_false.fetch_add(1, Ordering::SeqCst);
        }
    })));

    let client_public_addr: SocketAddr = "203.0.113.11:4001".parse().expect("parse public addr");

    // Phase 1: no relays → on_listening(false) called
    eng.test_stun_consistent_process_with_relays(client_public_addr, vec![]);

    assert_eq!(
        listening_false_count.load(Ordering::SeqCst),
        1,
        "phase 1: on_listening(false) should be called once"
    );
    assert_eq!(
        listening_true_count.load(Ordering::SeqCst),
        0,
        "phase 1: on_listening(true) should not be called"
    );

    // Phase 2: relay available → on_listening(true) should be called
    let relay_addr: SocketAddr = "10.0.0.2:7001".parse().expect("parse relay addr");
    let relay = crate::util::test_util::signed_root_relay("RELAYTWO", relay_addr);
    eng.test_stun_consistent_process_with_relays(client_public_addr, vec![relay]);

    assert_eq!(
        listening_false_count.load(Ordering::SeqCst),
        1,
        "phase 2: on_listening(false) should not be called again"
    );
    assert_eq!(
        listening_true_count.load(Ordering::SeqCst),
        1,
        "phase 2: on_listening(true) should be called once when relay becomes available"
    );
}
