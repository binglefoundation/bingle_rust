use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicUsize, Ordering as AOrdering};

use rust_comms::stun::{StunEndpointFinder, StunState, StunEndpointFinderImpl};
use crate::util::test_util::init_test_logging;

fn make_xor_mapped_response(ip: [u8; 4], port: u16) -> Vec<u8> {
    let mut pkt = vec![0u8; 20];
    pkt[0] = 0x01; pkt[1] = 0x01;
    pkt[2] = 0x00; pkt[3] = 0x0c;
    pkt[4] = 0x21; pkt[5] = 0x12; pkt[6] = 0xA4; pkt[7] = 0x42;
    for i in 0..12 { pkt[8 + i] = i as u8; }
    pkt.extend_from_slice(&[0x00, 0x20, 0x00, 0x08]);
    pkt.push(0x00);
    pkt.push(0x01);
    let xport = port ^ 0x2112;
    pkt.extend_from_slice(&xport.to_be_bytes());
    let mut xaddr = ip;
    let cookie = [0x21u8, 0x12, 0xA4, 0x42];
    for i in 0..4 { xaddr[i] ^= cookie[i]; }
    pkt.extend_from_slice(&xaddr);
    pkt
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn state_transitions_consistent_and_inconsistent() {
    let mut finder = StunEndpointFinderImpl::new();
    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();
    finder.start(vec![s1, s2], 50, 50);

    let changes = Arc::new(Mutex::new(Vec::<(StunState, Option<SocketAddr>)>::new()));
    let changes_clone = changes.clone();
    finder.set_state_change_handler(Some(Arc::new(move |st, ep| {
        changes_clone.lock().unwrap().push((st, ep));
    })));

    // First response: SINGLE
    let r1 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s1, &r1);

    // Second response from another server, same endpoint -> CONSISTENT
    let r2 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s2, &r2);

    // Now different endpoint from s2 -> INCONSISTENT
    let r3 = make_xor_mapped_response([203, 0, 113, 10], 55001);
    finder.process_packet(s2, &r3);

    // Verify callback recorded transitions in order (SINGLE, CONSISTENT, INCONSISTENT)
    let list = changes.lock().unwrap();
    assert!(list.iter().any(|(st, _)| *st == StunState::Single));
    assert!(list.iter().any(|(st, _)| *st == StunState::Consistent));
    assert!(list.iter().any(|(st, _)| *st == StunState::Inconsistent));

    finder.stop();
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn error_after_three_intervals_with_less_than_two_responders() {
    let mut finder = StunEndpointFinderImpl::new();
    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();
    finder.start(vec![s1, s2], 5, 5);
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_clone = hits.clone();
    finder.set_error_handler(Some(Arc::new(move |msg| {
        assert!(msg.contains("Fewer than 2 STUN servers responded"));
        hits_clone.fetch_add(1, AOrdering::SeqCst);
    })));
    // Simulate only one server ever responding
    let r1 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s1, &r1);
    // Wait until error handler invoked or timeout
    let deadline = Instant::now() + Duration::from_millis(300);
    while Instant::now() < deadline && hits.load(AOrdering::SeqCst) < 1 {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(hits.load(AOrdering::SeqCst) >= 1);
    finder.stop();
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn two_consistent_responses_trigger_consistent_with_ip_in_callback() {
    let mut finder = StunEndpointFinderImpl::new();
    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();
    finder.start(vec![s1, s2], 50, 50);

    let seen: Arc<Mutex<Vec<(StunState, Option<SocketAddr>)>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_clone = Arc::clone(&seen);
    finder.set_state_change_handler(Some(Arc::new(move |st, ep| {
        seen_clone.lock().unwrap().push((st, ep));
    })));

    let (ip, port) = (IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9)), 55000);
    let r1 = make_xor_mapped_response([203, 0, 113, 9], port);
    finder.process_packet(s1, &r1);
    let r2 = make_xor_mapped_response([203, 0, 113, 9], port);
    finder.process_packet(s2, &r2);
    let list = seen.lock().unwrap();
    let consistent = list.iter().rfind(|(st, _)| *st == StunState::Consistent).cloned();
    assert!(consistent.is_some());
    assert_eq!(consistent.unwrap().1, Some(SocketAddr::new(ip, port)));

    finder.stop();
}

