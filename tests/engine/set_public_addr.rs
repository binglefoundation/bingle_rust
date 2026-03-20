use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use rust_comms::api::bingle_api::StartOptions;
use rust_comms::engine::Engine;
use crate::util::reusable_mock_api::MockApiBoth;

#[cfg_attr(not(target_os = "ios"), test)]
pub fn engine_set_last_public_addr_updates_both_fields() {
    let api = MockApiBoth::new();
    let api_weak = crate::util::reusable_mock_api::to_weak_api_both(api);
    
    let mut opts = StartOptions::default();
    opts.am_relay = false;
    
    let mut engine = Engine::new(&opts, api_weak);
    
    let test_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 1234);
    
    // Initial state (should be None since opts.static_ip is None)
    assert_eq!(engine.last_public_addr(), None);
    
    // Use setter
    engine.set_last_public_addr(Some(test_addr));
    
    // Check both fields
    assert_eq!(engine.last_public_addr(), Some(test_addr));
    
    // Check shared field
    assert_eq!(engine.last_public_addr_shared_for_tests(), Some(test_addr));
    
    // Set to None
    engine.set_last_public_addr(None);
    assert_eq!(engine.last_public_addr(), None);
    assert_eq!(engine.last_public_addr_shared_for_tests(), None);
}
