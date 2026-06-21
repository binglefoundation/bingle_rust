use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering as AOrdering};
use std::time::{Duration, Instant};

// Bring in the public trait and types from the crate under test
use rust_comms::stun::{StunEndpointFinder, StunState, StunEndpointFinderImpl};
use crate::util::test_util::init_test_logging;

fn make_xor_mapped_response(ip: [u8; 4], port: u16) -> Vec<u8> {
    // Build a minimal STUN success with XOR-MAPPED-ADDRESS for IPv4
    let mut pkt = vec![0u8; 20];
    // Message Type: 0x0101 (Binding Success Response)
    pkt[0] = 0x01; pkt[1] = 0x01;
    // We'll add one attribute of length 8
    pkt[2] = 0x00; pkt[3] = 0x0c; // 12 bytes (type+len + value)
    // Magic Cookie
    pkt[4] = 0x21; pkt[5] = 0x12; pkt[6] = 0xA4; pkt[7] = 0x42;
    // Transaction ID (12 bytes arbitrary)
    for i in 0..12 { pkt[8 + i] = i as u8; }
    // Attribute: XOR-MAPPED-ADDRESS (0x0020), length 8
    pkt.extend_from_slice(&[0x00, 0x20, 0x00, 0x08]);
    // Value: 0x00 family(0x01), x-port, x-address
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
pub fn single_response_triggers_single_and_callback_without_ip() {
    let mut finder = StunEndpointFinderImpl::new();
    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();
    finder.start(vec![s1, s2], 100, 200);

    let seen: Arc<Mutex<Vec<(StunState, Option<SocketAddr>)>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_clone = Arc::clone(&seen);
    finder.set_state_change_handler(Some(Arc::new(move |st, ep| {
        seen_clone.lock().unwrap().push((st, ep));
    })));

    // One response
    let resp = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s1, &resp);

    let list = seen.lock().unwrap();
    // Find the SINGLE callback and ensure endpoint is None
    let single = list.iter().find(|(st, _)| *st == StunState::Single).cloned();
    assert!(single.is_some());
    assert!(single.unwrap().1.is_none());

    finder.stop();
}


#[test]
#[cfg(not(target_os = "ios"))]
pub fn two_inconsistent_responses_trigger_inconsistent_callback_without_ip() {
    let mut finder = StunEndpointFinderImpl::new();
    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();
    finder.start(vec![s1, s2], 50, 50);

    let seen: Arc<Mutex<Vec<(StunState, Option<SocketAddr>)>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_clone = Arc::clone(&seen);
    finder.set_state_change_handler(Some(Arc::new(move |st, ep| {
        seen_clone.lock().unwrap().push((st, ep));
    })));

    let r1 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s1, &r1);
    let r2 = make_xor_mapped_response([203, 0, 113, 10], 55001);
    finder.process_packet(s2, &r2);
    let list = seen.lock().unwrap();
    let incons = list.iter().rfind(|(st, _)| *st == StunState::Inconsistent).cloned();
    assert!(incons.is_some());
    assert!(incons.unwrap().1.is_none());

    finder.stop();
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn two_consistent_then_one_inconsistent_switches_to_inconsistent_and_callback_without_ip() {
    let mut finder = StunEndpointFinderImpl::new();
    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();
    let s3: SocketAddr = "9.9.9.9:3478".parse().unwrap();
    finder.start(vec![s1, s2, s3], 500, 1000);

    let seen: Arc<Mutex<Vec<(StunState, Option<SocketAddr>)>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_clone = Arc::clone(&seen);
    finder.set_state_change_handler(Some(Arc::new(move |st, ep| {
        seen_clone.lock().unwrap().push((st, ep));
    })));

    // Two consistent
    let r1 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s1, &r1);
    let r2 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s2, &r2);

    // Then one inconsistent from third server
    let r3 = make_xor_mapped_response([203, 0, 113, 10], 55001);
    finder.process_packet(s3, &r3);

    let list = seen.lock().unwrap();
    let incons = list.iter().rfind(|(st, _)| *st == StunState::Inconsistent).cloned();
    assert!(incons.is_some());
    assert!(incons.unwrap().1.is_none());

    finder.stop();
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn after_two_responses_polls_resume_on_repeat_interval() {
    let mut finder = StunEndpointFinderImpl::new();
    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();

    let calls: Arc<Mutex<Vec<(String, u16)>>> = Arc::new(Mutex::new(Vec::new()));
    let calls_clone = Arc::clone(&calls);

    let search_ms = 500u64;
    let repeat_ms = 1500u64;
    finder.set_send_packet_handler(Some(Arc::new(move |host: &str, port: u16, _data: &[u8]| {
        calls_clone.lock().unwrap().push((host.to_string(), port));
    })));

    finder.start(vec![s1, s2], search_ms, repeat_ms);
    finder.stop(); // Stop background thread for manual ticking

    // Tick 1: Initial search poll happens because last_poll_tick is None
    finder.tick_for_test();
    {
        let rec = calls.lock().unwrap();
        assert_eq!(rec.len(), 2, "initial poll should send 2 packets");
    }

    // Provide two responses quickly to move to CONSISTENT
    let r1 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s1, &r1);
    let r2 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s2, &r2);

    // Now in CONSISTENT state. Repeat interval is 1500ms = 15 ticks.
    // Next poll should happen at tick 1 + 15 = 16.

    // Clear calls to measure only the next poll cycle
    calls.lock().unwrap().clear();

    // Tick 14 times (total ticks = 15). Should not have polled yet.
    finder.test_ticks(14);
    {
        let rec = calls.lock().unwrap();
        assert_eq!(rec.len(), 0, "should not poll before repeat interval");
    }

    // Tick once more (total ticks = 16). Should poll.
    finder.tick_for_test();
    {
        let rec = calls.lock().unwrap();
        assert_eq!(rec.len(), 2, "should poll all servers after repeat interval");
        let s1_key = (s1.ip().to_string(), s1.port());
        let s2_key = (s2.ip().to_string(), s2.port());
        assert!(rec.iter().any(|(h, p)| *h == s1_key.0 && *p == s1_key.1), "expected repeat poll for s1");
        assert!(rec.iter().any(|(h, p)| *h == s2_key.0 && *p == s2_key.1), "expected repeat poll for s2");
    }
}


