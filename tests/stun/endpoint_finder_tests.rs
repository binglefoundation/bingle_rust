use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicUsize, Ordering as AOrdering};

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

#[cfg_attr(not(target_os = "ios"), test)]
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

#[cfg_attr(not(target_os = "ios"), test)]
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

#[cfg_attr(not(target_os = "ios"), test)]
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

#[cfg_attr(not(target_os = "ios"), test)]
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

#[cfg_attr(not(target_os = "ios"), test)]
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

#[cfg_attr(not(target_os = "ios"), test)]
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

#[cfg_attr(not(target_os = "ios"), test)]
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

#[cfg_attr(not(target_os = "ios"), test)]
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

// Regression test for Bug 1 + Bug 2: when the NAT port changes between polling rounds,
// the state-change callback must fire with the new endpoint even though the StunState
// stays Consistent.
//
// Bug 1: recompute_state_and_notify only fired the callback when the StunState enum
//   variant changed.  A NAT port change with both servers still agreeing (Consistent)
//   was silently dropped.  Fix: also fire when self.endpoint changes while staying
//   Consistent.
//
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
#[cfg_attr(not(target_os = "ios"), test)]
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
    // With the Bug 2 fix the stale endpoints are already cleared, so the first response
    // yields Single (state change Consistent->Single -> callback fires) and the second
    // yields Consistent(64715) (state change Single->Consistent -> callback fires).
    // Without the Bug 1 fix but with Bug 2 fix: first response goes Single (fires),
    // second goes Consistent(64715) (fires) -- still works via state-variant changes.
    // Without either fix: first response goes Inconsistent(stale+new), second goes
    // Consistent(64715) -- also fires via state-variant changes.
    // Without Bug 1 fix but also without Bug 2 fix, AND if somehow both servers already
    // have 64715 simultaneously: stays Consistent, no callback (the silent bug case).
    // The endpoint_changed guard (Bug 1 fix) makes this robust regardless of path.
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
#[cfg_attr(not(target_os = "ios"), test)]
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

#[cfg_attr(not(target_os = "ios"), test)]
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
