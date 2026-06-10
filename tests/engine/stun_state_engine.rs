use std::sync::Arc;
use rust_comms::api::bingle_api::StartOptions;
use rust_comms::engine::{Engine, EngineState, NatType};

#[test]
fn test_engine_handles_stun_inconsistent() {
    let options = StartOptions {
        handle: "test".to_string(),
        ..StartOptions::new("".into())
    };
    
    let engine = Arc::new(Engine::new(&options, crate::util::mock_bingle_api::mock_api_weak()));
    
    use rust_comms::engine::BingleAccessUnsafeForTests;
    
    engine.access_unsafe_for_tests(|e| {
        e.test_force_stun_inconsistent();
    });
    
    assert_eq!(engine.nat_type(), NatType::Symmetric);
    assert_eq!(engine.state(), EngineState::NATRestricted);
}

#[test]
fn test_engine_handles_stun_blocked() {
    let options = StartOptions {
        handle: "test".to_string(),
        ..StartOptions::new("".into())
    };
    
    let engine = Arc::new(Engine::new(&options, crate::util::mock_bingle_api::mock_api_weak()));
    
    use rust_comms::engine::BingleAccessUnsafeForTests;
    
    engine.access_unsafe_for_tests(|e| {
        e.test_force_stun_blocked();
    });
    
    assert_eq!(engine.nat_type(), NatType::NoConnection);
    // Should stay in StunIdentify
    assert_eq!(engine.state(), EngineState::StunIdentify);
}