// Integration test: when no STUN server responds within the timeout period,
// the state should transition to Blocked.
//
// The finder sends binding requests every `search_interval_ms` milliseconds.
// After 3 consecutive intervals where a server has not responded, the server's
// failure count reaches 3, it is removed from the list, and once all servers are
// removed and no responses are seen for 3 intervals, the state is set to Blocked.
//
// This test installs a send handler that deliberately does NOT deliver any
// response (simulating a firewall blocking all STUN traffic), then waits for
// the background thread to fire at least 3 intervals and asserts that the
// state-change callback was called with Blocked.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn blocked_when_no_servers_respond_within_timeout() {
    init_test_logging();

    let mut finder = StunEndpointFinderImpl::new();
    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();

    // Install a send handler that discards packets (simulates a blocked network).
    // The handler must exist so that the background thread attempts to send but
    // no process_packet call ever arrives.
    let send_count: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let send_count_clone = Arc::clone(&send_count);
    finder.set_send_packet_handler(Some(Arc::new(move |_host: &str, _port: u16, _data: &[u8]| {
        *send_count_clone.lock().unwrap() += 1;
        // Deliberately do nothing: no response is injected.
    })));

    let changes: Arc<Mutex<Vec<(StunState, Option<SocketAddr>)>>> =
        Arc::new(Mutex::new(Vec::new()));
    let changes_clone = Arc::clone(&changes);
    finder.set_state_change_handler(Some(Arc::new(move |st, ep| {
        changes_clone.lock().unwrap().push((st, ep));
    })));

    // Use a tick-based approach for determinism.
    // search_interval_ms = 100 ms (1 tick)
    finder.start(vec![s1, s2], 100, 60_000);
    finder.stop(); // Stop the background thread immediately so we can tick manually.

    // Tick the finder manually. 3 intervals × 1 tick/interval = 3 ticks.
    finder.test_ticks(10);

    let sends = *send_count.lock().unwrap();
    assert!(
        sends >= 6,
        "expected at least 6 send attempts (3 intervals × 2 servers) but got {}",
        sends
    );

    let list = changes.lock().unwrap();
    let blocked = list.iter().any(|(st, _)| *st == StunState::Blocked);
    assert!(
        blocked,
        "expected state to become Blocked after all servers failed to respond, \
         but state changes were: {:?}",
        *list
    );
}

