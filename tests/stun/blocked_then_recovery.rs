// Test that after entering Blocked state, the STUN finder continues polling and
// recovers to Consistent when servers start responding.
//
// This verifies the fix for the bug where servers were removed after 3 failures,
// causing polling to cease permanently once Blocked state was reached.
//
// Also verifies that while in Blocked state, STUN Binding Requests are sent every 2s.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::api::ripple_message_unit::test_util::init_test_logging;
use rust_comms::dtls::{NetworkMux, UdpNetworkMux};
use rust_comms::stun::{
    SimpleStunServer, SimpleStunStartOptions, StunEndpointFinder, StunEndpointFinderImpl, StunState,
};

fn find_unused_loopback_port() -> u16 {
    let sock = UdpSocket::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).expect("bind temp socket");
    let port = sock.local_addr().expect("local addr").port();
    drop(sock);
    port
}

/// Start the finder pointing at two silent addresses (nothing bound), wait for
/// Blocked, then verify STUN Binding Requests are sent every ~2s while blocked,
/// then start two real STUN servers on those addresses and verify the
/// finder recovers to Consistent.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn blocked_then_recovery_to_consistent() {
    init_test_logging();

    // Reserve two ports — nothing is bound yet so STUN requests will be silently dropped.
    let p1 = find_unused_loopback_port();
    let p2 = find_unused_loopback_port();
    let a1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p1);
    let a2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p2);

    // Mux for sending STUN packets
    let mux = Arc::new(UdpNetworkMux::bind(("0.0.0.0", 0)).expect("bind mux"));
    mux.set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();

    let finder = Arc::new(Mutex::new(StunEndpointFinderImpl::new()));
    {
        let finder_clone = finder.clone();
        let handler = Arc::new(
            move |src: &dyn NetworkMux, from: &SocketAddr, data: &[u8]| {
                let _ = src.as_any();
                if let Ok(mut f) = finder_clone.lock() {
                    f.process_packet(*from, data);
                }
            },
        );
        mux.set_handle_stun_arc(Some(handler));
    }

    mux.start().expect("start mux");

    let seen: Arc<Mutex<Vec<StunState>>> = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();

    // Track timestamps of STUN sends that occur while in Blocked state.
    // Each send round dispatches to both servers; we record one timestamp per round
    // (the first send of each round) to measure inter-round intervals.
    let blocked_send_times: Arc<Mutex<Vec<Instant>>> = Arc::new(Mutex::new(Vec::new()));
    let blocked_send_times2 = blocked_send_times.clone();
    let seen_for_send = seen.clone();

    {
        let mut f = finder.lock().unwrap();
        let m = mux.clone();
        // Use a counter to record one timestamp per send round (two servers => two sends per round).
        let send_count: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
        f.set_send_packet_handler(Some(Arc::new(move |host, port, payload| {
            if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                let addr = std::net::SocketAddr::new(ip, port);
                let nsk = rust_comms::api::bingle_api::NetworkEndpoint::new_direct(addr);
                let _ = m.write(&nsk, payload);
            }
            // Record one timestamp per round (every 2nd send = one full round for 2 servers).
            let is_blocked = seen_for_send.lock().unwrap().contains(&StunState::Blocked);
            if is_blocked {
                let mut count = send_count.lock().unwrap();
                *count += 1;
                // First send of each round (odd-numbered send)
                if *count % 2 == 1 {
                    blocked_send_times2.lock().unwrap().push(Instant::now());
                }
            }
        })));
        f.set_state_change_handler(Some(Arc::new(move |st, _ep| {
            seen2.lock().unwrap().push(st);
        })));
        // search_time_ms=2000: in Blocked state requests are sent every 2s.
        // repeat_time_ms=10000: used in Consistent/Inconsistent states.
        // 3 intervals of 2s = 6s to reach Blocked from None.
        f.start(vec![a1, a2], 2000, 10000);
    }

    // Wait for Blocked state (up to 10 s: 3 × 2s intervals + margin)
    let blocked = {
        let start = Instant::now();
        loop {
            if start.elapsed() > Duration::from_secs(10) {
                break false;
            }
            if seen.lock().unwrap().contains(&StunState::Blocked) {
                break true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    assert!(blocked, "finder did not reach Blocked state within 10 s");

    // Wait to collect at least 3 send-round timestamps while in Blocked state
    // (i.e. observe at least 2 inter-round gaps to measure the interval).
    let collected = {
        let start = Instant::now();
        loop {
            if start.elapsed() > Duration::from_secs(10) {
                break false;
            }
            if blocked_send_times.lock().unwrap().len() >= 3 {
                break true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    assert!(
        collected,
        "did not observe 3 Blocked-state send rounds within 10 s"
    );

    // Verify that consecutive send rounds are spaced ~2s apart (within ±50% tolerance).
    {
        let times = blocked_send_times.lock().unwrap();
        for i in 1..times.len() {
            let gap = times[i].duration_since(times[i - 1]);
            assert!(
                gap >= Duration::from_millis(1000) && gap <= Duration::from_millis(3000),
                "Blocked-state send interval was {:?}, expected ~2s (1s–3s)",
                gap
            );
        }
    }

    // Now start real STUN servers on the same addresses — finder should recover.
    let mut s1 = SimpleStunServer::start(SimpleStunStartOptions {
        bind_addr: a1,
        attach_to: None,
        broken_nat: false,
    })
    .expect("start s1");
    let mut s2 = SimpleStunServer::start(SimpleStunStartOptions {
        bind_addr: a2,
        attach_to: None,
        broken_nat: false,
    })
    .expect("start s2");

    // Wait for Consistent state (up to 10 s: next 2s poll + response processing)
    let recovered = {
        let start = Instant::now();
        loop {
            if start.elapsed() > Duration::from_secs(10) {
                break false;
            }
            if seen.lock().unwrap().contains(&StunState::Consistent) {
                break true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    };

    // Cleanup
    {
        let mut f = finder.lock().unwrap();
        f.stop();
    }
    mux.stop();
    s1.stop();
    s2.stop();

    assert!(
        recovered,
        "finder did not recover to Consistent after STUN servers became available"
    );
}
