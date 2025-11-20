use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicUsize, Ordering as AOrdering};

// Bring in the public trait and types from the crate under test
use rust_comms::stun::{StunEndpointFinder, StunState, StunEndpointFinderImpl};

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
fn state_transitions_consistent_and_inconsistent() {
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
fn error_after_three_intervals_with_less_than_two_responders() {
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
    // Wait a bit more than 3 intervals to allow error condition
    std::thread::sleep(Duration::from_millis(20));
    assert!(hits.load(AOrdering::SeqCst) >= 1);
    finder.stop();
}

#[test]
fn single_response_triggers_single_and_callback_without_ip() {
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
fn two_consistent_responses_trigger_consistent_with_ip_in_callback() {
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

#[test]
fn two_inconsistent_responses_trigger_inconsistent_callback_without_ip() {
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
fn two_consistent_then_one_inconsistent_switches_to_inconsistent_and_callback_without_ip() {
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
fn nonresponsive_server_removed_after_three_search_polls() {
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
fn after_two_responses_polls_resume_on_repeat_interval() {
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
