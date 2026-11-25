use std::net::{IpAddr, Ipv6Addr, SocketAddr, UdpSocket};
use std::time::Duration;

use bingle_core::stun::stun_endpoint_finder_impl::StunEndpointFinderImpl;
use bingle_core::stun::{SimpleStunServer, SimpleStunStartOptions};

fn hex(s: &str) -> Vec<u8> {
    let s: String = s.split_whitespace().collect();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// RFC 5769 §2.2 sample IPv4 response: SOFTWARE, XOR-MAPPED-ADDRESS, MESSAGE-INTEGRITY, FINGERPRINT.
const RFC5769_IPV4_RESPONSE: &str = "
    0101 003c 2112a442 b7e7a701 bc34d686 fa87dfae
    8022 000b 74657374 20766563 746f7220
    0020 0008 0001a147 e112a643
    0008 0014 2b91f599 fd9e9011 8c7c7158 fadd8d81 b61dd9f2
    8028 0004 c07d4c96";

/// RFC 5769 §2.3 sample IPv6 response.
const RFC5769_IPV6_RESPONSE: &str = "
    0101 0048 2112a442 b7e7a701 bc34d686 fa87dfae
    8022 000b 74657374 20766563 746f7220
    0020 0014 0002a147 0113a9fa a5d3f179 bc25f4b5 bed2b9d9
    0008 0014 a382954e 4be67bf1 1784c97c 8292c275 bfe3ed41
    8028 0004 c8fb0b4c";

#[test]
#[cfg(not(target_os = "ios"))]
pub fn rfc5769_ipv4_vector_parses() {
    let got = StunEndpointFinderImpl::parse_xor_mapped_address(&hex(RFC5769_IPV4_RESPONSE));
    assert_eq!(got, Some("192.0.2.1:32853".parse().unwrap()));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn rfc5769_ipv6_vector_parses() {
    let got = StunEndpointFinderImpl::parse_xor_mapped_address(&hex(RFC5769_IPV6_RESPONSE));
    assert_eq!(
        got,
        Some(
            "[2001:db8:1234:5678:11:2233:4455:6677]:32853"
                .parse()
                .unwrap()
        )
    );
}

/// A Binding response carrying one attribute of `atype` with `value` (padded to 4 bytes).
fn response_with(atype: u16, value: &[u8]) -> Vec<u8> {
    let pad = (4 - value.len() % 4) % 4;
    let mut out = hex("0101 0000 2112a442 b7e7a701 bc34d686 fa87dfae");
    out.extend_from_slice(&atype.to_be_bytes());
    out.extend_from_slice(&(value.len() as u16).to_be_bytes());
    out.extend_from_slice(value);
    out.extend(std::iter::repeat_n(0, pad));
    let len = (out.len() - 20) as u16;
    out[2..4].copy_from_slice(&len.to_be_bytes());
    out
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn plain_mapped_address_parses_in_both_families() {
    // MAPPED-ADDRESS (0x0001) carries the address unmasked.
    let v4 = response_with(0x0001, &hex("0001 0d96 c0000201"));
    assert_eq!(
        StunEndpointFinderImpl::parse_xor_mapped_address(&v4),
        Some("192.0.2.1:3478".parse().unwrap())
    );
    let v6 = response_with(
        0x0001,
        &hex("0002 0d96 20010db8 00000000 00000000 00000001"),
    );
    assert_eq!(
        StunEndpointFinderImpl::parse_xor_mapped_address(&v6),
        Some("[2001:db8::1]:3478".parse().unwrap())
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn malformed_mapped_addresses_are_rejected_without_panic() {
    let cases: &[(&str, Vec<u8>)] = &[
        (
            "IPv4 value too short",
            response_with(0x0020, &hex("0001 a147 e112")),
        ),
        (
            "IPv6 value too short",
            response_with(0x0020, &hex("0002 a147 0113a9fa a5d3f179")),
        ),
        (
            "IPv6 family in an 8-byte value",
            response_with(0x0020, &hex("0002 a147 e112a643")),
        ),
        (
            "unknown family",
            response_with(0x0020, &hex("0003 a147 e112a643")),
        ),
        (
            "value shorter than its header",
            response_with(0x0020, &hex("00")),
        ),
        ("attribute length past the end", {
            let mut r = response_with(
                0x0020,
                &hex("0002 a147 0113a9fa a5d3f179 bc25f4b5 bed2b9d9"),
            );
            r[22..24].copy_from_slice(&0x0040u16.to_be_bytes());
            r
        }),
        ("no magic cookie", {
            let mut r = hex(RFC5769_IPV6_RESPONSE);
            r[4] = 0;
            r
        }),
    ];
    for (label, data) in cases {
        assert_eq!(
            StunEndpointFinderImpl::parse_xor_mapped_address(data),
            None,
            "{label}"
        );
    }
    for len in 0..hex(RFC5769_IPV6_RESPONSE).len() {
        // Every truncation of a valid response is handled without panicking.
        let _ =
            StunEndpointFinderImpl::parse_xor_mapped_address(&hex(RFC5769_IPV6_RESPONSE)[..len]);
    }
}

/// Send a Binding request from `client` to `server` and parse the reply's mapped address.
fn binding_request(client: &UdpSocket, server: SocketAddr) -> Option<SocketAddr> {
    let mut req = hex("0001 0000 2112a442");
    req.extend_from_slice(b"abcdefghijkl");
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client.send_to(&req, server).unwrap();
    let mut buf = [0u8; 512];
    let (n, _) = client.recv_from(&mut buf).ok()?;
    StunEndpointFinderImpl::parse_xor_mapped_address(&buf[..n])
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn simple_server_on_ipv6_loopback_reports_the_ipv6_client() {
    let server = SimpleStunServer::start(SimpleStunStartOptions {
        bind_addr: SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 0),
        attach_to: None,
        broken_nat: false,
    })
    .expect("start IPv6 STUN server");
    let client = UdpSocket::bind("[::1]:0").unwrap();

    let mapped = binding_request(&client, server.local_addr()).expect("mapped address");
    assert_eq!(
        mapped,
        client.local_addr().unwrap(),
        "family 0x02 decodes to the client"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn simple_server_still_reports_ipv4_clients_as_ipv4() {
    let server = SimpleStunServer::start(SimpleStunStartOptions {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        attach_to: None,
        broken_nat: false,
    })
    .expect("start IPv4 STUN server");
    let client = UdpSocket::bind("127.0.0.1:0").unwrap();

    let mapped = binding_request(&client, server.local_addr()).expect("mapped address");
    assert_eq!(mapped, client.local_addr().unwrap());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn broken_nat_perturbs_ipv6_mappings_consistently() {
    let server = SimpleStunServer::start(SimpleStunStartOptions {
        bind_addr: SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 0),
        attach_to: None,
        broken_nat: true,
    })
    .expect("start IPv6 STUN server");
    let client = UdpSocket::bind("[::1]:0").unwrap();

    let first = binding_request(&client, server.local_addr()).expect("mapped address");
    let second = binding_request(&client, server.local_addr()).expect("mapped address");
    assert!(first.is_ipv6());
    assert_ne!(
        first,
        client.local_addr().unwrap(),
        "broken NAT reports a different mapping"
    );
    assert_eq!(first, second, "stable per requester");
}
