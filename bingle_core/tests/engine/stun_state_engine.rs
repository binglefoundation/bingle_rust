use bingle_core::api::bingle_api::StartOptions;
use bingle_core::engine::{Engine, EngineState, NatType};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn test_engine_handles_stun_inconsistent() {
    // on_stun_inconsistent now spawns a thread that attempts relay registration.
    // Without an app_id or relay finder, it logs a warning and returns without
    // changing nat_type or state. The synchronous variant (test_force_stun_inconsistent_sync)
    // is used here so the worker completes before assertions run.
    let options = StartOptions {
        handle: "test".to_string(),
        ..StartOptions::new("".into())
    };

    let engine = Arc::new(Engine::new(
        &options,
        crate::util::mock_bingle_api::mock_api_weak(),
    ));

    use bingle_core::engine::BingleAccessUnsafeForTests;

    engine.access_unsafe_for_tests(|e| {
        e.test_force_stun_inconsistent_sync();
    });

    // Without app_id or relay finder, registration fails gracefully.
    // nat_type remains Unknown (not Symmetric — that was the old pre-relay-registration behavior).
    assert_eq!(engine.nat_type(), NatType::Unknown);
    // state stays at StunIdentify (default initial state — no relay found to register with).
    assert_eq!(engine.state(), EngineState::StunIdentify);
}

#[test]
fn test_engine_handles_stun_blocked() {
    let options = StartOptions {
        handle: "test".to_string(),
        ..StartOptions::new("".into())
    };

    let engine = Arc::new(Engine::new(
        &options,
        crate::util::mock_bingle_api::mock_api_weak(),
    ));

    use bingle_core::engine::BingleAccessUnsafeForTests;

    engine.access_unsafe_for_tests(|e| {
        e.test_force_stun_blocked();
    });

    assert_eq!(engine.nat_type(), NatType::NoConnection);
    // Should stay in StunIdentify
    assert_eq!(engine.state(), EngineState::StunIdentify);
}

#[test]
fn test_engine_handles_stun_none_sets_unknown_and_calls_on_listening_false() {
    let options = StartOptions {
        handle: "test".to_string(),
        ..StartOptions::new("".into())
    };

    let engine = Arc::new(Engine::new(
        &options,
        crate::util::mock_bingle_api::mock_api_weak(),
    ));

    let called_false = Arc::new(AtomicBool::new(false));
    let called_true = Arc::new(AtomicBool::new(false));
    let flag_false = called_false.clone();
    let flag_true = called_true.clone();

    use bingle_core::engine::BingleAccessUnsafeForTests;
    engine.access_unsafe_for_tests(|e| {
        e.set_on_listening_handler(Some(Arc::new(move |listening, _nat: NatType| {
            if listening {
                flag_true.store(true, Ordering::SeqCst);
            } else {
                flag_false.store(true, Ordering::SeqCst);
            }
        })));
    });

    // Simulate having previously registered (registered flag = true).
    engine.access_unsafe_for_tests(|e| {
        e.set_state_internal(EngineState::Registered);
    });
    // Confirm registered before the transition.
    assert_eq!(engine.state(), EngineState::Registered);

    engine.access_unsafe_for_tests(|e| {
        e.test_force_stun_none();
    });

    assert_eq!(
        engine.nat_type(),
        NatType::Unknown,
        "nat_type should be Unknown after on_stun_none"
    );
    assert_eq!(
        engine.state(),
        EngineState::StunIdentify,
        "state should be StunIdentify after on_stun_none"
    );
    assert!(
        called_false.load(Ordering::SeqCst),
        "on_listening(false) should have been called"
    );
    assert!(
        !called_true.load(Ordering::SeqCst),
        "on_listening(true) should NOT have been called"
    );
}
