// Issue #498: the host's own IPv6 addresses, used to tell a native mapped address from a
// translated one.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use bingle_core::util::local_addrs::{
    LocalAddrSource, LocalIpv6, SystemLocalAddrs, is_global_unicast,
};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn global_unicast_excludes_local_scopes() {
    let global: Ipv6Addr = "2a01:4f8::42".parse().unwrap();
    assert!(is_global_unicast(&global));
    for not_global in [
        "::",
        "::1",
        "fe80::1",
        "fd00::1",
        "ff02::1",
        "::ffff:1.2.3.4",
        "2001:db8::1",
    ] {
        let ip: Ipv6Addr = not_global.parse().unwrap();
        assert!(!is_global_unicast(&ip), "{not_global} is not global");
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn contains_matches_ipv6_only() {
    let host: Ipv6Addr = "2a01:4f8::42".parse().unwrap();
    let local = LocalIpv6 {
        addrs: vec![host],
        route: true,
    };
    assert!(local.contains(&IpAddr::V6(host)));
    assert!(!local.contains(&IpAddr::V6("2a01:4f8::43".parse().unwrap())));
    assert!(!local.contains(&IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn the_system_source_reports_a_global_address_whenever_it_has_a_route() {
    let local = SystemLocalAddrs.local_ipv6();
    if local.route {
        assert!(
            local.addrs.iter().any(is_global_unicast),
            "a route implies a global source address: {local:?}"
        );
    }
}
