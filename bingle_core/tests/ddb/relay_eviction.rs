// Tests for evicting relays whose on-chain allow_relay permission is withdrawn (issue #120).

use std::sync::Mutex;

use bingle_core::ddb::{
    AdvertRecord, DdbBackend, InMemoryDdbBackend, InetSocketAddress, evict_disallowed_relays,
    relays_to_evict,
};

fn record(id: &str, am_relay: bool) -> AdvertRecord {
    AdvertRecord::new_unsigned(
        id.to_string(),
        Some(InetSocketAddress {
            host: "10.0.0.1".to_string(),
            port: 12121,
        }),
        Some(am_relay),
        None,
        None,
        "1970-01-01T00:00:00Z".to_string(),
    )
}

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn only_a_definite_refusal_evicts() {
    let relays = ids(&["ALLOWED", "DISABLED", "UNKNOWN"]);
    let evict = relays_to_evict(&relays, "SELF", |id| match id {
        "ALLOWED" => Some(true),
        "DISABLED" => Some(false),
        _ => None,
    });
    assert_eq!(evict, ids(&["DISABLED"]));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn own_id_is_never_evicted_or_checked() {
    let relays = ids(&["SELF", "OTHER"]);
    let mut checked = Vec::new();
    let evict = relays_to_evict(&relays, "SELF", |id| {
        checked.push(id.to_string());
        Some(false)
    });
    assert_eq!(evict, ids(&["OTHER"]));
    assert_eq!(checked, ids(&["OTHER"]), "own permission is not re-checked");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn chain_outage_keeps_every_relay() {
    let relays = ids(&["A", "B", "C"]);
    assert!(relays_to_evict(&relays, "SELF", |_| None).is_empty());
}

/// The acceptance test from the issue: an am_relay advert whose account is not allow_relay is
/// omitted from make_epoch_info once evicted, and the permitted relay stays.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn make_epoch_info_omits_a_relay_that_is_not_allow_relay() {
    let backend = Mutex::new(InMemoryDdbBackend::new());
    {
        let mut b = backend.lock().unwrap();
        b.upsert(record("RELAY_ALLOWED", true));
        b.upsert(record("RELAY_DISABLED", true));
    }
    let evicted = evict_disallowed_relays(&backend, "SELF", |id| Some(id == "RELAY_ALLOWED"));
    assert_eq!(evicted, ids(&["RELAY_DISABLED"]));

    let b = backend.lock().unwrap();
    let (relay_ids, endpoints) = b.make_epoch_info();
    assert_eq!(relay_ids, ids(&["RELAY_ALLOWED"]));
    assert_eq!(endpoints.expect("endpoints").len(), 1);
    assert!(b.lookup("RELAY_DISABLED").is_none());
}

/// Only relay adverts are re-checked: a client advert is outside the relay set and is untouched.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn client_adverts_are_not_checked_or_evicted() {
    let backend = Mutex::new(InMemoryDdbBackend::new());
    backend.lock().unwrap().upsert(record("CLIENT", false));
    let mut checked = Vec::new();
    let evicted = evict_disallowed_relays(&backend, "SELF", |id| {
        checked.push(id.to_string());
        Some(false)
    });
    assert!(evicted.is_empty());
    assert!(checked.is_empty());
    assert!(backend.lock().unwrap().lookup("CLIENT").is_some());
}
