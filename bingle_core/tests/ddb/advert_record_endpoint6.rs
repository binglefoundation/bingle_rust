//! AdvertRecord `endpoint6` (issue #493): canonical serialisation, signing, the relay ingest
//! gate and the on-chain CSV form.

use bingle_core::ddb::{AdvertRecord, InetSocketAddress, is_advertisable_ipv6};
use ed25519_dalek::SigningKey;
use std::net::Ipv6Addr;

/// Golden vector produced by the serialiser before `endpoint6` existed (staging a860654c, whose
/// advert_record.rs matches `deployed`): key = [7; 32], endpoint 203.0.113.9:3478, amRelay,
/// relayId "RELAYX", date 2026-06-16T18:31:00Z.
const GOLDEN_JSON: &str = r#"{"id":"5JFGYY7CTRJAVPXVKB5RGLWF7GKUO5VOX27HXESCD3VGSFCG2IWAKDM5YU","endpoint":{"host":"203.0.113.9","port":3478},"amRelay":true,"relayId":"RELAYX","date":"2026-06-16T18:31:00Z","sig":"AAQ+5JtHrC5TzRt/BNjxr7FVvYpjRBFeM7DOJtMx6X7NcK6UetvzTxyrcqU6hsjLXnwPVM+ASVzy2N+WkJBsAw=="}"#;
const GOLDEN_CSV: &str = "203.0.113.9:3478,T,RELAYX,,2026-06-16T18:31:00Z,AAQ+5JtHrC5TzRt/BNjxr7FVvYpjRBFeM7DOJtMx6X7NcK6UetvzTxyrcqU6hsjLXnwPVM+ASVzy2N+WkJBsAw==";
const GOLDEN_ID: &str = "5JFGYY7CTRJAVPXVKB5RGLWF7GKUO5VOX27HXESCD3VGSFCG2IWAKDM5YU";

fn golden_key() -> SigningKey {
    SigningKey::from_bytes(&[7u8; 32])
}

fn inet(host: &str, port: u16) -> InetSocketAddress {
    InetSocketAddress {
        host: host.to_string(),
        port,
    }
}

fn base_record(endpoint6: Option<InetSocketAddress>) -> AdvertRecord {
    AdvertRecord::new_unsigned(
        GOLDEN_ID.to_string(),
        Some(inet("203.0.113.9", 3478)),
        Some(true),
        Some("RELAYX".to_string()),
        None,
        "2026-06-16T18:31:00Z".to_string(),
    )
    .with_endpoint6(endpoint6)
    .signed(&golden_key())
}

