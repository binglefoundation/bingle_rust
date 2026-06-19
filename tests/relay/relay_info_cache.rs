use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use rust_comms::engine::RelayState;
use rust_comms::relay::relay_finder::{RelayFinderTrait, RelayInfoCache};
use crate::util::test_util::{signed_root_relay, signed_root_relay_with, signed_non_root_relay};

fn addr(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_info_cache_add_update_delete() {
    let cache = RelayInfoCache::new(vec![signed_root_relay_with(
        "RID1",
        addr(41001),
        Some(RelayState::Starting),
        Some(15),
    )]);

    assert!(cache.add_relay(signed_root_relay_with(
        "RID2",
        addr(41002),
        Some(RelayState::Available),
        Some(30),
    )));
    assert!(!cache.add_relay(signed_root_relay_with(
        "RID2",
        addr(49999),
        Some(RelayState::Off),
        Some(60),
    )));

    assert!(cache.update_relay(signed_root_relay_with(
        "RID2",
        addr(42002),
        Some(RelayState::Loaded),
        Some(45),
    )));
    assert!(!cache.update_relay(signed_root_relay_with(
        "RID3",
        addr(42003),
        Some(RelayState::Available),
        Some(20),
    )));

    let relays = cache.list_all_relays("RID1", true);
    assert_eq!(relays.len(), 2);
    assert_eq!(relays[0].id(), "RID1");
    assert_eq!(relays[1].id(), "RID2");
    assert_eq!(relays[1].address(), addr(42002));
    assert_eq!(relays[1].state, Some(RelayState::Loaded));
    assert_eq!(relays[1].ttl, Some(45));

    assert!(cache.delete_relay("RID2"));
    assert!(!cache.delete_relay("RID2"));
    let remaining = cache.list_all_relays("RID1", true);
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id(), "RID1");
    assert_eq!(remaining[0].ttl, Some(15));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_info_cache_trait_behaviour() {
    let relay_2_addr = addr(51002);
    let cache = RelayInfoCache::new(vec![
        signed_root_relay_with("RID1", addr(51001), Some(RelayState::Available), Some(25)),
        signed_root_relay_with("RID2", relay_2_addr, Some(RelayState::Available), Some(35)),
    ]);

    let relays_without_self = cache.list_all_relays("RID1", false);
    assert_eq!(relays_without_self.len(), 1);
    assert_eq!(relays_without_self[0].id(), "RID2");
    assert_eq!(relays_without_self[0].ttl, Some(35));

    let found = cache
        .find_relay("RID1")
        .expect("find_relay should return a relay other than self");
    assert_eq!(found.id(), "RID2");
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

    cache.clear_state_cache();
    let cleared = cache.list_all_relays("RID1", true);
    assert_eq!(cleared.len(), 2);
    assert_eq!(cleared[0].state, None);
    assert_eq!(cleared[1].state, None);
    assert_eq!(cleared[0].ttl, Some(25));
    assert_eq!(cleared[1].ttl, Some(35));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_info_last_updated_set_on_construction() {
    let before = Instant::now();
    let relay = signed_root_relay("RID1", addr(52001));
    let after = Instant::now();
    assert!(relay.last_updated >= before, "last_updated should be >= before construction");
    assert!(relay.last_updated <= after, "last_updated should be <= after construction");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_info_last_updated_set_on_root_with_construction() {
    let before = Instant::now();
    let relay = signed_root_relay_with("RID1", addr(52002), Some(RelayState::Available), Some(30));
    let after = Instant::now();
    assert!(relay.last_updated >= before);
    assert!(relay.last_updated <= after);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_info_last_updated_set_on_non_root_construction() {
    let before = Instant::now();
    let relay = signed_non_root_relay("RID1", addr(52003));
    let after = Instant::now();
    assert!(relay.last_updated >= before);
    assert!(relay.last_updated <= after);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_info_cache_update_relay_refreshes_last_updated() {
    let cache = RelayInfoCache::new(vec![signed_root_relay_with(
        "RID1",
        addr(53001),
        Some(RelayState::Starting),
        Some(15),
    )]);

    std::thread::sleep(Duration::from_millis(10));

    let original_updated = cache
        .list_all_relays("RID1", true)
        .into_iter()
        .next()
        .expect("relay should exist in cache")
        .last_updated;

    std::thread::sleep(Duration::from_millis(10));
    let before_update = Instant::now();

    let updated = cache.update_relay(signed_root_relay_with(
        "RID1",
        addr(53001),
        Some(RelayState::Available),
        Some(30),
    ));
    assert!(updated, "update_relay should return true for existing id");

    let after_update = Instant::now();
    let stored = cache
        .list_all_relays("RID1", true)
        .into_iter()
        .next()
        .expect("relay should still exist after update")
        .last_updated;

    assert!(stored > original_updated, "last_updated should advance after update_relay");
    assert!(stored >= before_update, "last_updated should be >= before_update instant");
    assert!(stored <= after_update, "last_updated should be <= after_update instant");
}
