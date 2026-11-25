//! IPv6 reachability classification (issue #498). IPv6 is classified on its own, beside the IPv4
//! `NatType`: a host can be IPv4 `Restricted` and IPv6 `Open`.

use std::net::SocketAddr;

use crate::stun::StunState;
use crate::util::local_addrs::LocalIpv6;

/// IPv6 reachability, reported beside the IPv4 `NatType` (never folded into it: an app already
/// reads IPv4 `Open` as full cone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NatTypeV6 {
    /// Not yet determined.
    Unknown = 0,
    /// No IPv6 route: the host has no global IPv6 address.
    None = 1,
    /// The host has an IPv6 route but no STUN server answered over UDP.
    NoConnection = 2,
    /// The host can connect out over IPv6 but cannot be reached unsolicited: no NAT and a failed
    /// (or not yet passed) IPv6 triangle test, typically a stateful firewall (RFC 6092). No
    /// `endpoint6` is published; the host can still connect directly to `Open` peers.
    OpenFirewalled = 3,
    /// No NAT and the IPv6 triangle test passed: reachable unsolicited at its own address. The
    /// only state that publishes `endpoint6`.
    Open = 4,
}

impl NatTypeV6 {
    /// Decode the value stored by `as u8`; unknown values read as `Unknown`.
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => NatTypeV6::None,
            2 => NatTypeV6::NoConnection,
            3 => NatTypeV6::OpenFirewalled,
            4 => NatTypeV6::Open,
            _ => NatTypeV6::Unknown,
        }
    }

    /// The name the JavaScript Interface (JSI) reports.
    pub fn as_str(&self) -> &'static str {
        match self {
            NatTypeV6::Unknown => "Unknown",
            NatTypeV6::None => "None",
            NatTypeV6::NoConnection => "NoConnection",
            NatTypeV6::OpenFirewalled => "OpenFirewalled",
            NatTypeV6::Open => "Open",
        }
    }
}

/// Outcome of the IPv6 triangle test (#499) for the current IPv6 address: whether a relay could
/// reach the host unsolicited at its mapped address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ipv6Triangle {
    /// Not run yet for this address (also the state until #499 lands).
    Untested,
    Succeeded,
    Failed,
}

/// Classify IPv6 reachability from the IPv6 STUN state and mapped endpoint, the host's own IPv6
/// addresses and the triangle test outcome.
///
/// - Blocked: `NoConnection`, or `None` when the host has no IPv6 route at all.
/// - No reply yet: `Unknown`, or `None` when the host has no IPv6 route.
/// - A mapped address (from one server or several in agreement) equal to a local address means no
///   NAT: `Open` once the triangle test has passed, else `OpenFirewalled`. An untested address is
///   never `Open`.
/// - A mapped address that is not local (a NAT66 or prefix translator), or servers that disagree:
///   `OpenFirewalled`, since the host can connect out but has no address to be reached on.
pub fn classify_nat_v6(
    state: StunState,
    mapped: Option<SocketAddr>,
    local: &LocalIpv6,
    triangle: Ipv6Triangle,
) -> NatTypeV6 {
    match state {
        StunState::Blocked | StunState::None if !local.route => NatTypeV6::None,
        StunState::Blocked => NatTypeV6::NoConnection,
        StunState::None => NatTypeV6::Unknown,
        StunState::Inconsistent => NatTypeV6::OpenFirewalled,
        StunState::Single | StunState::Consistent => match mapped {
            Some(m) if local.contains(&m.ip()) && triangle == Ipv6Triangle::Succeeded => {
                NatTypeV6::Open
            }
            Some(_) => NatTypeV6::OpenFirewalled,
            None => NatTypeV6::Unknown,
        },
    }
}
