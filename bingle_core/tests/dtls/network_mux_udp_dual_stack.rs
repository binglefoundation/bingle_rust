use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bingle_core::api::bingle_api::IpFamily;
use bingle_core::api::bingle_api::NetworkEndpoint;
use bingle_core::dtls::network_mux_udp::{SocketMode, canonical_peer, socket_target};
use bingle_core::dtls::{NetworkMux, UdpNetworkMux};

/// A started mux of `family` whose STUN handler records each sender address. STUN-class datagrams
/// (first byte 0..=3) reach the handler without any DTLS state, so they show what the receive path
/// reports as the peer address.
fn recording_mux(family: IpFamily) -> (Arc<UdpNetworkMux>, Arc<Mutex<Vec<SocketAddr>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    let mux = UdpNetworkMux::bind_all(0, family)
        .expect("bind mux")
        .with_handle_stun(Arc::new(move |_src, from, _data| {
            s.lock().unwrap().push(*from);
        }));
    let mux = Arc::new(mux);
    mux.start().expect("start mux");
    (mux, seen)
}

fn wait_for(seen: &Mutex<Vec<SocketAddr>>, n: usize) -> Vec<SocketAddr> {
    let start = Instant::now();
    while seen.lock().unwrap().len() < n && start.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(10));
    }
    seen.lock().unwrap().clone()
}

fn v4_loopback(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

fn v6_loopback(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port)
}

