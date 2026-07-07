use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use bingle_core::api::bingle_api::NetworkEndpoint;
use bingle_core::api::network_endpoint::NetworkEndpointKey;

fn addr(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn direct_endpoint_key_has_only_inet_addr() {
    let ep = NetworkEndpoint::new_direct(addr(12345));
    let key = ep.get_key().expect("direct endpoint should produce a key");
    assert_eq!(key.inet_socket_address, Some(addr(12345)));
    assert_eq!(key.relay_id, None);
    assert_eq!(key.relay_channel, None);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_endpoint_key_contains_both_id_and_channel() {
    let ep = NetworkEndpoint::new_relay("RID123".to_string(), Some(addr(9000)), Some(0x4001));
    let key = ep
        .get_key()
        .expect("relay endpoint should produce a key when channel present");
    assert_eq!(key.inet_socket_address, None);
    assert_eq!(key.relay_id.as_deref(), Some("RID123"));
    assert_eq!(key.relay_channel, Some(0x4001));
}

#[test]
#[cfg(not(target_os = "ios"))]
#[should_panic]
pub fn relay_endpoint_key_panics_if_channel_missing() {
    // Construct a relay endpoint with id but without channel; get_key must panic per requirement
    let ep = NetworkEndpoint::new_relay("RIDNOCH".to_string(), Some(addr(9001)), None);
    let _ = ep.get_key();
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_keys_with_same_id_and_diff_channel_are_distinct() {
    let ep1 = NetworkEndpoint::new_relay("RIDABC".to_string(), Some(addr(9002)), Some(0x4001));
    let ep2 = NetworkEndpoint::new_relay("RIDABC".to_string(), Some(addr(9002)), Some(0x4002));
    let k1: NetworkEndpointKey = ep1.get_key().unwrap();
    let k2: NetworkEndpointKey = ep2.get_key().unwrap();
    assert_ne!(k1, k2, "keys must differ when channel differs");
}
