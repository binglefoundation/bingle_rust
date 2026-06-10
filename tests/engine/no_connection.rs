use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use rust_comms::api::bingle_api::StartOptions;
use rust_comms::engine::{BingleAccessUnsafeForTests, Engine, EngineState, NatType};

#[test]
fn test_stun_blocked_sets_no_connection_nat_type() {
    let options = StartOptions {
        handle: "test_blocked".to_string(),
        ..StartOptions::new("".into())
    };
    let engine = Arc::new(Engine::new(&options, crate::util::mock_bingle_api::mock_api_weak()));

    engine.access_unsafe_for_tests(|e| {
        e.test_force_stun_blocked();
    });

    assert_eq!(engine.nat_type(), NatType::NoConnection);
}

#[test]
fn test_stun_blocked_calls_on_listening_false() {
    let options = StartOptions {
        handle: "test_blocked_cb".to_string(),
        ..StartOptions::new("".into())
    };

    let called_with_false = Arc::new(AtomicBool::new(false));
    let called_with_true = Arc::new(AtomicBool::new(false));

    let flag_false = called_with_false.clone();
    let flag_true = called_with_true.clone();

    let mut eng = Engine::new(&options, crate::util::mock_bingle_api::mock_api_weak());
    eng.set_on_listening_handler(Some(Arc::new(move |listening, _nat| {
        if listening {
            flag_true.store(true, Ordering::SeqCst);
        } else {
            flag_false.store(true, Ordering::SeqCst);
        }
    })));

    eng.test_force_stun_blocked();

    assert!(
        called_with_false.load(Ordering::SeqCst),
        "on_listening should be called with false when stun is blocked"
    );
    assert!(
        !called_with_true.load(Ordering::SeqCst),
        "on_listening should not be called with true when stun is blocked"
    );
}

#[test]
fn test_no_relay_target_sets_no_connection_nat_type() {
    let options = StartOptions {
        handle: "test_no_relay_target".to_string(),
        ..StartOptions::new("".into())
    };
    let engine = Arc::new(Engine::new(&options, crate::util::mock_bingle_api::mock_api_weak()));

    engine.access_unsafe_for_tests(|e| {
        e.test_stun_consistent_process_no_addr();
    });

    assert_eq!(
        engine.nat_type(),
        NatType::NoConnection,
        "nat_type should be NoConnection when no relay target is available"
    );
}

#[test]
fn test_no_relay_target_calls_on_listening_false() {
    let options = StartOptions {
        handle: "test_no_relay_cb".to_string(),
        ..StartOptions::new("".into())
    };

    let called_with_false = Arc::new(AtomicBool::new(false));
    let called_with_true = Arc::new(AtomicBool::new(false));

    let flag_false = called_with_false.clone();
    let flag_true = called_with_true.clone();

    let mut eng = Engine::new(&options, crate::util::mock_bingle_api::mock_api_weak());
    eng.set_on_listening_handler(Some(Arc::new(move |listening, _nat| {
        if listening {
            flag_true.store(true, Ordering::SeqCst);
        } else {
            flag_false.store(true, Ordering::SeqCst);
        }
    })));

    eng.test_stun_consistent_process_no_addr();

    assert!(
        called_with_false.load(Ordering::SeqCst),
        "on_listening should be called with false when no relay target"
    );
    assert!(
        !called_with_true.load(Ordering::SeqCst),
        "on_listening should not be called with true when no relay target"
    );
}

#[test]
fn test_stun_blocked_sets_stun_identify_state() {
    let options = StartOptions {
        handle: "test_blocked_state".to_string(),
        ..StartOptions::new("".into())
    };
    let mut eng = Engine::new(&options, crate::util::mock_bingle_api::mock_api_weak());

    eng.test_force_stun_blocked();

    assert_eq!(
        eng.state(),
        EngineState::StunIdentify,
        "state should be StunIdentify after stun blocked (to allow retry)"
    );
}

#[test]
fn test_no_relay_target_sets_stun_identify_state() {
    let options = StartOptions {
        handle: "test_no_relay_state".to_string(),
        ..StartOptions::new("".into())
    };
    let mut eng = Engine::new(&options, crate::util::mock_bingle_api::mock_api_weak());

    eng.test_stun_consistent_process_no_addr();

    assert_eq!(
        eng.state(),
        EngineState::StunIdentify,
        "state should be StunIdentify when no relay target (to allow retry)"
    );
}