// Test that a server which has ever responded is not removed even after 3+ consecutive
// poll failures. This verifies the fix: retain a server if ever_responded=true, so that
// transient network issues at our end do not cause a previously-reachable server to be
// permanently dropped.
//
// Scenario: s1 responds once (ever_responded=true), s2 never responds.
// After 3 failures s2 is removed. s1 is kept alive by ever_responded.
// Eventually intervals_without_two reaches 3, state goes Blocked, and in Blocked
// state ALL servers are polled — so s1 must receive further binding requests.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn server_responds_once_then_stops_keeps_polling() {
    use std::collections::HashMap;

    let mut finder = StunEndpointFinderImpl::new();
    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();

    let counts: Arc<Mutex<HashMap<String, usize>>> = Arc::new(Mutex::new(HashMap::new()));
    let counts_clone = Arc::clone(&counts);

    finder.set_send_packet_handler(Some(Arc::new(move |host: &str, port: u16, _data: &[u8]| {
        let key = format!("{}:{}", host, port);
        let mut m = counts_clone.lock().unwrap();
        *m.entry(key).or_insert(0) += 1;
    })));

    // search=100ms: reach Blocked quickly (3 × 1 tick = 3 ticks after s1 stops responding).
    finder.start(vec![s1, s2], 100, 10_000);
    finder.stop();

    // s1 responds once then goes silent; s2 never responds.
    let r1 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.tick_for_test(); // Tick 1: first poll
    finder.process_packet(s1, &r1);

    let s1_key = format!("{}:{}", s1.ip(), s1.port());
    let s2_key = format!("{}:{}", s2.ip(), s2.port());

    // Tick until s2 is removed (after 3 failures) and finder enters Blocked.
    // Tick 1 was s1=1, s2=1.
    // Tick 2: s1=1, s2=2 (s1 already responded)
    // Tick 3: s1=1, s2=3
    // Tick 4: s1=2, s2=removed (since intervals_without_two reaches 3)
    finder.test_ticks(5);

    let s1_count = counts.lock().unwrap().get(&s1_key).cloned().unwrap_or(0);
    let s2_count = counts.lock().unwrap().get(&s2_key).cloned().unwrap_or(0);

    // s2 should have been polled then removed after 3 failures (no ever_responded).
    assert!(s2_count >= 3,
        "expected s2 polled >=3 times before removal, got {}", s2_count);

    // s1 must have been polled more than once: initial poll + at least one Blocked-state poll.
    // (In Blocked state ALL servers are polled every interval regardless of responded flag.)
    assert!(s1_count >= 2,
        "s1 (responded once, ever_responded=true) should keep being polled \
         after 3 failures (via Blocked state), but was only polled {} time(s)", s1_count);
}