// This test verifies that when the NAT mapping changes (endpoint port changes between
// polling rounds), a new Consistent callback is fired with the updated endpoint.
//
// Bug 1: the Consistent state was not re-fired when the endpoint changed but the
//   state variant stayed Consistent.  Fix: added an `endpoint_changed` guard.
// Bug 2: per-server `endpoint` and `responded` were not cleared when new binding
//   requests were sent.  Stale endpoints from the previous round lingered, which caused
//   a transient Inconsistent state when one server responded first.  Fix: clear both
//   fields in the send loop.
//
// This test uses a short repeat interval so the background thread fires a second round
// of binding requests (which with the Bug 2 fix clears both servers' stale endpoints).
// The test then injects both new responses.  Without the fixes the final Consistent
// callback with port 64715 is either not emitted (Bug 1 only) or arrives as a
// Consistent→Inconsistent→Consistent bounce (Bug 2 without Bug 1 fix).
#[test]
#[cfg(not(target_os = "ios"))]
pub fn consistent_endpoint_change_fires_callback() {
    init_test_logging();

    let mut finder = StunEndpointFinderImpl::new();
    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();

    // Track send calls so we know when the background thread starts a new round
    let send_count: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let send_count_clone = Arc::clone(&send_count);
    finder.set_send_packet_handler(Some(Arc::new(move |_host: &str, _port: u16, _data: &[u8]| {
        *send_count_clone.lock().unwrap() += 1;
    })));

    // Short repeat so the background thread fires a second round within the test
    finder.start(vec![s1, s2], 100, 20);

    let changes: Arc<Mutex<Vec<(StunState, Option<SocketAddr>)>>> =
        Arc::new(Mutex::new(Vec::new()));
    let changes_clone = Arc::clone(&changes);
    finder.set_state_change_handler(Some(Arc::new(move |st, ep| {
        changes_clone.lock().unwrap().push((st, ep));
    })));

    // Round 1: both servers agree on port 2447 -> state becomes Consistent(2447)
    let r1a = make_xor_mapped_response([150, 228, 229, 199], 2447);
    finder.process_packet(s1, &r1a);
    let r1b = make_xor_mapped_response([150, 228, 229, 199], 2447);
    finder.process_packet(s2, &r1b);

    // Verify Consistent(2447) was reported
    let first_consistent = {
        let list = changes.lock().unwrap();
        list.iter()
            .rfind(|(st, _)| *st == StunState::Consistent)
            .cloned()
    };
    assert!(first_consistent.is_some(), "expected Consistent callback after round 1");
    assert_eq!(
        first_consistent.unwrap().1.unwrap().port(),
        2447,
        "expected endpoint port 2447 in round 1"
    );

    // Wait for the background thread to start a new polling round (it will send at
    // least 2 new binding requests -- one per server -- and clear stale endpoints).
    let sends_after_round1 = *send_count.lock().unwrap();
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        let sends = *send_count.lock().unwrap();
        if sends >= sends_after_round1 + 2 { break; }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        *send_count.lock().unwrap() >= sends_after_round1 + 2,
        "background thread did not send a second round of binding requests in time"
    );

    let callbacks_before_round2 = changes.lock().unwrap().len();

    // Round 2: NAT port has changed to 64715.  Both servers respond with the new port.
    let r2a = make_xor_mapped_response([150, 228, 229, 199], 64715);
    finder.process_packet(s1, &r2a);
    let r2b = make_xor_mapped_response([150, 228, 229, 199], 64715);
    finder.process_packet(s2, &r2b);

    // A Consistent(64715) callback must have fired after round 2
    let list = changes.lock().unwrap();
    let consistent_64715 = list
        .iter()
        .skip(callbacks_before_round2)
        .find(|(st, ep)| {
            *st == StunState::Consistent
                && ep.map(|e| e.port()) == Some(64715)
        })
        .cloned();
    assert!(
        consistent_64715.is_some(),
        "expected a Consistent(64715) callback after round 2 endpoint change, \
         but callbacks after round 1 were: {:?}",
        &list[callbacks_before_round2..]
    );

    finder.stop();
}

// Test that servers which have ever responded are not removed when they fail to
// respond in subsequent polling rounds (e.g. in Consistent state repeat-polling).
// This protects against transient network issues at our end dropping all servers
// and resetting state to None after a healthy Consistent phase.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn consistent_servers_not_removed_when_no_repeat_response() {
    use std::collections::HashMap;

    let mut finder = StunEndpointFinderImpl::new();
    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();

    let counts: Arc<Mutex<HashMap<String, usize>>> = Arc::new(Mutex::new(HashMap::new()));
    let counts_clone = Arc::clone(&counts);

    // search=100ms, repeat=20ms: once Consistent the repeat interval fires quickly.
    finder.set_send_packet_handler(Some(Arc::new(move |host: &str, port: u16, _data: &[u8]| {
        let key = format!("{}:{}", host, port);
        let mut m = counts_clone.lock().unwrap();
        *m.entry(key).or_insert(0) += 1;
    })));

    finder.start(vec![s1, s2], 100, 20);

    // Both servers respond once → state goes Consistent; neither responds again.
    let r1 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s1, &r1);
    let r2 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s2, &r2);

    let s1_key = format!("{}:{}", s1.ip(), s1.port());
    let s2_key = format!("{}:{}", s2.ip(), s2.port());

    // Wait for Consistent state + several repeat-interval polls (>= 5 × 20ms = 100ms).
    // Use 400ms to give plenty of margin.
    std::thread::sleep(Duration::from_millis(400));

    let s1_before = counts.lock().unwrap().get(&s1_key).cloned().unwrap_or(0);
    let s2_before = counts.lock().unwrap().get(&s2_key).cloned().unwrap_or(0);

    // Both must have been polled at least 3 times to have accumulated 3+ failures.
    assert!(s1_before >= 3,
        "expected s1 polled >=3 times in repeat phase, got {}", s1_before);
    assert!(s2_before >= 3,
        "expected s2 polled >=3 times in repeat phase, got {}", s2_before);

    // Wait a further 100ms; both servers must still be polled (ever_responded keeps them).
    std::thread::sleep(Duration::from_millis(100));

    let s1_after = counts.lock().unwrap().get(&s1_key).cloned().unwrap_or(0);
    let s2_after = counts.lock().unwrap().get(&s2_key).cloned().unwrap_or(0);

    finder.stop();

    assert!(s1_after > s1_before,
        "s1 (reached Consistent, ever_responded=true) should keep being polled \
         after repeated failures, but poll count stalled at {}", s1_before);
    assert!(s2_after > s2_before,
        "s2 (reached Consistent, ever_responded=true) should keep being polled \
         after repeated failures, but poll count stalled at {}", s2_before);
}
