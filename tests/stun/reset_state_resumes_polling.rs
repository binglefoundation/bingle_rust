use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rust_comms::dtls::{NetworkMux, UdpNetworkMux};
use rust_comms::stun::{StunEndpointFinder, StunEndpointFinderImpl, StunState, SimpleStunServer, SimpleStunStartOptions};

fn find_unused_loopback_port() -> u16 {
    let sock = UdpSocket::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).expect("bind temp socket");
    let port = sock.local_addr().expect("local addr").port();
    drop(sock);
    port
}

/// Verifies that after reset_state() is called (e.g. when no relay is found), the STUN polling
/// loop re-polls all servers and can reach Consistent again. This is the regression test for the
/// bug where reset_state() did not clear server.responded flags, causing the polling loop to
/// skip all servers and never send new binding requests.
#[cfg_attr(not(target_os = "ios"), test)]
pub fn reset_state_resumes_stun_polling() {
    let p1 = find_unused_loopback_port();
    let p2 = find_unused_loopback_port();
    let a1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p1);
    let a2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p2);

    let s1 = SimpleStunServer::start(SimpleStunStartOptions { bind_addr: a1, attach_to: None, broken_nat: false }).expect("start s1");
    let s2 = SimpleStunServer::start(SimpleStunStartOptions { bind_addr: a2, attach_to: None, broken_nat: false }).expect("start s2");

    let mux = Arc::new(UdpNetworkMux::bind(("0.0.0.0", 0)).expect("bind mux"));
    mux.set_read_timeout(Some(Duration::from_millis(100))).unwrap();

    let finder = Arc::new(Mutex::new(StunEndpointFinderImpl::new()));
    {
        let finder_clone = finder.clone();
        let handler = Arc::new(move |src: &dyn NetworkMux, from: &SocketAddr, data: &[u8]| {
            let _ = src.as_any();
            if let Ok(mut f) = finder_clone.lock() { f.process_packet(*from, data); }
        });
        mux.set_handle_stun_arc(Some(handler));
    }

    mux.start().expect("start mux");

    let consistent_count: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
    let consistent_count2 = consistent_count.clone();

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
            if st == StunState::Consistent {
                *consistent_count2.lock().unwrap() += 1;
            }
        })));
        f.start(vec![a1, a2], 100, 500);
    }

    // Wait for first CONSISTENT
    let start = Instant::now();
    let reached_first = loop {
        if start.elapsed() > Duration::from_secs(3) { break false; }
        if *consistent_count.lock().unwrap() >= 1 { break true; }
        std::thread::sleep(Duration::from_millis(25));
    };
    assert!(reached_first, "did not reach first CONSISTENT");

    // Simulate what the engine does when no relay is found: call reset_state()
    {
        let mut f = finder.lock().unwrap();
        f.reset_state();
    }

    // After reset, the polling loop must re-poll servers and reach CONSISTENT again
    let start2 = Instant::now();
    let reached_second = loop {
        if start2.elapsed() > Duration::from_secs(3) { break false; }
        if *consistent_count.lock().unwrap() >= 2 { break true; }
        std::thread::sleep(Duration::from_millis(25));
    };

    // Cleanup
    {
        let mut f = finder.lock().unwrap();
        f.stop();
    }
    mux.stop();
    let mut s1 = s1; let mut s2 = s2;
    s1.stop(); s2.stop();

    assert!(reached_second, "STUN polling did not resume after reset_state() - servers were not re-polled");
}