// Test that when the finder is in Consistent state and then servers stop responding
// for the no-response timeout period, it transitions back to None and fires the
// state-change callback with StunState::None.
//
// Uses set_no_response_timeout_for_tests to set a short 200ms timeout so the test
// runs quickly without waiting the default 30s.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn consistent_reverts_to_none_after_no_response_timeout() {
    init_test_logging();

    let mut finder = StunEndpointFinderImpl::new();
    // Use a short no-response timeout so the test is fast.
    finder.set_no_response_timeout_for_tests(Duration::from_millis(300));

    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();

    let changes: Arc<Mutex<Vec<(StunState, Option<SocketAddr>)>>> =
        Arc::new(Mutex::new(Vec::new()));
    let changes_clone = Arc::clone(&changes);
    finder.set_state_change_handler(Some(Arc::new(move |st, ep| {
        changes_clone.lock().unwrap().push((st, ep));
    })));

    // Use a long repeat interval (1s = 10 ticks) so the poll loop does not re-poll too fast.
    finder.start(vec![s1, s2], 1000, 1000);
    finder.stop();

    // Inject two consistent responses so the finder reaches Consistent state.
    let r1 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    let r2 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.tick_for_test(); // Ensure it polls if background thread didn't yet
    finder.process_packet(s1, &r1);
    finder.process_packet(s2, &r2);

    // Confirm Consistent was reached.
    let reached_consistent = {
        let list = changes.lock().unwrap();
        list.iter().any(|(st, _)| *st == StunState::Consistent)
    };
    assert!(reached_consistent, "finder should have reached Consistent after two matching responses");

    // Manually tick past the no-response timeout (300ms = 3 ticks)
    // plus the repeat poll interval (1000ms = 10 ticks).
    finder.test_ticks(20);

    let list = changes.lock().unwrap();
    let consistent_idx = list.iter().position(|(st, _)| *st == StunState::Consistent)
        .expect("expected Consistent state before None");
    let none_after = list[consistent_idx..].iter().any(|(st, _)| *st == StunState::None);
    assert!(
        none_after,
        "expected StunState::None after no-response timeout, but state changes were: {:?}",
        *list
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn stop_stops_promptly() {
    let mut finder = StunEndpointFinderImpl::new();
    // Start with a long search/repeat time
    finder.start(vec![], 5000, 5000);

    // Give it a moment to actually start the thread
    std::thread::sleep(Duration::from_millis(100));

    let start = Instant::now();
    finder.stop();
    let elapsed = start.elapsed();

    // If it takes more than 1 second, it's definitely not prompt
    assert!(elapsed < Duration::from_millis(500), "stop() took {:?}, which is too long", elapsed);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn state_transitions_consistent_and_inconsistent() {
    let mut finder = StunEndpointFinderImpl::new();
    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();
    finder.start(vec![s1, s2], 50, 50);
    finder.stop();

    let changes = Arc::new(Mutex::new(Vec::<(StunState, Option<SocketAddr>)>::new()));
    let changes_clone = changes.clone();
    finder.set_state_change_handler(Some(Arc::new(move |st, ep| {
        changes_clone.lock().unwrap().push((st, ep));
    })));

    finder.tick_for_test();

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
    finder.start(vec![s1, s2], 100, 100);
    finder.stop();
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_clone = hits.clone();
    finder.set_error_handler(Some(Arc::new(move |msg| {
        assert!(msg.contains("Fewer than 2 STUN servers responded"));
        hits_clone.fetch_add(1, AOrdering::SeqCst);
    })));

    // Tick 1: Initial poll
    finder.tick_for_test();

    // Simulate only one server ever responding
    let r1 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s1, &r1);

    // We need 3 intervals with < 2 responders to trigger the error.
    finder.test_ticks(3);

    assert!(hits.load(AOrdering::SeqCst) >= 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn two_consistent_responses_trigger_consistent_with_ip_in_callback() {
    let mut finder = StunEndpointFinderImpl::new();
    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();
    finder.start(vec![s1, s2], 50, 50);
    finder.stop();

    let seen: Arc<Mutex<Vec<(StunState, Option<SocketAddr>)>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_clone = Arc::clone(&seen);
    finder.set_state_change_handler(Some(Arc::new(move |st, ep| {
        seen_clone.lock().unwrap().push((st, ep));
    })));

    finder.tick_for_test();

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

    // Short repeat so we can tick to a second round within the test
    finder.start(vec![s1, s2], 100, 100);
    finder.stop();

    let changes: Arc<Mutex<Vec<(StunState, Option<SocketAddr>)>>> =
        Arc::new(Mutex::new(Vec::new()));
    let changes_clone = Arc::clone(&changes);
    finder.set_state_change_handler(Some(Arc::new(move |st, ep| {
        changes_clone.lock().unwrap().push((st, ep));
    })));

    finder.tick_for_test();

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

    // Advance to the next polling round (it will send at
    // least 2 new binding requests -- one per server -- and clear stale endpoints).
    let sends_after_round1 = *send_count.lock().unwrap();
    finder.tick_for_test();

    assert!(
        *send_count.lock().unwrap() >= sends_after_round1 + 2,
        "finder did not send a second round of binding requests"
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

#[test]
#[cfg(not(target_os = "ios"))]
pub fn consistent_servers_not_removed_when_no_repeat_response() {
    let mut finder = StunEndpointFinderImpl::new();
    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();

    let counts: Arc<Mutex<HashMap<String, usize>>> = Arc::new(Mutex::new(HashMap::new()));
    let counts_clone = Arc::clone(&counts);

    // search=100ms, repeat=100ms: once Consistent the repeat interval fires every tick.
    finder.set_send_packet_handler(Some(Arc::new(move |host: &str, port: u16, _data: &[u8]| {
        let key = format!("{}:{}", host, port);
        let mut m = counts_clone.lock().unwrap();
        *m.entry(key).or_insert(0) += 1;
    })));

    finder.start(vec![s1, s2], 100, 100);
    finder.stop();

    finder.tick_for_test();

    // Both servers respond once → state goes Consistent; neither responds again.
    let r1 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s1, &r1);
    let r2 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s2, &r2);

    let s1_key = format!("{}:{}", s1.ip(), s1.port());
    let s2_key = format!("{}:{}", s2.ip(), s2.port());

    // Advance 4 ticks (400ms) to trigger repeat-interval polls.
    finder.test_ticks(4);

    let s1_before = counts.lock().unwrap().get(&s1_key).cloned().unwrap_or(0);
    let s2_before = counts.lock().unwrap().get(&s2_key).cloned().unwrap_or(0);

    // Both must have been polled at least 3 times to have accumulated 3+ failures.
    assert!(s1_before >= 3,
        "expected s1 polled >=3 times in repeat phase, got {}", s1_before);
    assert!(s2_before >= 3,
        "expected s2 polled >=3 times in repeat phase, got {}", s2_before);

    // Wait a further tick; both servers must still be polled (ever_responded keeps them).
    finder.tick_for_test();

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
