use bingle_core::util::config_utils::parse_stun_list;
use std::net::{Ipv4Addr, SocketAddr};

#[test]
#[cfg(not(target_os = "ios"))]
fn parse_stun_list_takes_ipv4() {
    let result = parse_stun_list("127.0.0.1:3478").unwrap();
    assert_eq!(
        result,
        vec![SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 3478)]
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn parse_stun_list_keeps_direct_ipv6() {
    // Both address families are kept (issue #496).
    let result = parse_stun_list("[::1]:3478").unwrap();
    assert_eq!(result, vec!["[::1]:3478".parse::<SocketAddr>().unwrap()]);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn parse_stun_list_mixed_ipv4_ipv6_keeps_order() {
    let result = parse_stun_list("127.0.0.1:3478, [::1]:3478, 192.168.1.1:3478").unwrap();
    let expected: Vec<SocketAddr> = vec![
        "127.0.0.1:3478".parse().unwrap(),
        "[::1]:3478".parse().unwrap(),
        "192.168.1.1:3478".parse().unwrap(),
    ];
    assert_eq!(result, expected);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn parse_stun_list_drops_duplicates() {
    let result = parse_stun_list("127.0.0.1:3478 127.0.0.1:3478 [::ffff:127.0.0.1]:3478").unwrap();
    assert_eq!(
        result,
        vec!["127.0.0.1:3478".parse::<SocketAddr>().unwrap()]
    );
}
