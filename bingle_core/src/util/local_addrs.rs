//! The host's own IPv6 addresses (issue #498), used to tell whether a STUN-mapped IPv6 address is
//! translated (a NAT66 or prefix translator in the path) or is the host's own address (no NAT).

use std::net::{IpAddr, Ipv6Addr, SocketAddr, UdpSocket};

/// A global IPv6 destination used only to ask the kernel which source address and route it would
/// pick; connecting a UDP socket sends nothing. Google Public DNS, an operator already in the
/// built-in STUN list.
pub const IPV6_ROUTE_PROBE: SocketAddr = SocketAddr::new(
    IpAddr::V6(Ipv6Addr::new(0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888)),
    53,
);

/// What the host knows about its own IPv6 addressing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalIpv6 {
    /// Every IPv6 address on the host's interfaces, and the source address the kernel picks for
    /// a global destination.
    pub addrs: Vec<Ipv6Addr>,
    /// Whether the host has a route to the global IPv6 internet from a global source address.
    pub route: bool,
}

impl LocalIpv6 {
    /// Whether `ip` is one of the host's own addresses.
    pub fn contains(&self, ip: &IpAddr) -> bool {
        match ip.to_canonical() {
            IpAddr::V6(v6) => self.addrs.contains(&v6),
            IpAddr::V4(_) => false,
        }
    }
}

/// Source of the host's IPv6 addresses, injectable for tests.
pub trait LocalAddrSource: Send + Sync {
    /// The host's IPv6 addresses and whether it has a global IPv6 route, read now.
    fn local_ipv6(&self) -> LocalIpv6;
}

/// The operating system's view: `getifaddrs` on Unix, plus the source address of a UDP socket
/// connected towards [`IPV6_ROUTE_PROBE`].
pub struct SystemLocalAddrs;

impl LocalAddrSource for SystemLocalAddrs {
    fn local_ipv6(&self) -> LocalIpv6 {
        let mut addrs = interface_ipv6_addrs();
        let source = outbound_ipv6_source();
        if let Some(src) = source
            && !addrs.contains(&src)
        {
            addrs.push(src);
        }
        LocalIpv6 {
            addrs,
            route: source.is_some_and(|s| is_global_unicast(&s)),
        }
    }
}

/// Whether `ip` is a global unicast IPv6 address: not unspecified, loopback, link-local
/// (fe80::/10), unique local (fc00::/7), multicast, IPv4-mapped or the documentation prefix.
pub fn is_global_unicast(ip: &Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || ip.to_ipv4_mapped().is_some()
        || (first & 0xffc0) == 0xfe80
        || (first & 0xfe00) == 0xfc00
        || (first == 0x2001 && ip.segments()[1] == 0x0db8))
}

/// The source address the kernel picks for a global IPv6 destination; `None` when there is no
/// IPv6 route (the connect fails) or IPv6 is unavailable.
fn outbound_ipv6_source() -> Option<Ipv6Addr> {
    let sock = UdpSocket::bind((Ipv6Addr::UNSPECIFIED, 0)).ok()?;
    sock.connect(IPV6_ROUTE_PROBE).ok()?;
    match sock.local_addr().ok()?.ip() {
        IpAddr::V6(v6) if !v6.is_unspecified() => Some(v6),
        _ => None,
    }
}

/// Every IPv6 address on the host's interfaces.
#[cfg(unix)]
fn interface_ipv6_addrs() -> Vec<Ipv6Addr> {
    let mut out = Vec::new();
    let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs fills head with a list we free below with freeifaddrs.
    if unsafe { libc::getifaddrs(&mut head) } != 0 || head.is_null() {
        tracing::debug!("[local_addrs] getifaddrs failed; using the route probe only");
        return out;
    }
    let mut cur = head;
    while !cur.is_null() {
        // SAFETY: cur is a node of the list getifaddrs returned.
        let ifa = unsafe { &*cur };
        if !ifa.ifa_addr.is_null() {
            // SAFETY: ifa_addr points at a sockaddr whose family field is valid to read.
            let family = unsafe { (*ifa.ifa_addr).sa_family } as libc::c_int;
            if family == libc::AF_INET6 {
                // SAFETY: an AF_INET6 entry's ifa_addr points at a sockaddr_in6.
                let sa = unsafe { &*(ifa.ifa_addr as *const libc::sockaddr_in6) };
                let v6 = Ipv6Addr::from(sa.sin6_addr.s6_addr);
                if !out.contains(&v6) {
                    out.push(v6);
                }
            }
        }
        cur = ifa.ifa_next;
    }
    // SAFETY: head came from getifaddrs and is freed once.
    unsafe { libc::freeifaddrs(head) };
    out
}

/// Every IPv6 address on the host's interfaces; without `getifaddrs` only the route probe is used.
#[cfg(not(unix))]
fn interface_ipv6_addrs() -> Vec<Ipv6Addr> {
    Vec::new()
}