/// A socket bound to `addr`'s loopback that reports whether a datagram arrives within the timeout.
fn receiver(addr: SocketAddr) -> UdpSocket {
    let s = UdpSocket::bind(addr).expect("bind receiver");
    s.set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    s
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ip_family_parses_and_displays() {
    for (text, family) in [
        ("auto", IpFamily::Auto),
        ("v4", IpFamily::V4),
        ("V6", IpFamily::V6),
        ("ipv4", IpFamily::V4),
    ] {
        assert_eq!(text.parse::<IpFamily>().unwrap(), family, "{text}");
    }
    assert!("both".parse::<IpFamily>().is_err());
    assert_eq!(IpFamily::default(), IpFamily::Auto);
    assert_eq!(IpFamily::V6.to_string(), "v6");
    assert_eq!(serde_json::to_string(&IpFamily::V4).unwrap(), "\"v4\"");
    assert_eq!(
        serde_json::from_str::<IpFamily>("\"auto\"").unwrap(),
        IpFamily::Auto
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn canonical_peer_unmaps_ipv4_mapped_addresses() {
    let mapped: SocketAddr = "[::ffff:192.0.2.1]:3478".parse().unwrap();
    assert_eq!(canonical_peer(mapped), "192.0.2.1:3478".parse().unwrap());
    let v6: SocketAddr = "[2001:db8::1]:3478".parse().unwrap();
    assert_eq!(canonical_peer(v6), v6);
    let v4: SocketAddr = "192.0.2.1:3478".parse().unwrap();
    assert_eq!(canonical_peer(v4), v4);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn socket_target_maps_only_where_needed() {
    let v4: SocketAddr = "192.0.2.1:3478".parse().unwrap();
    let v6: SocketAddr = "[2001:db8::1]:3478".parse().unwrap();
    let mapped: SocketAddr = "[::ffff:192.0.2.1]:3478".parse().unwrap();

    assert_eq!(socket_target(SocketMode::V4, v4).unwrap(), v4);
    assert!(socket_target(SocketMode::V4, v6).is_err());
    assert_eq!(socket_target(SocketMode::V4, mapped).unwrap(), v4);

    assert_eq!(socket_target(SocketMode::DualStack, v4).unwrap(), mapped);
    assert_eq!(socket_target(SocketMode::DualStack, v6).unwrap(), v6);

    assert_eq!(socket_target(SocketMode::V6Only, v6).unwrap(), v6);
    assert!(socket_target(SocketMode::V6Only, v4).is_err());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn auto_mux_receives_both_families_as_canonical_addresses() {
    let (mux, seen) = recording_mux(IpFamily::Auto);
    assert_eq!(
        mux.mode(),
        SocketMode::DualStack,
        "auto is dual-stack where the host has IPv6"
    );
    let port = mux.local_addr().unwrap().port();

    let s4 = UdpSocket::bind(v4_loopback(0)).unwrap();
    let s6 = UdpSocket::bind(v6_loopback(0)).unwrap();
    s4.send_to(&[0, 1, 2], v4_loopback(port)).unwrap();
    s6.send_to(&[0, 1, 2], v6_loopback(port)).unwrap();

    let got = wait_for(&seen, 2);
    mux.stop();
    assert!(
        got.contains(&s4.local_addr().unwrap()),
        "IPv4 sender reported as plain IPv4, not ::ffff: ({got:?})"
    );
    assert!(
        got.contains(&s6.local_addr().unwrap()),
        "IPv6 sender reported ({got:?})"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn dual_stack_mux_sends_to_both_families() {
    let (mux, _seen) = recording_mux(IpFamily::Auto);
    let r4 = receiver(v4_loopback(0));
    let r6 = receiver(v6_loopback(0));
    let mut buf = [0u8; 16];

    mux.write(
        &NetworkEndpoint::new_direct(r4.local_addr().unwrap()),
        &[0, 4],
    )
    .expect("send to IPv4 from the dual-stack socket");
    mux.write(
        &NetworkEndpoint::new_direct(r6.local_addr().unwrap()),
        &[0, 6],
    )
    .expect("send to IPv6 from the dual-stack socket");

    let (n4, _) = r4
        .recv_from(&mut buf)
        .expect("IPv4 receiver got the datagram");
    assert_eq!(&buf[..n4], &[0, 4]);
    let (n6, _) = r6
        .recv_from(&mut buf)
        .expect("IPv6 receiver got the datagram");
    assert_eq!(&buf[..n6], &[0, 6]);
    mux.stop();
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn v4_family_uses_only_ipv4() {
    let (mux, seen) = recording_mux(IpFamily::V4);
    assert_eq!(mux.mode(), SocketMode::V4);
    assert!(mux.local_addr().unwrap().is_ipv4());
    let port = mux.local_addr().unwrap().port();

    let s4 = UdpSocket::bind(v4_loopback(0)).unwrap();
    s4.send_to(&[0, 1], v4_loopback(port)).unwrap();
    assert_eq!(wait_for(&seen, 1), vec![s4.local_addr().unwrap()]);

    let err = mux
        .write(&NetworkEndpoint::new_direct(v6_loopback(9)), &[0])
        .expect_err("an IPv4 socket refuses an IPv6 destination");
    assert!(err.contains("IPv6"), "{err}");
    mux.stop();
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn v6_family_uses_only_ipv6() {
    let (mux, seen) = recording_mux(IpFamily::V6);
    assert_eq!(mux.mode(), SocketMode::V6Only);
    let port = mux.local_addr().unwrap().port();

    let s6 = UdpSocket::bind(v6_loopback(0)).unwrap();
    s6.send_to(&[0, 1], v6_loopback(port)).unwrap();
    // An IPv4 datagram to the same port number has no IPv4 socket to land on.
    let s4 = UdpSocket::bind(v4_loopback(0)).unwrap();
    let _ = s4.send_to(&[0, 2], v4_loopback(port));

    let got = wait_for(&seen, 1);
    std::thread::sleep(Duration::from_millis(200));
    let got_after = seen.lock().unwrap().clone();
    assert_eq!(got, vec![s6.local_addr().unwrap()]);
    assert_eq!(
        got_after, got,
        "no IPv4 datagram reached the IPv6-only socket"
    );

    let err = mux
        .write(&NetworkEndpoint::new_direct(v4_loopback(9)), &[0])
        .expect_err("an IPv6-only socket refuses an IPv4 destination");
    assert!(err.contains("IPv4"), "{err}");
    mux.stop();
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn rebind_keeps_the_socket_mode() {
    for (family, mode) in [
        (IpFamily::Auto, SocketMode::DualStack),
        (IpFamily::V4, SocketMode::V4),
        (IpFamily::V6, SocketMode::V6Only),
    ] {
        let mux = UdpNetworkMux::bind_all(0, family).expect("bind");
        let before = mux.local_addr().unwrap();
        mux.drop_socket_for_tests();
        let rebound = mux.rebind_for_tests().expect("rebind");
        let after = rebound.local_addr().unwrap();
        assert_eq!(
            after, before,
            "{family}: same address and port after rebind"
        );
        let only_v6 = after.is_ipv6() && socket2::SockRef::from(&rebound).only_v6().unwrap();
        let rebound_mode = match (after.is_ipv4(), only_v6) {
            (true, _) => SocketMode::V4,
            (false, true) => SocketMode::V6Only,
            (false, false) => SocketMode::DualStack,
        };
        assert_eq!(rebound_mode, mode, "{family}");
    }
}
