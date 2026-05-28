use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use rust_comms::engine::RelayState;
use rust_comms::relay::relay_finder::{RelayFinderTrait, RelayInfo};
use rust_comms::relay::relay_updater::RelayUpdater;

fn addr(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn relay_updater_init_from_blockchain_sets_state_ttl_and_sorts() {
    let updater = RelayUpdater::new(
        "MYID.".to_string(),
        Arc::new(|| {
            vec![
                RelayInfo::root("ZZZ", addr(56003)),
                RelayInfo::root("MYID", addr(56001)),
                RelayInfo::root("AAA", addr(56002)),
            ]
        }),
    );

    updater.init_from_blockchain();

    let cache = updater.relay_info_cache();
    let relays = cache.list_all_relays("MYID", true);
    assert_eq!(relays.len(), 3);
    assert_eq!(relays[0].id, "AAA");
    assert_eq!(relays[1].id, "MYID");
    assert_eq!(relays[2].id, "ZZZ");

    let own = relays
        .iter()
        .find(|relay| relay.id == "MYID")
        .expect("MYID should be present in relay cache");
    assert!(own.is_root);
    assert_eq!(own.state, Some(RelayState::Own));
    assert_eq!(own.ttl, Some(30_000));

    let first_other = relays
        .iter()
        .find(|relay| relay.id == "AAA")
        .expect("AAA should be present in relay cache");
    assert!(first_other.is_root);
    assert_eq!(first_other.state, Some(RelayState::Unknown));
    assert_eq!(first_other.ttl, Some(30));

    let second_other = relays
        .iter()
        .find(|relay| relay.id == "ZZZ")
        .expect("ZZZ should be present in relay cache");
    assert!(second_other.is_root);
    assert_eq!(second_other.state, Some(RelayState::Unknown));
    assert_eq!(second_other.ttl, Some(30));
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn relay_updater_init_from_blockchain_sets_unknown_when_own_not_found() {
    let updater = RelayUpdater::new(
        "MISSING.".to_string(),
        Arc::new(|| {
            vec![
                RelayInfo::root("ROOT1", addr(56101)),
                RelayInfo::root("ROOT2", addr(56102)),
            ]
        }),
    );

    updater.init_from_blockchain();

    let cache = updater.relay_info_cache();
    let relays = cache.list_all_relays("MISSING", true);
    assert_eq!(relays.len(), 2);
    for relay in relays {
        assert!(relay.is_root);
        assert_eq!(relay.state, Some(RelayState::Unknown));
        assert_eq!(relay.ttl, Some(30));
    }
}