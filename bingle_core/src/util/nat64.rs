//! NAT64 address translation (issue #496), so a node on an IPv6-only network with NAT64/DNS64 (as
//! App Review uses) can reach IPv4 peers, relays and STUN servers by their IPv4 literals.
//!
//! Rust never passes an IPv4 literal through the system resolver, so nothing synthesises its NAT64
//! form. This module discovers the network's NAT64 prefix (RFC 7050: the `ipv4only.arpa` AAAA
//! answer embeds 192.0.0.170/171) and embeds or extracts IPv4 addresses per RFC 6052. The UDP mux
//! uses it at the socket: a send to an IPv4 peer with no IPv4 route goes to the synthesised IPv6
//! address, and a datagram from a synthesised address is reported as its IPv4 peer, so everything
//! above the socket keeps one IPv4 identity per peer.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The well-known name whose AAAA answer reveals the NAT64 prefix (RFC 7050).
pub const IPV4ONLY_ARPA: &str = "ipv4only.arpa";
/// The two IPv4 addresses `ipv4only.arpa` resolves to (RFC 7050 §2.2).
const WELL_KNOWN_IPV4: [Ipv4Addr; 2] =
    [Ipv4Addr::new(192, 0, 0, 170), Ipv4Addr::new(192, 0, 0, 171)];
/// How long a "no IPv4 route" finding holds before the next IPv4 send tries the native route again.
/// Short enough that a brief Wi-Fi drop (which also gives "no route" errors) doesn't divert IPv4
/// traffic for long; long enough that an IPv6-only network isn't re-probed on every send.
pub const NO_IPV4_ROUTE_HOLD: Duration = Duration::from_secs(60);
/// Prefix lengths RFC 6052 §2.2 allows.
const PREFIX_LENGTHS: [u8; 6] = [32, 40, 48, 56, 64, 96];

/// A NAT64 prefix (RFC 6052): the leading `len` bits of `addr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nat64Prefix {
    addr: Ipv6Addr,
    len: u8,
}

/// The byte positions of the 4 embedded IPv4 bytes for each prefix length (RFC 6052 §2.2). Bits
/// 64–71 (byte 8, the "u" octet) are never used, so the shorter prefixes split around it.
fn embed_positions(len: u8) -> Option<[usize; 4]> {
    match len {
        32 => Some([4, 5, 6, 7]),
        40 => Some([5, 6, 7, 9]),
        48 => Some([6, 7, 9, 10]),
        56 => Some([7, 9, 10, 11]),
        64 => Some([9, 10, 11, 12]),
        96 => Some([12, 13, 14, 15]),
        _ => None,
    }
}

impl Nat64Prefix {
    /// A prefix of `len` bits (one of 32, 40, 48, 56, 64, 96) taken from `addr`. Bits after the
    /// prefix are ignored.
    pub fn new(addr: Ipv6Addr, len: u8) -> Option<Self> {
        embed_positions(len)?;
        let mut o = addr.octets();
        for (i, b) in o.iter_mut().enumerate() {
            if i * 8 >= len as usize {
                *b = 0;
            }
        }
        Some(Self {
            addr: Ipv6Addr::from(o),
            len,
        })
    }

    /// The prefix length in bits.
    pub fn len(&self) -> u8 {
        self.len
    }

    /// Always false: a prefix has one of the RFC 6052 lengths.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// The IPv6 address that carries `v4` under this prefix.
    pub fn embed(&self, v4: Ipv4Addr) -> Ipv6Addr {
        let mut o = self.addr.octets();
        if let Some(pos) = embed_positions(self.len) {
            for (p, b) in pos.iter().zip(v4.octets()) {
                o[*p] = b;
            }
        }
        Ipv6Addr::from(o)
    }

    /// The IPv4 address embedded in `v6`, if `v6` lies under this prefix.
    pub fn extract(&self, v6: Ipv6Addr) -> Option<Ipv4Addr> {
        let pos = embed_positions(self.len)?;
        let o = v6.octets();
        let p = self.addr.octets();
        let prefix_bytes = self.len as usize / 8;
        if o[..prefix_bytes] != p[..prefix_bytes] {
            return None;
        }
        Some(Ipv4Addr::new(o[pos[0]], o[pos[1]], o[pos[2]], o[pos[3]]))
    }
}

impl std::str::FromStr for Nat64Prefix {
    type Err = String;
    /// Parse `addr/len`, e.g. `64:ff9b::/96`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (a, l) = s
            .split_once('/')
            .ok_or_else(|| format!("invalid NAT64 prefix '{s}': expected addr/len"))?;
        let addr: Ipv6Addr = a
            .parse()
            .map_err(|e| format!("invalid NAT64 prefix '{s}': {e}"))?;
        let len: u8 = l
            .parse()
            .map_err(|e| format!("invalid NAT64 prefix '{s}': {e}"))?;
        Self::new(addr, len).ok_or_else(|| {
            format!("invalid NAT64 prefix '{s}': length must be 32, 40, 48, 56, 64 or 96")
        })
    }
}

impl std::fmt::Display for Nat64Prefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.addr, self.len)
    }
}

