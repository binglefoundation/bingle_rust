// Issue #498: IPv6 reachability classification. The triangle result (#499) is an input, so both
// outcomes are tested here by injecting it.

use std::net::{Ipv6Addr, SocketAddr};

use bingle_core::engine::nat_v6::classify_nat_v6;
use bingle_core::engine::{Ipv6Triangle, NatTypeV6};
use bingle_core::stun::StunState;
use bingle_core::util::local_addrs::LocalIpv6;

const HOST: Ipv6Addr = Ipv6Addr::new(0x2a01, 0x4f8, 0, 0, 0, 0, 0, 0x42);

fn host_mapped() -> Option<SocketAddr> {
    Some(SocketAddr::new(HOST.into(), 40000))
}

fn routed() -> LocalIpv6 {
    LocalIpv6 {
        addrs: vec![Ipv6Addr::LOCALHOST, HOST],
        route: true,
    }
}

fn unrouted() -> LocalIpv6 {
    LocalIpv6 {
        addrs: vec![Ipv6Addr::LOCALHOST],
        route: false,
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn mapped_equal_to_local_with_a_successful_triangle_is_open() {
    for state in [StunState::Single, StunState::Consistent] {
        assert_eq!(
            classify_nat_v6(state, host_mapped(), &routed(), Ipv6Triangle::Succeeded),
            NatTypeV6::Open
        );
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn mapped_equal_to_local_with_a_failed_triangle_is_open_firewalled() {
    assert_eq!(
        classify_nat_v6(
            StunState::Consistent,
            host_mapped(),
            &routed(),
            Ipv6Triangle::Failed
        ),
        NatTypeV6::OpenFirewalled
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn mapped_equal_to_local_before_any_triangle_is_never_open() {
    assert_eq!(
        classify_nat_v6(
            StunState::Consistent,
            host_mapped(),
            &routed(),
            Ipv6Triangle::Untested
        ),
        NatTypeV6::OpenFirewalled
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_translated_mapped_address_is_never_open() {
    let translated = Some("[2001:db8:ffff::9]:5000".parse().unwrap());
    for triangle in [
        Ipv6Triangle::Untested,
        Ipv6Triangle::Succeeded,
        Ipv6Triangle::Failed,
    ] {
        assert_eq!(
            classify_nat_v6(StunState::Consistent, translated, &routed(), triangle),
            NatTypeV6::OpenFirewalled
        );
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_ipv6_route_is_none() {
    for state in [StunState::None, StunState::Blocked] {
        assert_eq!(
            classify_nat_v6(state, None, &unrouted(), Ipv6Triangle::Untested),
            NatTypeV6::None
        );
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn a_route_with_no_udp_reply_is_no_connection_once_blocked() {
    assert_eq!(
        classify_nat_v6(StunState::Blocked, None, &routed(), Ipv6Triangle::Untested),
        NatTypeV6::NoConnection
    );
    assert_eq!(
        classify_nat_v6(StunState::None, None, &routed(), Ipv6Triangle::Untested),
        NatTypeV6::Unknown,
        "still searching"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn disagreeing_ipv6_servers_are_open_firewalled() {
    assert_eq!(
        classify_nat_v6(
            StunState::Inconsistent,
            None,
            &routed(),
            Ipv6Triangle::Succeeded
        ),
        NatTypeV6::OpenFirewalled
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn the_u8_mapping_round_trips() {
    for v in [
        NatTypeV6::Unknown,
        NatTypeV6::None,
        NatTypeV6::NoConnection,
        NatTypeV6::OpenFirewalled,
        NatTypeV6::Open,
    ] {
        assert_eq!(NatTypeV6::from_u8(v as u8), v);
    }
    assert_eq!(NatTypeV6::from_u8(200), NatTypeV6::Unknown);
}
