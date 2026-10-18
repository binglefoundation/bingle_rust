//! Triangle-test corner exclusion (issue #347): a corner candidate sharing an IP the client has
//! sent to is skipped on any port, except between loopback endpoints (localnet), where the port
//! tells nodes apart.

use std::net::SocketAddr;

use crate::util::test_util::signed_root_relay;
use bingle_core::relay::corner_exclusion::is_excluded_corner;
use bingle_core::relay::relay_finder::{RelayFinderTrait, RelayInfoCache};

fn sa(text: &str) -> SocketAddr {
    text.parse().expect("socket address")
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn public_candidate_on_a_seen_ip_is_excluded_on_any_port() {
    let seen = [sa("203.0.113.5:9000")];
    assert!(is_excluded_corner(sa("203.0.113.5:9000"), &seen));
    assert!(
        is_excluded_corner(sa("203.0.113.5:9001"), &seen),
        "an address-restricted NAT admits any port from a seen IP"
    );
    assert!(!is_excluded_corner(sa("203.0.113.6:9000"), &seen));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn loopback_candidates_are_told_apart_by_port() {
    let seen = [sa("127.0.0.1:9000")];
    assert!(is_excluded_corner(sa("127.0.0.1:9000"), &seen));
    assert!(
        !is_excluded_corner(sa("127.0.0.1:9001"), &seen),
        "localnet nodes all share 127.0.0.1"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn nothing_is_excluded_by_an_empty_list() {
    assert!(!is_excluded_corner(sa("203.0.113.5:9000"), &[]));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ipv6_candidate_on_a_seen_ip_is_excluded_on_any_port() {
    let seen = [sa("[2001:db8::1]:9000")];
    assert!(is_excluded_corner(sa("[2001:db8::1]:9500"), &seen));
    assert!(!is_excluded_corner(sa("[2001:db8::2]:9000"), &seen));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn cache_skips_a_corner_relay_sharing_a_seen_ip() {
    // two relays behind one public IP; the client has sent to one of them
    let cache = RelayInfoCache::new(vec![
        signed_root_relay("RID_A", sa("203.0.113.5:9000")),
        signed_root_relay("RID_B", sa("203.0.113.5:9001")),
        signed_root_relay("RID_C", sa("203.0.113.9:9000")),
    ]);
    let corner = cache
        .find_relay_excluding("SELF", &[sa("203.0.113.5:9000")])
        .expect("a relay on another IP is left");
    assert_eq!(
        corner.id(),
        "RID_C",
        "both relays on the seen IP are skipped"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn cache_has_no_corner_when_every_relay_shares_a_seen_ip() {
    let cache = RelayInfoCache::new(vec![
        signed_root_relay("RID_A", sa("203.0.113.5:9000")),
        signed_root_relay("RID_B", sa("203.0.113.5:9001")),
    ]);
    assert!(
        cache
            .find_relay_excluding("SELF", &[sa("203.0.113.5:9000")])
            .is_err(),
        "no corner node rather than one the client's NAT may already admit"
    );
}
