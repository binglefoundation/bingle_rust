use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use rust_comms::dtls::{NetworkMux, UdpNetworkMux};
use rust_comms::stun::{StunEndpointFinder, StunEndpointFinderImpl, StunState};
use crate::engine::ddb_upsert::test_util::init_test_logging;

// Global holder of the endpoint finder so a plain function pointer handler can access it.
static FINDER: OnceLock<Arc<Mutex<StunEndpointFinderImpl>>> = OnceLock::new();

fn stun_handler(_src: &dyn NetworkMux, from: &SocketAddr, data: &[u8]) {
    if let Some(f_arc) = FINDER.get() {
        if let Ok(mut f) = f_arc.lock() {
            f.process_packet(*from, data);
        }
    }
}

fn resolve(host: &str, port: u16) -> Option<SocketAddr> {
    (host, port).to_socket_addrs().ok()?.next()
}

#[cfg_attr(not(target_os = "ios"), test)]
#[ignore] // Live network test; run explicitly with `cargo test -- --ignored`
fn live_stun_endpoint_finder_with_udp_mux() {
    init_test_logging();
    
    // Choose three public STUN servers (widely used and generally responsive)
    let servers = [
        ("stun.l.google.com", 19302u16),
        ("stun.freeswitch.org", 3478u16),
        ("stun.2talk.co.nz", 3478u16)
    ];

    let addrs: Vec<SocketAddr> = servers
        .iter()
        .filter_map(|(h, p)| resolve(h, *p))
        .collect();

    assert!(addrs.len() >= 2, "expected to resolve at least two STUN servers");

    // Bind a UDP network mux and install the STUN handler that forwards to our finder
    let mux = Arc::new(
        UdpNetworkMux::bind(("0.0.0.0", 0))
            .expect("bind mux")
            .with_handle_stun(Arc::new(stun_handler)),
    );
    mux.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
    mux.start().expect("start mux");

    // Prepare the endpoint finder and wire it to the mux for sending
    let finder = StunEndpointFinderImpl::new();
    let finder_arc = Arc::new(Mutex::new(finder));
    // publish globally for the handler
    let _ = FINDER.set(Arc::clone(&finder_arc));

    let seen: Arc<Mutex<Vec<(StunState, Option<SocketAddr>)>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_clone = Arc::clone(&seen);

    {
        let mut f = finder_arc.lock().unwrap();
        // Send via mux.write with DNS resolution handled by std using (host, port)
        let mux_for_send = Arc::clone(&mux);
        f.set_send_packet_handler(Some(Arc::new(move |host: &str, port: u16, data: &[u8]| {
            // Best-effort send; ignore error in test path
            if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                            let addr = std::net::SocketAddr::new(ip, port);
                            let nsk = rust_comms::api::bingle_api::NetworkEndpoint::new_direct(addr);
                            let _ = mux_for_send.write(&nsk, data);
                        }
        })));

        f.set_state_change_handler(Some(Arc::new(move |st, ep| {
            seen_clone.lock().unwrap().push((st, ep));
        })));

        // Use moderate intervals suitable for live network
        f.start(addrs, 400, 1500);
    }

    // Wait until we reach CONSISTENT (common case) or INCONSISTENT with >=2 responses
    let start = Instant::now();
    let deadline = Duration::from_secs(10);
    let ok = loop {
        if start.elapsed() > deadline { break false; }
        {
            let rec = seen.lock().unwrap();
            if let Some((st, ep)) = rec.iter().rev().find(|(st, _)| *st == StunState::Consistent || *st == StunState::Inconsistent) {
                // For CONSISTENT, we expect an endpoint Some(..)
                if *st == StunState::Consistent {
                    assert!(ep.is_some(), "CONSISTENT should provide endpoint");
                    break true;
                } else {
                    // INCONSISTENT may occur on some networks; accept it as success for live test
                    tracing::info!("Stun state {:?}: {:?}", st, ep);
                    break true;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    };

    // Stop the finder and mux
    {
        let mut f = finder_arc.lock().unwrap();
        f.stop();
    }
    mux.stop();

    assert!(ok, "did not reach CONSISTENT or INCONSISTENT state in time");
}
