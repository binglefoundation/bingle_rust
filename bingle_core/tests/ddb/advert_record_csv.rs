use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use bingle_core::ddb::{AdvertRecord, InetSocketAddress};

#[test]
fn test_advert_record_csv_roundtrip() {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let public_key = signing_key.verifying_key();

    // Convert public key to Algorand address
    let pk_bytes: [u8; 32] = public_key.to_bytes();
    let address = bingle_core::blockchain::algo_ops::byte_key_to_address(&pk_bytes).unwrap();

    let record = AdvertRecord::new(
        address.clone(),
        Some(InetSocketAddress {
            host: "127.0.0.1".into(),
            port: 1234,
        }),
        Some(true),
        Some("RELAY_ID".to_string()),
        Some("RELAY_SIG".to_string()),
        "2026-06-16T18:31:00Z".into(),
        &signing_key,
    );

    let csv = record.serialize_csv();
    // Expected format: endpoint,am_relay,relay_id,relay_sig,date,sig

    let deserialized =
        AdvertRecord::deserialize_csv(address.clone(), &csv).expect("Should deserialize");

    assert_eq!(record.id, deserialized.id);
    assert_eq!(record.endpoint, deserialized.endpoint);
    assert_eq!(record.am_relay, deserialized.am_relay);
    assert_eq!(record.relay_id, deserialized.relay_id);
    assert_eq!(record.relay_sig, deserialized.relay_sig);
    assert_eq!(record.date, deserialized.date);
    assert_eq!(record.sig, deserialized.sig);

    assert!(
        deserialized.verify(),
        "Signature should still be valid after CSV roundtrip"
    );
}

#[test]
fn test_advert_record_csv_roundtrip_minimal() {
    let id = "P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA".to_string();
    let record = AdvertRecord::new_unsigned(
        id.clone(),
        None,
        Some(false),
        None,
        None,
        "1970-01-01T00:00:00Z".into(),
    );

    let csv = record.serialize_csv();
    assert_eq!(csv, ",F,,,1970-01-01T00:00:00Z,");

    let deserialized = AdvertRecord::deserialize_csv(id.clone(), &csv).expect("Should deserialize");
    assert_eq!(record, deserialized);
}

#[test]
fn test_advert_record_csv_deserialize_invalid() {
    let id = "ADDR".to_string();
    // Wrong number of parts
    assert!(AdvertRecord::deserialize_csv(id.clone(), "part1,part2").is_none());
    // Invalid boolean
    assert!(AdvertRecord::deserialize_csv(id.clone(), ",X,,,date,sig").is_none());
    // Invalid endpoint
    // Note: deserialize_csv uses InetSocketAddress::from_str(parts[0]).ok()
    // which returns None if invalid, and then the record field becomes None.
    // So it might not fail the whole deserialization if parts[0] is garbage but not empty.
    // Let's check implementation.
    /*
        let endpoint = if parts[0].is_empty() {
            None
        } else {
            InetSocketAddress::from_str(parts[0]).ok()
        };
    */
    // If parts[0] is "invalid", InetSocketAddress::from_str("invalid").ok() is None.
    // So it results in endpoint: None.
}
