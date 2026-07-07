use bingle_core::ddb::{AdvertRecord, DdbBackend, InMemoryDdbBackend, InetSocketAddress};

fn sample_record(id: &str) -> AdvertRecord {
    AdvertRecord::new_unsigned(
        id.to_string(),
        Some(InetSocketAddress {
            host: "127.0.0.1".to_string(),
            port: 4433,
        }),
        Some(false),
        None,
        None,
        "2025-01-01T00:00:00Z".to_string(),
    )
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn upsert_then_lookup_returns_same_record() {
    let mut db = InMemoryDdbBackend::new();
    let rec = sample_record("ID1");
    db.upsert(rec.clone());
    let roundtrip = db.lookup("ID1");
    assert!(
        roundtrip.is_some(),
        "lookup should return Some after upsert"
    );
    assert_eq!(rec, roundtrip.unwrap());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn upsert_updates_existing() {
    let mut db = InMemoryDdbBackend::new();
    let mut rec = sample_record("ID2");
    db.upsert(rec.clone());

    // Update fields and upsert again
    rec.endpoint = Some(InetSocketAddress {
        host: "host".into(),
        port: 5555,
    });
    rec.am_relay = Some(true);
    db.upsert(rec.clone());

    let got = db.lookup("ID2");
    assert!(got.is_some(), "lookup should find updated record");
    assert_eq!(rec, got.unwrap());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn delete_then_lookup_none() {
    let mut db = InMemoryDdbBackend::new();
    db.upsert(sample_record("ID3"));

    db.delete("ID3");
    let none = db.lookup("ID3");
    assert!(none.is_none(), "lookup should be None after delete");
}

/// Helper: create a relay AdvertRecord with a given id and address.
fn relay_record(id: &str, host: &str, port: u16) -> AdvertRecord {
    AdvertRecord::new_unsigned(
        id.to_string(),
        Some(InetSocketAddress {
            host: host.to_string(),
            port,
        }),
        Some(true),
        None,
        None,
        "1970-01-01T00:00:00Z".to_string(),
    )
}

/// When there is already a live relay in the DDB and a second relay is added
/// (simulating the self-upsert after ddb_load_from_peer), both relays must
/// appear in the getRelaysStatus response (make_epoch_info).
#[test]
#[cfg(not(target_os = "ios"))]
pub fn both_relays_listed_in_epoch_after_second_relay_added() {
    let mut db = InMemoryDdbBackend::new();

    // Simulate existing relay already in DDB (the peer we loaded from)
    let relay_a = relay_record("RELAY_A", "10.0.0.1", 12121);
    db.upsert(relay_a);

    // Verify epoch contains only the first relay
    let (ids, endpoints) = db.make_epoch_info();
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0], "RELAY_A");
    let eps = endpoints.expect("endpoints should be present when all relays have one");
    assert_eq!(eps.len(), 1);
    assert_eq!(eps[0].host, "10.0.0.1");
    assert_eq!(eps[0].port, 12121);

    // Simulate the self-upsert that happens after ddb_load_from_peer
    let relay_b = relay_record("RELAY_B", "10.0.0.2", 12122);
    db.upsert(relay_b);

    // Both relays must now appear in the epoch response
    let (ids, endpoints) = db.make_epoch_info();
    assert_eq!(ids.len(), 2, "epoch should list both relays");
    // make_epoch_info sorts by id
    assert_eq!(ids[0], "RELAY_A");
    assert_eq!(ids[1], "RELAY_B");
    let eps = endpoints.expect("endpoints should be present when all relays have one");
    assert_eq!(eps.len(), 2);
    assert_eq!(eps[0].host, "10.0.0.1");
    assert_eq!(eps[0].port, 12121);
    assert_eq!(eps[1].host, "10.0.0.2");
    assert_eq!(eps[1].port, 12122);
}
