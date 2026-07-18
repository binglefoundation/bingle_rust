use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bingle_core::dtls::{NetworkMux, UdpNetworkMux};
use bingle_core::stun::{
    SimpleStunServer, SimpleStunStartOptions, StunEndpointFinder, StunEndpointFinderImpl, StunState,
};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn simple_stun_mixed_servers_inconsistent() {
    // Start two simple STUN servers: one normal, one broken_nat
    let p1 = crate::util::test_util::find_unused_loopback_port();
    let p2 = crate::util::test_util::find_unused_loopback_port();
    let a1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p1);
    let a2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p2);

    let s1 = SimpleStunServer::start(SimpleStunStartOptions {
        bind_addr: a1,
        attach_to: None,
        broken_nat: false,
    })
    .expect("start s1");
    let s2 = SimpleStunServer::start(SimpleStunStartOptions {
        bind_addr: a2,
        attach_to: None,
        broken_nat: true,
    })
    .expect("start s2");

    // Mux
    let mux = Arc::new(UdpNetworkMux::bind(("0.0.0.0", 0)).expect("bind mux"));
    mux.set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();

    // Wire mux STUN handler to forward packets into our finder
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

    let seen: Arc<Mutex<Vec<(StunState, Option<SocketAddr>)>>> = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();

    // Configure finder send path via mux
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
        f.set_state_change_handler(Some(Arc::new(move |st, ep| {
            seen2.lock().unwrap().push((st, ep));
        })));
        f.start(vec![a1, a2], 100, 500);
    }

    // Wait for INCONSISTENT
    let start = Instant::now();
    let ok = loop {
        if start.elapsed() > Duration::from_secs(3) {
            break false;
        }
        {
            let rec = seen.lock().unwrap();
            if let Some((st, _)) = rec
                .iter()
                .rev()
                .find(|(s, _)| *s == StunState::Inconsistent)
            {
                let _ = st; // just to satisfy borrow checker
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
    let mut s1 = s1;
    let mut s2 = s2;
    s1.stop();
    s2.stop();

    assert!(ok, "did not reach INCONSISTENT");
}