/// The prefixes under which an `ipv4only.arpa` answer embeds one of its well-known IPv4 addresses
/// (RFC 7050 §3). Each address is tried at every RFC 6052 length.
pub fn prefixes_from_ipv4only_answer(answer: &[Ipv6Addr]) -> Vec<Nat64Prefix> {
    let mut found = Vec::new();
    for a in answer {
        for len in PREFIX_LENGTHS {
            if let Some(p) = Nat64Prefix::new(*a, len)
                && let Some(v4) = p.extract(*a)
                && WELL_KNOWN_IPV4.contains(&v4)
                && !found.contains(&p)
            {
                found.push(p);
                break;
            }
        }
    }
    found
}

/// Name resolution for NAT64 prefix discovery, injectable so tests don't depend on the network.
pub trait Nat64Resolver: Send + Sync {
    /// The IPv6 addresses `host` resolves to through the system resolver.
    fn aaaa(&self, host: &str) -> Vec<Ipv6Addr>;
    /// The address the platform itself synthesises for an IPv4 literal, where it does (Apple's
    /// `getaddrinfo` with `AI_DEFAULT` on a NAT64 network); `None` elsewhere.
    fn synthesise(&self, v4: Ipv4Addr) -> Option<Ipv6Addr>;
}

/// The system resolver: `getaddrinfo` through std for names, and on Apple platforms `getaddrinfo`
/// with `AI_DEFAULT` for literals.
pub struct SystemNat64Resolver;

impl Nat64Resolver for SystemNat64Resolver {
    fn aaaa(&self, host: &str) -> Vec<Ipv6Addr> {
        match (host, 0).to_socket_addrs() {
            Ok(it) => it
                .filter_map(|a| match a.ip() {
                    IpAddr::V6(v6) if v6.to_ipv4_mapped().is_none() => Some(v6),
                    _ => None,
                })
                .collect(),
            Err(e) => {
                tracing::debug!("[nat64] resolving {host} failed: {e}");
                Vec::new()
            }
        }
    }

    #[cfg(target_vendor = "apple")]
    fn synthesise(&self, v4: Ipv4Addr) -> Option<Ipv6Addr> {
        apple_synthesise(v4)
    }

    #[cfg(not(target_vendor = "apple"))]
    fn synthesise(&self, _v4: Ipv4Addr) -> Option<Ipv6Addr> {
        None
    }
}

/// Apple's documented path for IPv4 literals on NAT64 networks: `getaddrinfo` on the dotted-quad
/// string with `AI_DEFAULT` returns the synthesised IPv6 address when the network has NAT64.
#[cfg(target_vendor = "apple")]
fn apple_synthesise(v4: Ipv4Addr) -> Option<Ipv6Addr> {
    use std::ffi::CString;
    let host = CString::new(v4.to_string()).ok()?;
    // SAFETY: zeroed addrinfo is a valid "no hints" value; we set the fields we need.
    let mut hints: libc::addrinfo = unsafe { std::mem::zeroed() };
    hints.ai_family = libc::AF_UNSPEC;
    hints.ai_socktype = libc::SOCK_DGRAM;
    hints.ai_flags = libc::AI_DEFAULT;
    let mut res: *mut libc::addrinfo = std::ptr::null_mut();
    // SAFETY: host and hints outlive the call; res is freed below with freeaddrinfo.
    let rc = unsafe { libc::getaddrinfo(host.as_ptr(), std::ptr::null(), &hints, &mut res) };
    if rc != 0 || res.is_null() {
        return None;
    }
    let mut found = None;
    let mut cur = res;
    while !cur.is_null() {
        // SAFETY: cur is a node of the list getaddrinfo returned.
        let ai = unsafe { &*cur };
        if ai.ai_family == libc::AF_INET6 && !ai.ai_addr.is_null() {
            // SAFETY: an AF_INET6 entry's ai_addr points at a sockaddr_in6.
            let sa = unsafe { &*(ai.ai_addr as *const libc::sockaddr_in6) };
            let v6 = Ipv6Addr::from(sa.sin6_addr.s6_addr);
            if v6.to_ipv4_mapped().is_none() {
                found = Some(v6);
                break;
            }
        }
        cur = ai.ai_next;
    }
    // SAFETY: res came from getaddrinfo and is freed once.
    unsafe { libc::freeaddrinfo(res) };
    found
}

/// The node's NAT64 state: the discovered (or configured) prefixes, and whether this network has
/// shown it has no IPv4 route. Owned by the engine and shared with the UDP mux.
pub struct Nat64 {
    resolver: Box<dyn Nat64Resolver>,
    configured: Option<Nat64Prefix>,
    prefixes: Mutex<Vec<Nat64Prefix>>,
    /// When a send last showed this network has no IPv4 route; `None` when it hasn't, or the
    /// finding was cleared. It holds for `hold`, then IPv4 is tried natively again.
    no_ipv4_route_since: Mutex<Option<Instant>>,
    hold: Duration,
}

