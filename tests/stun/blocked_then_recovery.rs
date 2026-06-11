// Test that after entering Blocked state, the STUN finder continues polling and
// recovers to Consistent when servers start responding.
//
// This verifies the fix for the bug where servers were removed after 3 failures,
// causing polling to cease permanently once Blocked state was reached.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rust_comms::dtls::{NetworkMux, UdpNetworkMux};
use rust_comms::stun::{SimpleStunServer, SimpleStunStartOptions, StunEndpointFinder, StunEndpointFinderImpl, StunState};

fn find_unused_loopback_port() -> u16 {
    let sock = UdpSocket::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).expect("bind temp socket");
    let port = sock.local_addr().expect("local addr").port();
    drop(sock);
    port
}

/// Start the finder pointing at two silent addresses (nothing bound), wait for
/// Blocked, then start two real STUN servers on those addresses and verify the
/// finder recovers to Consistent.
#[cfg_attr(not(target_os = "ios"), test)]
pub fn blocked_then_recovery_to_consistent() {
    // Reserve two ports — nothing is bound yet so STUN requests will be silently dropped.
    let p1 = find_unused_loopback_port();
    let p2 = find_unused_loopback_port();
    let a1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p1);
    let a2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p2);

    // Mux for sending STUN packets
    let mux = Arc::new(UdpNetworkMux::bind(("0.0.0.0", 0)).expect("bind mux"));
    mux.set_read_timeout(Some(Duration::from_millis(100))).unwrap();

    let finder = Arc::new(Mutex::new(StunEndpointFinderImpl::new()));
    {
        let finder_clone = finder.clone();
        let handler = Arc::new(move |src: &dyn NetworkMux, from: &SocketAddr, data: &[u8]| {
            let _ = src.as_any();
            if let Ok(mut f) = finder_clone.lock() {
                f.process_packet(*from, data);
            }
        });
        mux.set_handle_stun_arc(Some(handler));
    }

    mux.start().expect("start mux");

    let seen: Arc<Mutex<Vec<StunState>>> = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();

    {
        let mut f = finder.lock().unwrap();
        let m = mux.clone();
        f.set_send_packet_handler(Some(Arc::new(move |host, port, payload| {
            if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                let addr = std::net::SocketAddr::new(ip, port);
                let nsk = rust_comms::api::bingle_api::NetworkEndpoint::new_direct(addr);
                let _ = m.write(&nsk, payload);
            }
        })));
        f.set_state_change_handler(Some(Arc::new(move |st, _ep| {
            seen2.lock().unwrap().push(st);
        })));
        // Short search interval: 3 × 100 ms = 300 ms to reach Blocked
        f.start(vec![a1, a2], 100, 1000);
    }

    // Wait for Blocked state (up to 3 s)
    let blocked = {
        let start = Instant::now();
        loop {
            if start.elapsed() > Duration::from_secs(3) {
                break false;
            }
            if seen.lock().unwrap().contains(&StunState::Blocked) {
                break true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    assert!(blocked, "finder did not reach Blocked state within 3 s");

    // Now start real STUN servers on the same addresses — finder should recover.
    let mut s1 = SimpleStunServer::start(SimpleStunStartOptions { bind_addr: a1, attach_to: None, broken_nat: false })
        .expect("start s1");
    let mut s2 = SimpleStunServer::start(SimpleStunStartOptions { bind_addr: a2, attach_to: None, broken_nat: false })
        .expect("start s2");

    // Wait for Consistent state (up to 5 s)
    let recovered = {
        let start = Instant::now();
        loop {
            if start.elapsed() > Duration::from_secs(5) {
                break false;
            }
            if seen.lock().unwrap().contains(&StunState::Consistent) {
                break true;
            }
            std::thread::sleep(Duration::from_millis(50));
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

    assert!(recovered, "finder did not recover to Consistent after STUN servers became available");
}
