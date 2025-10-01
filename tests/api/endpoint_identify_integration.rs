use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::EngineState;
use rust_comms::dtls::{NetworkMux, UdpNetworkMux};
use rust_comms::stun::{StunEndpointFinder, StunEndpointFinderImpl, StunState, SimpleStunServer, SimpleStunStartOptions};

fn find_unused_loopback_port() -> u16 {
    // Bind to 127.0.0.1:0 to let OS choose a free port, then return that port.
    // Drop the socket to free it for the test process to rebind shortly after.
    let sock = UdpSocket::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).expect("bind temp socket");
    let port = sock.local_addr().expect("local addr").port();
    drop(sock);
    port
}

// Option B integration test: use BingleApiImpl as the entry point, but mock out
// the discovery by forcing STUN consistent on the underlying Engine. We avoid
// a real Algorand localnet and real relays; instead, we start two relay instances
// (static endpoints) and two client instances, then validate that the clients reach
// EndpointAvailable with the expected public address.
#[test]
fn bingle_api_endpoint_identify_via_forced_stun() {
    // Set up two relay instances with static endpoints (127.0.0.1 with known, unused ports)
    let r1_port = find_unused_loopback_port();
    let r2_port = find_unused_loopback_port();
    assert_ne!(r1_port, 0);
    assert_ne!(r2_port, 0);
    let relay1_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r1_port);
    let relay2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), r2_port);

    let mut relay1 = BingleApiImpl::new();
    let mut relay2 = BingleApiImpl::new();

    let r1_opts = StartOptions { handle: "relay1".into(), algo_passphrase: Some("b64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()), static_ip: Some(relay1_addr), am_relay: true, stun_servers: None };
    let r2_opts = StartOptions { handle: "relay2".into(), algo_passphrase: Some("b64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()), static_ip: Some(relay2_addr), am_relay: true, stun_servers: None };

    // Start relays (no assertions about DTLS; we use them only as placeholders)
    let _ = relay1.start(r1_opts);
    let _ = relay2.start(r2_opts);

    // Start two local STUN servers we will use for consistency resolution
    let p1 = find_unused_loopback_port();
    let p2 = find_unused_loopback_port();
    let a1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p1);
    let a2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), p2);

    let mut s1 = SimpleStunServer::start(SimpleStunStartOptions { bind_addr: a1, attach_to: None, broken_nat: false }).expect("start s1");
    let mut s2 = SimpleStunServer::start(SimpleStunStartOptions { bind_addr: a2, attach_to: None, broken_nat: false }).expect("start s2");

    // Two client instances without staticEndpoint; provide the STUN server list to Engine.start
    let mut client1 = BingleApiImpl::new();
    let mut client2 = BingleApiImpl::new();

    let stun_list = vec![a1, a2];
    let c1_opts = StartOptions { handle: "client1".into(), algo_passphrase: Some("b64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()), static_ip: None, am_relay: false, stun_servers: Some(stun_list.clone()) };
    let c2_opts = StartOptions { handle: "client2".into(), algo_passphrase: Some("b64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()), static_ip: None, am_relay: false, stun_servers: Some(stun_list.clone()) };

    client1.start(c1_opts).expect("client1 start() failed");
    client2.start(c2_opts).expect("client2 start() failed");

    // Independently verify our two STUN servers yield a CONSISTENT endpoint using a standalone finder
    let mut mux = Arc::new(UdpNetworkMux::bind(("0.0.0.0", 0)).expect("bind mux"));
    mux.set_read_timeout(Some(Duration::from_millis(100))).unwrap();

    let finder = Arc::new(Mutex::new(StunEndpointFinderImpl::new()));
    {
        let finder_clone = finder.clone();
        let handler = Arc::new(move |src: &dyn NetworkMux, from: &SocketAddr, data: &[u8]| {
            let _ = src.as_any();
            if let Ok(mut f) = finder_clone.lock() { f.process_packet(*from, data); }
        });
        if let Some(inner) = Arc::get_mut(&mut mux) {
            inner.set_handle_stun(Some(handler));
        } else {
            panic!("expected unique Arc for mux when setting handler");
        }
    }
    mux.start().expect("start mux");

    let seen: Arc<Mutex<Vec<(StunState, Option<SocketAddr>)>>> = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();
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

    // Wait for CONSISTENT from the standalone finder
    let start = Instant::now();
    let mut discovered_ep: Option<SocketAddr> = None;
    let ok = loop {
        if start.elapsed() > Duration::from_secs(3) { break false; }
        {
            let rec = seen.lock().unwrap();
            if let Some((st, ep)) = rec.iter().rev().find(|(s, _)| *s == StunState::Consistent) {
                assert!(ep.is_some(), "CONSISTENT should have endpoint");
                discovered_ep = *ep;
                break true;
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    };

    // Inform each client engine of the discovered public endpoint (test hook), then require EndpointAvailable
    // if let Some(ep) = discovered_ep {
    //     client1.engine_force_stun_consistent_for_tests(ep);
    //     client2.engine_force_stun_consistent_for_tests(ep);
    // }

    // Wait up to 10 seconds for both client engines to enter EndpointAvailable
    let wait_start = Instant::now();
    while wait_start.elapsed() < Duration::from_secs(10) {
        if client1.engine_state_for_tests() == Some(EngineState::EndpointAvailable)
            && client2.engine_state_for_tests() == Some(EngineState::EndpointAvailable) {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    let s1_state = client1.engine_state_for_tests();
    let s2_state = client2.engine_state_for_tests();
    assert_eq!(s1_state, Some(EngineState::EndpointAvailable), "unexpected client1 state: {:?}", s1_state);
    assert_eq!(s2_state, Some(EngineState::EndpointAvailable), "unexpected client2 state: {:?}", s2_state);

    // Cleanup
    {
        let mut f = finder.lock().unwrap();
        f.stop();
    }
    mux.stop();

    // Stop instances and STUN servers
    relay1.stop();
    relay2.stop();
    client1.stop();
    client2.stop();
    s1.stop();
    s2.stop();

    assert!(ok, "did not reach CONSISTENT");
}
