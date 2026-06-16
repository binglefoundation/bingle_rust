use std::net::{SocketAddr, Ipv4Addr};
use rust_comms::util::config_utils::parse_stun_list;

#[test]
fn parse_stun_list_takes_first_ipv4() {
    // Test with direct IPv4
    let input = "127.0.0.1:3478";
    let result = parse_stun_list(input).unwrap();
    assert_eq!(result.len(), 1);
    assert!(result[0].is_ipv4());
    assert_eq!(result[0], SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 3478));
}

#[test]
fn parse_stun_list_rejects_direct_ipv6() {
    // Test with direct IPv6 - should be ignored or error if it's the only one
    let input = "[::1]:3478";
    let result = parse_stun_list(input);
    assert!(result.is_err(), "Should reject IPv6 address when it's the only entry");
}

#[test]
fn parse_stun_list_mixed_ipv4_ipv6() {
    let input = "127.0.0.1:3478, [::1]:3478, 192.168.1.1:3478";
    let result = parse_stun_list(input).unwrap();
    // It should take 127.0.0.1 and 192.168.1.1, but skip [::1]
    assert_eq!(result.len(), 2);
    assert!(result[0].is_ipv4());
    assert_eq!(result[0].port(), 3478);
    assert!(result[1].is_ipv4());
    assert_eq!(result[1].port(), 3478);
}

#[test]
fn parse_stun_list_dns_ipv4_ipv6() {
    // localhost usually resolves to both 127.0.0.1 and ::1
    // We want to ensure we only get 127.0.0.1
    let input = "localhost:3478";
    let result = parse_stun_list(input).unwrap();
    assert!(!result.is_empty());
    for addr in result {
        assert!(addr.is_ipv4(), "All resolved addresses should be IPv4, found {:?}", addr);
    }
}
