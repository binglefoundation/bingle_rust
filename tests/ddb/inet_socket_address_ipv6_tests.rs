use rust_comms::ddb::InetSocketAddress;
use std::convert::TryFrom;
use std::net::SocketAddr;
use std::str::FromStr;

#[test]
#[cfg(not(target_os = "ios"))]
fn inet_socket_address_from_str_rejects_ipv6() {
    // IPv6 address
    let s = "[::1]:4433";
    let res = InetSocketAddress::from_str(s);

    // BEFORE FIX: res is Ok(...)
    // AFTER FIX: res should be Err(...)
    assert!(
        res.is_err(),
        "InetSocketAddress::from_str should reject IPv6: {}",
        s
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn inet_socket_address_try_from_rejects_ipv6() {
    // If we have brackets, parse() might succeed and return an IPv6 SocketAddr
    let isa = InetSocketAddress {
        host: "[::1]".to_string(),
        port: 4433,
    };
    let res = SocketAddr::try_from(isa);

    // BEFORE FIX: res might be Ok if host has brackets
    // AFTER FIX: res should be Err
    assert!(
        res.is_err(),
        "SocketAddr::try_from(InetSocketAddress) should reject IPv6"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn inet_socket_address_accepts_ipv4() {
    let s = "1.2.3.4:4433";
    let res = InetSocketAddress::from_str(s);
    assert!(
        res.is_ok(),
        "InetSocketAddress::from_str should accept IPv4: {}",
        s
    );

    let isa = res.unwrap();
    assert_eq!(isa.host, "1.2.3.4");
    assert_eq!(isa.port, 4433);

    let sa = SocketAddr::try_from(isa).expect("should convert back to SocketAddr");
    assert!(sa.is_ipv4());
}
