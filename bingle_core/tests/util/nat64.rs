use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

use bingle_core::util::nat64::{
    IPV4ONLY_ARPA, Nat64, Nat64Prefix, Nat64Resolver, is_no_route_error,
    prefixes_from_ipv4only_answer,
};

/// RFC 6052 §2.4: 192.0.2.33 embedded under each prefix length of 2001:db8:122:344::/n.
const RFC6052_EXAMPLES: [(&str, &str); 6] = [
    ("2001:db8::/32", "2001:db8:c000:221::"),
    ("2001:db8:100::/40", "2001:db8:1c0:2:21::"),
    ("2001:db8:122::/48", "2001:db8:122:c000:2:2100::"),
    ("2001:db8:122:300::/56", "2001:db8:122:3c0:0:221::"),
    ("2001:db8:122:344::/64", "2001:db8:122:344:c0:2:2100:0"),
    ("2001:db8:122:344::/96", "2001:db8:122:344::c000:221"),
];

#[test]
#[cfg(not(target_os = "ios"))]
pub fn rfc6052_embedding_for_every_prefix_length() {
    let v4 = Ipv4Addr::new(192, 0, 2, 33);
    for (prefix, expected) in RFC6052_EXAMPLES {
        let p: Nat64Prefix = prefix.parse().unwrap();
        let expected: Ipv6Addr = expected.parse().unwrap();
        assert_eq!(p.embed(v4), expected, "embed under {prefix}");
        assert_eq!(p.extract(expected), Some(v4), "extract under {prefix}");
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn well_known_prefix_and_parse_errors() {
    let p: Nat64Prefix = "64:ff9b::/96".parse().unwrap();
    assert_eq!(
        p.embed(Ipv4Addr::new(192, 0, 2, 1)),
        "64:ff9b::c000:201".parse::<Ipv6Addr>().unwrap()
    );
    assert_eq!(
        p.extract("2001:db8::1".parse().unwrap()),
        None,
        "outside the prefix"
    );
    assert_eq!(p.to_string(), "64:ff9b::/96");
    for bad in ["64:ff9b::", "64:ff9b::/80", "nonsense/96", "64:ff9b::/x"] {
        assert!(bad.parse::<Nat64Prefix>().is_err(), "{bad}");
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ipv4only_answer_reveals_the_prefix_at_any_length() {
    for (prefix, _) in RFC6052_EXAMPLES {
        let p: Nat64Prefix = prefix.parse().unwrap();
        let answer = [
            p.embed(Ipv4Addr::new(192, 0, 0, 170)),
            p.embed(Ipv4Addr::new(192, 0, 0, 171)),
        ];
        assert_eq!(prefixes_from_ipv4only_answer(&answer), vec![p], "{prefix}");
    }
    let unrelated: Ipv6Addr = "2001:db8::1".parse().unwrap();
    assert!(prefixes_from_ipv4only_answer(&[unrelated]).is_empty());
}

struct FakeResolver {
    aaaa: Vec<Ipv6Addr>,
    synthesised_prefix: Option<Nat64Prefix>,
}

impl Nat64Resolver for FakeResolver {
    fn aaaa(&self, host: &str) -> Vec<Ipv6Addr> {
        assert_eq!(host, IPV4ONLY_ARPA);
        self.aaaa.clone()
    }
    fn synthesise(&self, v4: Ipv4Addr) -> Option<Ipv6Addr> {
        self.synthesised_prefix.map(|p| p.embed(v4))
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn discovery_translates_both_ways_and_resets_the_route_flag() {
    let p: Nat64Prefix = "64:ff9b::/96".parse().unwrap();
    let n = Nat64::new(
        Box::new(FakeResolver {
            aaaa: vec![p.embed(Ipv4Addr::new(192, 0, 0, 170))],
            synthesised_prefix: None,
        }),
        None,
    );
    assert!(n.prefixes().is_empty(), "nothing before discovery");
    n.mark_no_ipv4_route();
    assert_eq!(n.rediscover(), vec![p]);
    assert!(
        !n.no_ipv4_route(),
        "rediscovery forgets the old network's route state"
    );

    let peer: SocketAddr = "192.0.2.1:3478".parse().unwrap();
    let synthesised: SocketAddr = "[64:ff9b::c000:201]:3478".parse().unwrap();
    assert_eq!(n.synthesise(peer), Some(synthesised));
    assert_eq!(n.unsynthesise(synthesised), peer);
    let other: SocketAddr = "[2001:db8::1]:9".parse().unwrap();
    assert_eq!(n.unsynthesise(other), other, "native IPv6 is left alone");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn discovery_falls_back_to_platform_synthesis() {
    let p: Nat64Prefix = "2001:db8:64::/96".parse().unwrap();
    let n = Nat64::new(
        Box::new(FakeResolver {
            aaaa: vec![],
            synthesised_prefix: Some(p),
        }),
        None,
    );
    assert_eq!(n.rediscover(), vec![p]);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_nat64_network_has_no_prefix() {
    let n = Nat64::new(
        Box::new(FakeResolver {
            aaaa: vec![],
            synthesised_prefix: None,
        }),
        None,
    );
    assert!(n.rediscover().is_empty());
    assert_eq!(n.synthesise("192.0.2.1:1".parse().unwrap()), None);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn configured_prefix_replaces_discovery() {
    let configured: Nat64Prefix = "2001:db8:aaaa::/96".parse().unwrap();
    let discovered: Nat64Prefix = "64:ff9b::/96".parse().unwrap();
    let n = Nat64::new(
        Box::new(FakeResolver {
            aaaa: vec![discovered.embed(Ipv4Addr::new(192, 0, 0, 170))],
            synthesised_prefix: None,
        }),
        Some(configured),
    );
    assert_eq!(
        n.prefixes(),
        vec![configured],
        "in use before discovery runs"
    );
    assert_eq!(n.rediscover(), vec![configured]);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_route_errors_are_recognised() {
    use std::io::{Error, ErrorKind};
    assert!(is_no_route_error(&Error::from(
        ErrorKind::NetworkUnreachable
    )));
    assert!(is_no_route_error(&Error::from(ErrorKind::HostUnreachable)));
    assert!(is_no_route_error(&Error::from(ErrorKind::AddrNotAvailable)));
    assert!(!is_no_route_error(&Error::from(
        ErrorKind::ConnectionRefused
    )));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_route_is_only_recorded_when_a_translator_is_known() {
    let n = Nat64::new(
        Box::new(FakeResolver {
            aaaa: vec![],
            synthesised_prefix: None,
        }),
        None,
    );
    n.rediscover();
    assert!(
        !n.mark_no_ipv4_route(),
        "no prefix: a 'no route' error is just a failed send"
    );
    assert!(
        !n.no_ipv4_route(),
        "IPv4 is not diverted on a network without NAT64"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_route_finding_expires_and_can_be_cleared() {
    let p: Nat64Prefix = "64:ff9b::/96".parse().unwrap();
    let n = Nat64::new(
        Box::new(FakeResolver {
            aaaa: vec![],
            synthesised_prefix: None,
        }),
        Some(p),
    )
    .with_no_route_hold(std::time::Duration::from_millis(50));
    assert!(n.mark_no_ipv4_route());
    assert!(n.no_ipv4_route());
    std::thread::sleep(std::time::Duration::from_millis(80));
    assert!(
        !n.no_ipv4_route(),
        "after the hold, IPv4 is tried natively again"
    );

    assert!(n.mark_no_ipv4_route());
    n.clear_no_ipv4_route();
    assert!(!n.no_ipv4_route(), "cleared on recovery");
}
