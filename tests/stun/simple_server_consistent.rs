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

#[test]
fn simple_stun_two_servers_consistent() {
    // Start two simple STUN servers on random local ports
    let p1 = find_unused_loopback_port();
    let p2 = find_unused_loopback_port();
    let a1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p1);
    let a2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p2);

    let s1 = SimpleStunServer::start(SimpleStunStartOptions { bind_addr: a1, attach_to: None, broken_nat: false }).expect("start s1");
    let s2 = SimpleStunServer::start(SimpleStunStartOptions { bind_addr: a2, attach_to: None, broken_nat: false }).expect("start s2");

    // Mux bound to 0 that will send to both servers from the same local socket
    let mut mux = Arc::new(UdpNetworkMux::bind(("0.0.0.0", 0)).expect("bind mux"));
    mux.set_read_timeout(Some(Duration::from_millis(100))).unwrap();

    // Wire mux STUN handler to forward packets into our finder
    let finder = Arc::new(Mutex::new(StunEndpointFinderImpl::new()));
    {
        let finder_clone = finder.clone();
        let handler = Arc::new(move |src: &dyn NetworkMux, from: &SocketAddr, data: &[u8]| {
            let _ = src.as_any();
            if let Ok(mut f) = finder_clone.lock() { f.process_packet(*from, data); }
        });
        // Set the STUN handler before cloning the Arc, using Arc::get_mut while we have unique ownership
        if let Some(inner) = Arc::get_mut(&mut mux) {
            inner.set_handle_stun(Some(handler));
        } else {
            panic!("expected unique Arc for mux when setting handler");
        }
    }

    mux.start().expect("start mux");

    let seen: Arc<Mutex<Vec<(StunState, Option<SocketAddr>)>>> = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();

    // Configure finder send path via mux
    {
        let mut f = finder.lock().unwrap();
        let m = mux.clone();
        f.set_send_packet_handler(Some(Arc::new(move |host, port, payload| {
            let _ = m.write((host, port), payload);
        })));
        f.set_state_change_handler(Some(Arc::new(move |st, ep| {
            seen2.lock().unwrap().push((st, ep));
        })));
        f.start(vec![a1, a2], 100, 500);
    }

    // Wait for CONSISTENT
    let start = Instant::now();
    let ok = loop {
        if start.elapsed() > Duration::from_secs(3) { break false; }
        {
            let rec = seen.lock().unwrap();
            if let Some((st, ep)) = rec.iter().rev().find(|(s, _)| *s == StunState::Consistent) {
                assert!(ep.is_some(), "CONSISTENT should have endpoint");
                break true;
            }
        }
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

    assert!(ok, "did not reach CONSISTENT");
}