impl Nat64 {
    /// NAT64 state using `resolver` for discovery. A `configured` prefix (the `nat64_prefix` start
    /// option, for lab networks without DNS64) replaces discovery.
    pub fn new(resolver: Box<dyn Nat64Resolver>, configured: Option<Nat64Prefix>) -> Self {
        Self {
            resolver,
            configured,
            prefixes: Mutex::new(configured.into_iter().collect()),
            no_ipv4_route_since: Mutex::new(None),
            hold: NO_IPV4_ROUTE_HOLD,
        }
    }

    /// As [`new`](Self::new), with a different hold for the "no IPv4 route" finding (tests).
    pub fn with_no_route_hold(mut self, hold: Duration) -> Self {
        self.hold = hold;
        self
    }

    /// Discover the network's NAT64 prefixes (RFC 7050), replacing any earlier result, and forget
    /// whether IPv4 was routable. Blocking (a DNS lookup): run it off the hot path, at start and on
    /// a network change. Returns the prefixes now in use.
    pub fn rediscover(&self) -> Vec<Nat64Prefix> {
        self.clear_no_ipv4_route();
        let found = if let Some(p) = self.configured {
            vec![p]
        } else {
            let mut found = prefixes_from_ipv4only_answer(&self.resolver.aaaa(IPV4ONLY_ARPA));
            if found.is_empty() {
                // Some networks (and Apple's own synthesis) answer for literals but not the name;
                // learn the prefix from the platform's synthesis of a well-known address.
                let synthesised: Vec<Ipv6Addr> = WELL_KNOWN_IPV4
                    .iter()
                    .filter_map(|v4| self.resolver.synthesise(*v4))
                    .collect();
                found = prefixes_from_ipv4only_answer(&synthesised);
            }
            found
        };
        tracing::info!("[nat64] prefixes: {:?}", found);
        if let Ok(mut p) = self.prefixes.lock() {
            p.clone_from(&found);
        }
        found
    }

    /// The prefixes in use (configured or last discovered).
    pub fn prefixes(&self) -> Vec<Nat64Prefix> {
        self.prefixes.lock().map(|p| p.clone()).unwrap_or_default()
    }

    /// Note that a send to an IPv4 address failed for lack of a route, so IPv4 sends go through
    /// NAT64 for the next [`NO_IPV4_ROUTE_HOLD`]. Only recorded when a NAT64 prefix is known: on a
    /// network without a translator a "no route" error (e.g. a brief Wi-Fi drop) is just a failed
    /// send, and diverting IPv4 would make every later send fail too. Returns whether NAT64 is now
    /// in use for IPv4.
    pub fn mark_no_ipv4_route(&self) -> bool {
        if self.prefixes().is_empty() {
            return false;
        }
        if let Ok(mut since) = self.no_ipv4_route_since.lock() {
            if since.is_none() {
                tracing::info!(
                    "[nat64] no IPv4 route; IPv4 peers go through NAT64 for {:?}",
                    self.hold
                );
            }
            *since = Some(Instant::now());
        }
        true
    }

    /// Forget a "no IPv4 route" finding, so the next IPv4 send tries the native route: on a network
    /// change, or when the socket recovers from a network error.
    pub fn clear_no_ipv4_route(&self) {
        if let Ok(mut since) = self.no_ipv4_route_since.lock() {
            *since = None;
        }
    }

    /// Whether IPv4 sends currently go through NAT64: a "no IPv4 route" finding within the hold
    /// period, with a prefix still known.
    pub fn no_ipv4_route(&self) -> bool {
        let recent = self
            .no_ipv4_route_since
            .lock()
            .ok()
            .and_then(|since| *since)
            .is_some_and(|at| at.elapsed() < self.hold);
        recent && !self.prefixes().is_empty()
    }

    /// The synthesised IPv6 destination for IPv4 peer `to`, under the first prefix in use.
    pub fn synthesise(&self, to: SocketAddr) -> Option<SocketAddr> {
        let SocketAddr::V4(v4) = to else {
            return None;
        };
        let prefix = *self.prefixes.lock().ok()?.first()?;
        Some(SocketAddr::new(
            IpAddr::V6(prefix.embed(*v4.ip())),
            v4.port(),
        ))
    }

    /// The IPv4 peer behind `from`, if `from` is an address synthesised under a prefix in use;
    /// otherwise `from` unchanged.
    pub fn unsynthesise(&self, from: SocketAddr) -> SocketAddr {
        let SocketAddr::V6(v6) = from else {
            return from;
        };
        if let Ok(prefixes) = self.prefixes.lock() {
            for p in prefixes.iter() {
                if let Some(v4) = p.extract(*v6.ip()) {
                    return SocketAddr::new(IpAddr::V4(v4), v6.port());
                }
            }
        }
        from
    }
}

/// Whether a send error means "no route to that address family or host", the signal that this
/// network can't reach IPv4 natively.
pub fn is_no_route_error(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::NetworkUnreachable
            | std::io::ErrorKind::HostUnreachable
            | std::io::ErrorKind::AddrNotAvailable
    ) || matches!(e.raw_os_error(), Some(code) if code == libc::EAFNOSUPPORT)
}
