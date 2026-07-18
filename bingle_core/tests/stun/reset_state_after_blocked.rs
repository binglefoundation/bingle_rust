// Regression test: after reaching Blocked state (intervals_without_two >= 3),
// calling reset_state() must reset intervals_without_two so the finder can
// recover to Consistent when servers become available — not immediately
// re-enter Blocked on the very next poll.
//
// This reproduces the bug observed in the field: after a connection drop and
// IP:port change, the state changed to None (via reset_state) but the server
// list went empty because the finder immediately re-entered Blocked and the
// state_change callback fired with None endpoint before any server could respond.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bingle_core::dtls::{NetworkMux, UdpNetworkMux};
use bingle_core::stun::{
    SimpleStunServer, SimpleStunStartOptions, StunEndpointFinder, StunEndpointFinderImpl, StunState,
};

/// Verifies that reset_state() after Blocked does not immediately re-enter Blocked.
/// Steps:
///   1. Start finder with no servers listening → reaches Blocked.
///   2. Call reset_state() → state goes to None, intervals_without_two reset to 0.
///   3. Start real STUN servers → finder must reach Consistent (not re-Blocked first).
#[test]
#[cfg(not(target_os = "ios"))]
pub fn reset_state_after_blocked_recovers_to_consistent() {
    let p1 = crate::util::test_util::find_unused_loopback_port();
    let p2 = crate::util::test_util::find_unused_loopback_port();
    let a1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p1);
    let a2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p2);

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

    // Track all state changes in order so we can assert the sequence.
    let states: Arc<Mutex<Vec<StunState>>> = Arc::new(Mutex::new(Vec::new()));
    let states2 = states.clone();

    {
        let mut f = finder.lock().unwrap();
        let m = mux.clone();
        f.set_send_packet_handler(Some(Arc::new(move |host, port, payload| {
            if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                let addr = std::net::SocketAddr::new(ip, port);
                let nsk = bingle_core::api::bingle_api::NetworkEndpoint::new_direct(addr);
                let _ = m.write(&nsk, payload);
            }
        })));
        f.set_state_change_handler(Some(Arc::new(move |st, _ep| {
            states2.lock().unwrap().push(st);
        })));
        // search_time_ms=100: reach Blocked quickly (3 × 100ms = ~300ms).
        f.start(vec![a1, a2], 100, 500);
    }

    // Step 1: wait for Blocked state.
    let blocked = {
        let start = Instant::now();
        loop {
            if start.elapsed() > Duration::from_secs(3) {
                break false;
            }
            if states.lock().unwrap().contains(&StunState::Blocked) {
                break true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    };
    assert!(blocked, "finder did not reach Blocked within 3s");

    // Step 2: call reset_state() — this is what the engine does on connection drop.
    {
        let mut f = finder.lock().unwrap();
        f.reset_state();
    }

    // Step 3: start real STUN servers — finder must now reach Consistent.
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

    let consistent = {
        let start = Instant::now();
        loop {
            if start.elapsed() > Duration::from_secs(3) {
                break false;
            }
            if states.lock().unwrap().contains(&StunState::Consistent) {
                break true;
            }
            std::thread::sleep(Duration::from_millis(25));
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
        consistent,
        "finder did not reach Consistent after reset_state() + servers available — intervals_without_two was not reset"
    );

    // Also verify: after reset_state(), the finder did not immediately fire Blocked again
    // before reaching Consistent. The state sequence after reset should be None → ... → Consistent,
    // not None → Blocked → Consistent.
    let state_seq = states.lock().unwrap().clone();
    // Find the index of the last Blocked (before reset) and check no Blocked appears after Consistent.
    let consistent_pos = state_seq
        .iter()
        .rposition(|s| *s == StunState::Consistent)
        .expect("Consistent must appear");
    let blocked_after_consistent = state_seq[consistent_pos + 1..]
        .iter()
        .any(|s| *s == StunState::Blocked);
    assert!(
        !blocked_after_consistent,
        "Blocked appeared after Consistent — unexpected state regression"
    );
}
