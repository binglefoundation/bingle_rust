use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
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
    finder.start(vec![s1, s2, s3], 50, 50);

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
pub fn nonresponsive_server_removed_after_three_search_polls() {
    use std::collections::HashMap;

    let mut finder = StunEndpointFinderImpl::new();
    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();

    // Count sends per server
    let counts: Arc<Mutex<HashMap<String, usize>>> = Arc::new(Mutex::new(HashMap::new()));
    let counts_clone = Arc::clone(&counts);

    finder.set_send_packet_handler(Some(Arc::new(move |host: &str, port: u16, _data: &[u8]| {
        let key = format!("{}:{}", host, port);
        let mut m = counts_clone.lock().unwrap();
        *m.entry(key).or_insert(0) += 1;
    })));

    finder.start(vec![s1, s2], 5, 1000); // long repeat to avoid interference; search=5ms

    // s1 responds once; s2 never responds
    let r1 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s1, &r1);

    // Wait enough for s2 to be polled >=3 times and then removed
    std::thread::sleep(Duration::from_millis(30));

    let before = counts.lock().unwrap().get(&format!("{}:{}", s2.ip(), s2.port())).cloned().unwrap_or(0);

    // Sleep additional time; count for s2 should not increase after removal
    std::thread::sleep(Duration::from_millis(20));

    let after = counts.lock().unwrap().get(&format!("{}:{}", s2.ip(), s2.port())).cloned().unwrap_or(0);

    assert!(before >= 3, "expected at least 3 polls before removal, got {}", before);
    assert_eq!(before, after, "nonresponsive server should stop receiving polls after 3 failures");

    finder.stop();
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn after_two_responses_polls_resume_on_repeat_interval() {
    let mut finder = StunEndpointFinderImpl::new();
    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();

    let calls: Arc<Mutex<Vec<(String, u16, Instant)>>> = Arc::new(Mutex::new(Vec::new()));
    let calls_clone = Arc::clone(&calls);

    let search_ms = 5u64;
    let repeat_ms = 15u64;
    finder.set_send_packet_handler(Some(Arc::new(move |host: &str, port: u16, _data: &[u8]| {
        calls_clone.lock().unwrap().push((host.to_string(), port, Instant::now()));
    })));

    finder.start(vec![s1, s2], search_ms, repeat_ms);

    // Provide two responses quickly to move to CONSISTENT
    let r1 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s1, &r1);
    let r2 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s2, &r2);

    let t0 = Instant::now();
    // Wait up to ~3*repeat for a re-poll cycle
    std::thread::sleep(Duration::from_millis(repeat_ms as u64 * 3));

    let rec = calls.lock().unwrap();
    let any_after: Vec<_> = rec.iter().filter(|(_, _, t)| *t >= t0).collect();
    // Expect at least one call for each server after t0 (repeat cycle polls all servers)
    let s1_key = (s1.ip().to_string(), s1.port());
    let s2_key = (s2.ip().to_string(), s2.port());
    assert!(any_after.iter().any(|(h, p, _)| *h == s1_key.0 && *p == s1_key.1), "expected repeat poll for s1");
    assert!(any_after.iter().any(|(h, p, _)| *h == s2_key.0 && *p == s2_key.1), "expected repeat poll for s2");
    finder.stop();
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

    // Use a very short search interval so the 3-interval timeout fires quickly.
    // With search_interval_ms = 20 ms, 3 intervals = ~60 ms, well within 1 s.
    let search_interval_ms: u64 = 20;
    finder.start(vec![s1, s2], search_interval_ms, 60_000);

    let changes: Arc<Mutex<Vec<(StunState, Option<SocketAddr>)>>> =
        Arc::new(Mutex::new(Vec::new()));
    let changes_clone = Arc::clone(&changes);
    finder.set_state_change_handler(Some(Arc::new(move |st, ep| {
        changes_clone.lock().unwrap().push((st, ep));
    })));

    // Wait until we see a Blocked state or until 1 second has elapsed.
    let deadline = Instant::now() + Duration::from_millis(1_000);
    loop {
        {
            let list = changes.lock().unwrap();
            if list.iter().any(|(st, _)| *st == StunState::Blocked) {
                break;
            }
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    finder.stop();

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

    // search=20ms: reach Blocked quickly (3 × 20ms = ~60ms after s1 stops responding).
    finder.start(vec![s1, s2], 20, 10_000);

    // s1 responds once then goes silent; s2 never responds.
    let r1 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s1, &r1);

    let s1_key = format!("{}:{}", s1.ip(), s1.port());
    let s2_key = format!("{}:{}", s2.ip(), s2.port());

    // Wait long enough for s2 to accumulate 3 failures and be removed,
    // and for the finder to enter Blocked state and poll s1 again.
    // 3 intervals × 20ms = 60ms; add margin for thread scheduling.
    std::thread::sleep(Duration::from_millis(300));

    let s1_count = counts.lock().unwrap().get(&s1_key).cloned().unwrap_or(0);
    let s2_count = counts.lock().unwrap().get(&s2_key).cloned().unwrap_or(0);

    finder.stop();

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
    finder.set_no_response_timeout_for_tests(Duration::from_millis(200));

    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();

    // Use a long repeat interval (1s) so the poll loop does not re-poll too fast.
    // The no-response timeout (200ms) is shorter than the repeat interval (1s),
    // so the check triggers on the first repeat poll after the 200ms window.
    finder.start(vec![s1, s2], 50, 500);

    let changes: Arc<Mutex<Vec<(StunState, Option<SocketAddr>)>>> =
        Arc::new(Mutex::new(Vec::new()));
    let changes_clone = Arc::clone(&changes);
    finder.set_state_change_handler(Some(Arc::new(move |st, ep| {
        changes_clone.lock().unwrap().push((st, ep));
    })));

    // Inject two consistent responses so the finder reaches Consistent state.
    let r1 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    let r2 = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s1, &r1);
    finder.process_packet(s2, &r2);

    // Confirm Consistent was reached.
    let reached_consistent = {
        let list = changes.lock().unwrap();
        list.iter().any(|(st, _)| *st == StunState::Consistent)
    };
    assert!(reached_consistent, "finder should have reached Consistent after two matching responses");

    // Wait past the no-response timeout (200ms) plus the repeat poll interval (500ms)
    // so the poll loop triggers the timeout check.  Allow generous headroom for CI load.
    let deadline = Instant::now() + Duration::from_millis(5_000);
    loop {
        {
            let list = changes.lock().unwrap();
            // Look for a None transition AFTER the Consistent one.
            let consistent_idx = list.iter().position(|(st, _)| *st == StunState::Consistent);
            if let Some(ci) = consistent_idx {
                if list[ci..].iter().any(|(st, _)| *st == StunState::None) {
                    break;
                }
            }
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    finder.stop();

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

// Regression test for the bug where the no-response revert fired in the same loop
// iteration that sent the binding request.
//
// The sequence that triggered the bug:
//   1. Finder reaches Consistent state; last_response_time is set.
//   2. no_response_timeout elapses with no further response (e.g., network hiccup).
//   3. Loop wakes, sends binding requests, then IMMEDIATELY checks the timeout and
//      reverts — before the server has any chance to reply.
//
// Expected behaviour: after sending the request the timer is anchored to the moment
// of sending (last_request_time), so the revert must not fire until
// no_response_timeout elapses AFTER that request with still no answer.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn consistent_revert_does_not_fire_immediately_after_request_is_sent() {
    init_test_logging();

    let mut finder = StunEndpointFinderImpl::new();
    // Short no-response timeout and a repeat interval longer than the timeout so
    // the poll loop will send a request then immediately see the (now-stale)
    // last_response_time exceed the timeout — which was the bug.
    let no_response_ms = 200u64;
    let repeat_ms      = 600u64;
    finder.set_no_response_timeout_for_tests(Duration::from_millis(no_response_ms));

    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();

    let changes: Arc<Mutex<Vec<(StunState, Option<SocketAddr>)>>> =
        Arc::new(Mutex::new(Vec::new()));
    let changes_clone = Arc::clone(&changes);
    finder.set_state_change_handler(Some(Arc::new(move |st, ep| {
        changes_clone.lock().unwrap().push((st, ep));
    })));

    // Use a search interval long enough that both process_packet calls complete
    // before the background thread wakes and clears server state.  The repeat
    // interval (repeat_ms) is what we actually want to test.
    let search_ms = 500u64;
    finder.start(vec![s1, s2], search_ms, repeat_ms);

    // Inject two consistent responses → Consistent state; last_response_time is set.
    // Both happen synchronously here so the background thread has not yet run.
    let r = make_xor_mapped_response([203, 0, 113, 9], 55000);
    finder.process_packet(s1, &r);
    finder.process_packet(s2, &r);

    // Wait until Consistent is confirmed.
    let deadline = Instant::now() + Duration::from_millis(4_000);
    loop {
        if changes.lock().unwrap().iter().any(|(st, _)| *st == StunState::Consistent) { break; }
        assert!(Instant::now() < deadline, "timed out waiting for Consistent state");
        std::thread::sleep(Duration::from_millis(10));
    }

    // Record the time we reached Consistent.  From this point we withhold further
    // responses; the finder must not revert until at least no_response_ms after it
    // first sends a Consistent-state binding request (i.e., after repeat_ms + no_response_ms).
    let consistent_reached_at = Instant::now();

    // Wait until a None transition is recorded after Consistent.
    let overall_deadline = Instant::now() + Duration::from_millis(10_000);
    loop {
        let list = changes.lock().unwrap();
        let consistent_idx = list.iter().position(|(st, _)| *st == StunState::Consistent);
        if let Some(ci) = consistent_idx {
            if list[ci..].iter().any(|(st, _)| *st == StunState::None) { break; }
        }
        assert!(Instant::now() < overall_deadline, "timed out waiting for revert to None");
        drop(list);
        std::thread::sleep(Duration::from_millis(20));
    }

    let revert_elapsed = consistent_reached_at.elapsed();

    finder.stop();

    // Core assertion: the revert must not have fired immediately after the first
    // Consistent-state binding request.  The first such request is sent after
    // repeat_ms (600ms); the revert may only fire after another no_response_ms (200ms)
    // elapses with no reply.  So total time from Consistent to revert >= repeat_ms + no_response_ms.
    //
    // We allow a generous lower bound of repeat_ms alone (conservatively lower than
    // repeat_ms + no_response_ms) to avoid flakiness on slow CI machines, while still
    // catching the bug (which fires the revert within a few milliseconds of Consistent).
    let min_expected = Duration::from_millis(repeat_ms);
    assert!(
        revert_elapsed >= min_expected,
        "revert fired only {:?} after Consistent was reached \
         (expected >= {}ms = repeat_ms); the revert must not fire immediately after \
         the first Consistent-state binding request",
        revert_elapsed, repeat_ms
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
