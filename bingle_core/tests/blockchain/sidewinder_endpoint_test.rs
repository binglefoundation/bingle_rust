// Offline unit tests for the Sidewinder endpoint record codec and the shutdown clear guard (issue
// #237). Pure, no network. The codec is byte-compatible with Sidewinder `sw-membership`'s
// `EndpointRecord`; these round-trips and the documented-layout assertion mirror that crate's tests so
// the two repos stay in lock-step on the on-chain form.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

use bingle_core::blockchain::algo_bingle::AlgoBingle;
use bingle_core::blockchain::sidewinder_endpoint::SidewinderEndpointRecord;

fn v4(a: [u8; 4], port: u16) -> SocketAddrV4 {
    SocketAddrV4::new(Ipv4Addr::from(a), port)
}

fn v6(port: u16) -> SocketAddrV6 {
    SocketAddrV6::new(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1), port, 0, 0)
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn round_trips_both_stacks_through_the_base64_form() {
    let record =
        SidewinderEndpointRecord::new(Some(v4([203, 0, 113, 7], 4000)), Some(v6(4001))).unwrap();
    let decoded = SidewinderEndpointRecord::decode(&record.encode()).expect("decode");
    assert_eq!(decoded, record);
    // The ordered socket addresses a client would try: IPv4 first, then IPv6.
    assert_eq!(
        decoded.socket_addrs(),
        vec![
            SocketAddr::V4(v4([203, 0, 113, 7], 4000)),
            SocketAddr::V6(v6(4001)),
        ]
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn round_trips_a_single_stack_each_way() {
    // The ambiguous case the base64 wrapping guards: 127.0.0.1's bytes are valid ASCII.
    let only_v4 = SidewinderEndpointRecord::new(Some(v4([127, 0, 0, 1], 1080)), None).unwrap();
    assert_eq!(
        SidewinderEndpointRecord::decode(&only_v4.encode()),
        Some(only_v4)
    );

    let only_v6 = SidewinderEndpointRecord::new(None, Some(v6(1080))).unwrap();
    assert_eq!(
        SidewinderEndpointRecord::decode(&only_v6.encode()),
        Some(only_v6)
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn the_binary_layout_is_the_documented_compact_form() {
    // This mirrors sw-membership's assertion byte-for-byte; if it ever diverges the two repos have
    // drifted and discovery would break.
    let record = SidewinderEndpointRecord::new(Some(v4([10, 0, 0, 1], 0x0102)), None).unwrap();
    // flags = 0b01 (IPv4 only), then 4 address bytes, then the big-endian port.
    assert_eq!(record.to_bytes(), vec![0b01, 10, 0, 0, 1, 0x01, 0x02]);

    let both = SidewinderEndpointRecord::new(Some(v4([1, 2, 3, 4], 5)), Some(v6(6))).unwrap();
    assert_eq!(both.to_bytes().len(), 1 + 6 + 18); // flags + v4 block + v6 block
    assert_eq!(both.to_bytes()[0], 0b11);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn from_socket_addrs_uses_only_the_matching_family() {
    // A V6 address given as the IPv4 argument (and vice versa) is dropped, not published under the
    // wrong stack.
    let mixed = SidewinderEndpointRecord::from_socket_addrs(
        Some(SocketAddr::V6(v6(9000))),
        Some(SocketAddr::V4(v4([8, 8, 8, 8], 9001))),
    );
    assert_eq!(mixed, None); // neither argument yields an endpoint of its own family.

    let ok = SidewinderEndpointRecord::from_socket_addrs(
        Some(SocketAddr::V4(v4([8, 8, 8, 8], 53))),
        None,
    )
    .unwrap();
    assert_eq!(ok.v4, Some(v4([8, 8, 8, 8], 53)));
    assert_eq!(ok.v6, None);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn an_empty_record_has_no_endpoint_and_is_rejected() {
    assert_eq!(SidewinderEndpointRecord::new(None, None), None);
    assert_eq!(
        SidewinderEndpointRecord::from_socket_addrs(None, None),
        None
    );
    // A flags-only blob (no stacks) decodes to nothing.
    assert_eq!(SidewinderEndpointRecord::from_bytes(&[0b00]), None);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn malformed_records_are_rejected() {
    assert_eq!(SidewinderEndpointRecord::from_bytes(&[]), None); // empty
    assert_eq!(
        SidewinderEndpointRecord::from_bytes(&[0b01, 10, 0, 0]),
        None
    ); // truncated v4 block
    assert_eq!(
        SidewinderEndpointRecord::from_bytes(&[0b01, 10, 0, 0, 1, 0, 0, 99]),
        None
    ); // trailing byte
    assert_eq!(
        SidewinderEndpointRecord::from_bytes(&[0b100, 1, 2, 3]),
        None
    ); // unknown flag bit
    assert_eq!(SidewinderEndpointRecord::decode("not base64!!"), None);
}

// --- should_clear_sidewinder_endpoint: the shutdown redeploy-race guard -----------------------

#[test]
#[cfg(not(target_os = "ios"))]
pub fn clears_when_on_chain_record_matches_ours() {
    let ours = SidewinderEndpointRecord::new(Some(v4([1, 2, 3, 4], 5000)), None)
        .unwrap()
        .encode();
    assert!(AlgoBingle::should_clear_sidewinder_endpoint(
        Some(&ours),
        Some(&ours),
    ));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn skips_when_on_chain_record_differs_from_ours() {
    let ours = SidewinderEndpointRecord::new(Some(v4([1, 2, 3, 4], 5000)), None)
        .unwrap()
        .encode();
    let theirs = SidewinderEndpointRecord::new(Some(v4([1, 2, 3, 4], 6000)), None)
        .unwrap()
        .encode();
    // A replacement task published a newer record; do not clear it.
    assert!(!AlgoBingle::should_clear_sidewinder_endpoint(
        Some(&ours),
        Some(&theirs),
    ));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn skips_when_this_process_never_registered() {
    let current = SidewinderEndpointRecord::new(Some(v4([1, 2, 3, 4], 5000)), None)
        .unwrap()
        .encode();
    assert!(!AlgoBingle::should_clear_sidewinder_endpoint(
        None,
        Some(&current),
    ));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn skips_when_no_record_is_on_chain() {
    let ours = SidewinderEndpointRecord::new(Some(v4([1, 2, 3, 4], 5000)), None)
        .unwrap()
        .encode();
    assert!(!AlgoBingle::should_clear_sidewinder_endpoint(
        Some(&ours),
        None,
    ));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn skips_when_neither_side_has_a_record() {
    assert!(!AlgoBingle::should_clear_sidewinder_endpoint(None, None));
}
