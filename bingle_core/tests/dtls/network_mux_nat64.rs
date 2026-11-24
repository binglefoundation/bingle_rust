//! NAT64 at the UDP mux (issue #496), exercised on loopback with the test prefix `::/96`, under
//! which IPv4 `0.0.0.1` is synthesised as `::1`: a send to `0.0.0.1:p` must reach `[::1]:p`, and a
//! datagram from `[::1]:p` must be reported as `0.0.0.1:p`.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bingle_core::api::bingle_api::{IpFamily, NetworkEndpoint};
use bingle_core::dtls::{NetworkMux, UdpNetworkMux};
use bingle_core::util::nat64::{Nat64, Nat64Prefix, Nat64Resolver};

struct NoResolver;
impl Nat64Resolver for NoResolver {
    fn aaaa(&self, _host: &str) -> Vec<Ipv6Addr> {
        Vec::new()
    }
    fn synthesise(&self, _v4: Ipv4Addr) -> Option<Ipv6Addr> {
        None
    }
}

fn loopback_nat64() -> Arc<Nat64> {
    let prefix: Nat64Prefix = "::/96".parse().unwrap();
    Arc::new(Nat64::new(Box::new(NoResolver), Some(prefix)))
}

fn v4_peer(port: u16) -> SocketAddr {
    SocketAddr::new(Ipv4Addr::new(0, 0, 0, 1).into(), port)
}

fn mux_with_nat64(
    family: IpFamily,
) -> (Arc<UdpNetworkMux>, Arc<Nat64>, Arc<Mutex<Vec<SocketAddr>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    let mux = UdpNetworkMux::bind_all(0, family)
        .expect("bind")
        .with_handle_stun(Arc::new(move |_src, from, _data| {
            s.lock().unwrap().push(*from);
        }));
    let nat64 = loopback_nat64();
    mux.set_nat64(nat64.clone());
    let mux = Arc::new(mux);
    mux.start().expect("start");
    (mux, nat64, seen)
}

fn ipv6_receiver() -> UdpSocket {
    let r = UdpSocket::bind("[::1]:0").unwrap();
    r.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    r
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ipv6_only_socket_reaches_an_ipv4_peer_through_nat64() {
    let (mux, _nat64, _seen) = mux_with_nat64(IpFamily::V6);
    let r = ipv6_receiver();
    let port = r.local_addr().unwrap().port();

    mux.write(&NetworkEndpoint::new_direct(v4_peer(port)), &[0, 64])
        .expect("send through NAT64");
    let mut buf = [0u8; 8];
    let (n, _) = r
        .recv_from(&mut buf)
        .expect("the synthesised address received it");
    assert_eq!(&buf[..n], &[0, 64]);
    mux.stop();
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn dual_stack_socket_uses_nat64_once_ipv4_has_no_route() {
    let (mux, nat64, _seen) = mux_with_nat64(IpFamily::Auto);
    nat64.mark_no_ipv4_route();
    let r = ipv6_receiver();
    let port = r.local_addr().unwrap().port();

    mux.write(&NetworkEndpoint::new_direct(v4_peer(port)), &[0, 46])
        .expect("send");
    let mut buf = [0u8; 8];
    let (n, _) = r.recv_from(&mut buf).expect("received via NAT64");
    assert_eq!(&buf[..n], &[0, 46]);
    mux.stop();
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn datagrams_from_a_synthesised_address_are_reported_as_ipv4() {
    let (mux, _nat64, seen) = mux_with_nat64(IpFamily::V6);
    let port = mux.local_addr().unwrap().port();
    let sender = UdpSocket::bind("[::1]:0").unwrap();
    sender
        .send_to(&[0, 1], SocketAddr::new(Ipv6Addr::LOCALHOST.into(), port))
        .unwrap();

    let start = Instant::now();
    while seen.lock().unwrap().is_empty() && start.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(10));
    }
    let got = seen.lock().unwrap().clone();
    mux.stop();
    assert_eq!(got, vec![v4_peer(sender.local_addr().unwrap().port())]);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ipv4_socket_ignores_nat64() {
    let mux = UdpNetworkMux::bind_all(0, IpFamily::V4).expect("bind");
    mux.set_nat64(loopback_nat64());
    let err = mux
        .write(
            &NetworkEndpoint::new_direct("[::1]:9".parse().unwrap()),
            &[0],
        )
        .expect_err("still IPv4-only");
    assert!(err.contains("IPv6"), "{err}");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn socket_recovery_clears_the_no_route_finding() {
    let mux = UdpNetworkMux::bind_all(0, IpFamily::Auto).expect("bind");
    let nat64 = loopback_nat64();
    mux.set_nat64(nat64.clone());
    assert!(nat64.mark_no_ipv4_route());
    mux.drop_socket_for_tests();
    mux.rebind_for_tests().expect("rebind");
    assert!(
        !nat64.no_ipv4_route(),
        "a recovered socket tries IPv4 natively again"
    );
}