#[test]
#[cfg(not(target_os = "ios"))]
fn old_format_golden_record_still_verifies() {
    let rec: AdvertRecord = serde_json::from_str(GOLDEN_JSON).expect("golden parses");
    assert_eq!(rec.endpoint6, None);
    assert!(rec.verify(), "pre-#493 record must still verify");
    assert_eq!(rec.check_ingest(), Ok(()));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn record_without_endpoint6_serialises_and_signs_exactly_as_before() {
    let rec = base_record(None);
    assert_eq!(serde_json::to_string(&rec).unwrap(), GOLDEN_JSON);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn record_with_endpoint6_json_round_trips_and_verifies() {
    let rec = base_record(Some(inet("2001:db8::1", 3478)));
    let json = serde_json::to_string(&rec).unwrap();
    assert!(
        json.contains(r#""endpoint6":{"host":"2001:db8::1","port":3478}"#),
        "{}",
        json
    );
    let back: AdvertRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(back, rec);
    assert!(back.verify());
}

#[test]
#[cfg(not(target_os = "ios"))]
fn signature_covers_endpoint6() {
    let mut rec = base_record(Some(inet("2001:db8::1", 3478)));
    rec.endpoint6 = Some(inet("2001:db8::2", 3478));
    assert!(!rec.verify(), "changing endpoint6 must break the signature");

    // Stripping endpoint6 (as a relay that drops unknown fields would) breaks it too.
    let mut stripped = base_record(Some(inet("2001:db8::1", 3478)));
    stripped.endpoint6 = None;
    assert!(!stripped.verify());
}

#[test]
#[cfg(not(target_os = "ios"))]
fn with_endpoint6_clears_a_stale_signature() {
    let rec = base_record(None).with_endpoint6(Some(inet("2001:db8::1", 3478)));
    assert_eq!(rec.sig, None);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn ingest_accepts_global_unicast_and_loopback_endpoint6() {
    for host in [
        "2001:db8::1",
        "2606:4700::1111",
        "3fff::1",
        "::1",
        "[2001:db8::1]",
    ] {
        let rec = base_record(Some(inet(host, 3478)));
        assert_eq!(rec.check_ingest(), Ok(()), "host {:?}", host);
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn ingest_rejects_non_global_endpoint6() {
    for host in [
        "fe80::1",          // link-local
        "febf::1",          // link-local, top of fe80::/10
        "fc00::1",          // unique local
        "fd12:3456::1",     // unique local
        "ff02::1",          // multicast
        "::ffff:192.0.2.7", // IPv4-mapped
        "::",               // unspecified
        "192.0.2.7",        // not IPv6
        "relay.example.com",
    ] {
        let rec = base_record(Some(inet(host, 3478)));
        assert!(rec.verify(), "signature is fine for {:?}", host);
        assert!(!rec.endpoint6_acceptable(), "host {:?}", host);
        assert!(rec.check_ingest().is_err(), "host {:?}", host);
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn ingest_rejects_a_bad_signature() {
    let mut rec = base_record(None);
    rec.date = "2026-06-17T00:00:00Z".to_string();
    assert!(rec.check_ingest().is_err());
}

#[test]
#[cfg(not(target_os = "ios"))]
fn is_advertisable_ipv6_classifies_ranges() {
    let yes: [Ipv6Addr; 3] = [
        "2001:db8::1".parse().unwrap(),
        "2a00::1".parse().unwrap(),
        "::1".parse().unwrap(),
    ];
    let no: [Ipv6Addr; 4] = [
        "fe80::1".parse().unwrap(),
        "fc00::1".parse().unwrap(),
        "ff05::2".parse().unwrap(),
        "::ffff:1.2.3.4".parse().unwrap(),
    ];
    for ip in yes {
        assert!(is_advertisable_ipv6(&ip), "{}", ip);
    }
    for ip in no {
        assert!(!is_advertisable_ipv6(&ip), "{}", ip);
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn old_six_field_csv_still_parses_and_verifies() {
    let rec = AdvertRecord::deserialize_csv(GOLDEN_ID.to_string(), GOLDEN_CSV)
        .expect("six-field CSV parses");
    assert_eq!(rec.endpoint6, None);
    assert!(rec.verify());
    assert_eq!(rec.serialize_csv(), GOLDEN_CSV);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn csv_round_trips_with_endpoint6() {
    let rec = base_record(Some(inet("2001:db8::1", 3478)));
    let csv = rec.serialize_csv();
    assert!(
        csv.starts_with("203.0.113.9:3478,T,RELAYX,,2026-06-16T18:31:00Z,"),
        "{}",
        csv
    );
    assert!(csv.ends_with(",[2001:db8::1]:3478"), "{}", csv);
    assert_eq!(csv.split(',').count(), 7);
    let back = AdvertRecord::deserialize_csv(GOLDEN_ID.to_string(), &csv).expect("parses");
    assert_eq!(back, rec);
    assert!(back.verify());
}

#[test]
#[cfg(not(target_os = "ios"))]
fn csv_round_trips_ipv6_endpoint() {
    let rec = AdvertRecord::new_unsigned(
        GOLDEN_ID.to_string(),
        Some(inet("2001:db8::7", 3478)),
        Some(false),
        None,
        None,
        "2026-06-16T18:31:00Z".to_string(),
    )
    .signed(&golden_key());
    let csv = rec.serialize_csv();
    assert!(csv.starts_with("[2001:db8::7]:3478,F,"), "{}", csv);
    let back = AdvertRecord::deserialize_csv(GOLDEN_ID.to_string(), &csv).expect("parses");
    assert_eq!(back, rec);
    assert!(back.verify());
}

#[test]
#[cfg(not(target_os = "ios"))]
fn csv_with_unparsable_endpoint6_is_rejected() {
    let csv = format!("{},not-an-address", GOLDEN_CSV);
    assert!(AdvertRecord::deserialize_csv(GOLDEN_ID.to_string(), &csv).is_none());
    let too_many = format!("{},[2001:db8::1]:3478,extra", GOLDEN_CSV);
    assert!(AdvertRecord::deserialize_csv(GOLDEN_ID.to_string(), &too_many).is_none());
}
