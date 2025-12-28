use rust_comms::ddb::{AdvertRecord, DdbBackend, InMemoryDdbBackend, InetSocketAddress};

fn sample_record(id: &str) -> AdvertRecord {
    AdvertRecord {
        id: id.to_string(),
        endpoint: Some(InetSocketAddress { host: "127.0.0.1".to_string(), port: 4433 }),
        am_relay: Some(false),
        relay_id: None,
        relay_sig: None,
        date: "2025-01-01T00:00:00Z".to_string(),
        sig: None,
    }
}

#[test]
fn upsert_then_lookup_returns_same_record() {
    let mut db = InMemoryDdbBackend::new();
    let rec = sample_record("ID1");
    db.upsert(rec.clone());
    let roundtrip = db.lookup("ID1");
    assert!(roundtrip.is_some(), "lookup should return Some after upsert");
    assert_eq!(rec, roundtrip.unwrap());
}

#[test]
fn upsert_updates_existing() {
    let mut db = InMemoryDdbBackend::new();
    let mut rec = sample_record("ID2");
    db.upsert(rec.clone());

    // Update fields and upsert again
    rec.endpoint = Some(InetSocketAddress { host: "host".into(), port: 5555 });
    rec.am_relay = Some(true);
    db.upsert(rec.clone());

    let got = db.lookup("ID2");
    assert!(got.is_some(), "lookup should find updated record");
    assert_eq!(rec, got.unwrap());
}

#[test]
fn delete_then_lookup_none() {
    let mut db = InMemoryDdbBackend::new();
    db.upsert(sample_record("ID3"));

    db.delete("ID3");
    let none = db.lookup("ID3");
    assert!(none.is_none(), "lookup should be None after delete");
}
