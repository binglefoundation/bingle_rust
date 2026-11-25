//! InetSocketAddress IPv6 parsing and formatting (issue #493).

use bingle_core::ddb::InetSocketAddress;
use std::convert::TryFrom;
use std::net::SocketAddr;
use std::str::FromStr;

fn inet(host: &str, port: u16) -> InetSocketAddress {
    InetSocketAddress {
        host: host.to_string(),
        port,
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn inet_socket_address_bracketed_ipv6_round_trips() {
    let parsed = InetSocketAddress::from_str("[2001:db8::1]:3478").expect("bracketed IPv6 parses");
    assert_eq!(parsed, inet("2001:db8::1", 3478));
    assert_eq!(parsed.to_string(), "[2001:db8::1]:3478");
    let addr = SocketAddr::try_from(parsed.clone()).expect("converts to SocketAddr");
    assert_eq!(addr, "[2001:db8::1]:3478".parse::<SocketAddr>().unwrap());
    assert_eq!(InetSocketAddress::from(addr), parsed);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn inet_socket_address_unbracketed_ipv6_host_parses() {
    // The pre-#493 Display wrote an IPv6 host without brackets; the port is after the last colon.
    let parsed = InetSocketAddress::from_str("2001:db8::1:3478").expect("unbracketed IPv6 parses");
    assert_eq!(parsed, inet("2001:db8::1", 3478));
    assert_eq!(parsed.to_string(), "[2001:db8::1]:3478");
}

#[test]
#[cfg(not(target_os = "ios"))]
fn inet_socket_address_ipv4_round_trips() {
    let parsed = InetSocketAddress::from_str("1.2.3.4:4433").expect("IPv4 parses");
    assert_eq!(parsed, inet("1.2.3.4", 4433));
    assert_eq!(parsed.to_string(), "1.2.3.4:4433");
    let addr = SocketAddr::try_from(parsed).expect("converts to SocketAddr");
    assert!(addr.is_ipv4());
    assert_eq!(addr, "1.2.3.4:4433".parse::<SocketAddr>().unwrap());
}

#[test]
#[cfg(not(target_os = "ios"))]
fn inet_socket_address_try_from_accepts_ipv6_host_with_or_without_brackets() {
    let want: SocketAddr = "[::1]:4433".parse().unwrap();
    assert_eq!(SocketAddr::try_from(inet("::1", 4433)), Ok(want));
    assert_eq!(SocketAddr::try_from(inet("[::1]", 4433)), Ok(want));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn inet_socket_address_ipv4_mapped_canonicalises_to_ipv4() {
    let want: SocketAddr = "192.0.2.7:3478".parse().unwrap();
    assert_eq!(
        SocketAddr::try_from(inet("::ffff:192.0.2.7", 3478)),
        Ok(want)
    );
    let parsed = InetSocketAddress::from_str("[::ffff:192.0.2.7]:3478").expect("mapped parses");
    assert_eq!(parsed, inet("192.0.2.7", 3478));
    let mapped: SocketAddr = "[::ffff:192.0.2.7]:3478".parse().unwrap();
    assert_eq!(InetSocketAddress::from(mapped), inet("192.0.2.7", 3478));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn inet_socket_address_rejects_non_ip_and_malformed() {
    for text in [
        "relay.example.com:3478",
        "[1.2.3.4]:3478",
        "[2001:db8::1]",
        "2001:db8::1",
        "1.2.3.4",
        "[2001:db8::1]:99999",
        "",
    ] {
        assert!(
            InetSocketAddress::from_str(text).is_err(),
            "should reject {:?}",
            text
        );
    }
    assert!(SocketAddr::try_from(inet("relay.example.com", 3478)).is_err());
    assert!(SocketAddr::try_from(inet("[1.2.3.4]", 3478)).is_err());
}
