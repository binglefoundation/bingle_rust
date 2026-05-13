use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
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
    
    // Set search interval very short for the test
    // 3 intervals of 100ms = 300ms
    finder.start(vec![a1, a2], 100, 1000);
    
    let start = Instant::now();
    let mut blocked_seen = false;
    while start.elapsed() < Duration::from_secs(2) {
        if seen_states.lock().unwrap().contains(&StunState::Blocked) {
            blocked_seen = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    
    finder.stop();
    
    assert!(blocked_seen, "Did not detect Blocked state after 3 intervals");
}
