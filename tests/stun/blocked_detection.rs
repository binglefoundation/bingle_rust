use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use rust_comms::stun::{StunEndpointFinder, StunEndpointFinderImpl, StunState};

#[test]
pub fn test_stun_blocked_detection() {
    let mut finder = StunEndpointFinderImpl::new();

    // Use some dummy addresses that won't respond
    let a1: SocketAddr = "127.0.0.1:12345".parse().unwrap();
    let a2: SocketAddr = "127.0.0.1:12346".parse().unwrap();

    let seen_states = Arc::new(Mutex::new(Vec::new()));
    let seen_states_clone = seen_states.clone();

    finder.set_state_change_handler(Some(Arc::new(move |st, _ep| {
        seen_states_clone.lock().unwrap().push(st);
    })));

    // search_interval_ms = 100 ms (1 tick)
    finder.start(vec![a1, a2], 100, 1000);
    finder.stop(); // Stop background thread for manual ticking

    // Tick 1: first poll (intervals_without_two becomes 1)
    finder.tick_for_test();
    // Tick 2: second poll (intervals_without_two becomes 2)
    finder.tick_for_test();
    // Tick 3: third poll (intervals_without_two becomes 3, and responders == 0, so state becomes Blocked)
    finder.tick_for_test();

    let seen = seen_states.lock().unwrap();
    assert!(seen.contains(&StunState::Blocked), "Did not detect Blocked state after 3 intervals. Seen states: {:?}", *seen);
}
