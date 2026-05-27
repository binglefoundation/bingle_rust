use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use rust_comms::engine::RelayState;
use rust_comms::relay::relay_finder::{RelayFinderTrait, RelayInfo, RelayInfoCache};

fn addr(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn relay_info_cache_add_update_delete() {
    let cache = RelayInfoCache::new(vec![RelayInfo {
        id: "RID1".into(),
        address: addr(41001),
        state: Some(RelayState::Starting),
        ttl: Some(15),
    }]);

    assert!(cache.add_relay(RelayInfo {
        id: "RID2".into(),
        address: addr(41002),
        state: Some(RelayState::Available),
        ttl: Some(30),
    }));
    assert!(!cache.add_relay(RelayInfo {
        id: "RID2".into(),
        address: addr(49999),
        state: Some(RelayState::Off),
        ttl: Some(60),
    }));

    assert!(cache.update_relay(RelayInfo {
        id: "RID2".into(),
        address: addr(42002),
        state: Some(RelayState::Loaded),
        ttl: Some(45),
    }));
    assert!(!cache.update_relay(RelayInfo {
        id: "RID3".into(),
        address: addr(42003),
        state: Some(RelayState::Available),
        ttl: Some(20),
    }));

    let relays = cache.list_all_relays("RID1", true);
    assert_eq!(relays.len(), 2);
    assert_eq!(relays[0].id, "RID1");
    assert_eq!(relays[1].id, "RID2");
    assert_eq!(relays[1].address, addr(42002));
    assert_eq!(relays[1].state, Some(RelayState::Loaded));
    assert_eq!(relays[1].ttl, Some(45));

    assert!(cache.delete_relay("RID2"));
    assert!(!cache.delete_relay("RID2"));
    let remaining = cache.list_all_relays("RID1", true);
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, "RID1");
    assert_eq!(remaining[0].ttl, Some(15));
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn relay_info_cache_trait_behaviour() {
    let relay_2_addr = addr(51002);
    let cache = RelayInfoCache::new(vec![
        RelayInfo {
            id: "RID1".into(),
            address: addr(51001),
            state: Some(RelayState::Available),
            ttl: Some(25),
        },
        RelayInfo {
            id: "RID2".into(),
            address: relay_2_addr,
            state: Some(RelayState::Available),
            ttl: Some(35),
        },
    ]);

    let relays_without_self = cache.list_all_relays("RID1", false);
    assert_eq!(relays_without_self.len(), 1);
    assert_eq!(relays_without_self[0].id, "RID2");
    assert_eq!(relays_without_self[0].ttl, Some(35));

    let found = cache
        .find_relay("RID1")
        .expect("find_relay should return a relay other than self");
    assert_eq!(found.id, "RID2");
    assert_eq!(found.ttl, Some(35));

    let excluded = cache.find_relay_excluding("RID1", &[relay_2_addr]);
    assert!(excluded.is_err());

    let endpoint = cache
        .lookup_root_id("RID2")
        .expect("lookup_root_id should find RID2");
    let endpoint_addr = endpoint
        .inet_socket_address()
        .expect("lookup_root_id should return direct endpoint");
    assert_eq!(endpoint_addr, relay_2_addr);

    cache.load_relay_states("RID1");
    cache.clear_state_cache();
    let cleared = cache.list_all_relays("RID1", true);
    assert_eq!(cleared.len(), 2);
    assert_eq!(cleared[0].state, None);
    assert_eq!(cleared[1].state, None);
    assert_eq!(cleared[0].ttl, Some(25));
    assert_eq!(cleared[1].ttl, Some(35));
}
