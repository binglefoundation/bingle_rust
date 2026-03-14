use rust_comms::ddb::AdvertRecord;
use rust_comms::ddb::InetSocketAddress;

#[cfg_attr(not(target_os = "ios"), test)]
pub fn advert_record_serde_roundtrip() {
    let rec = AdvertRecord {
        id: "SOMEALGOWALLETADDR".to_string(),
        endpoint: Some(InetSocketAddress { host: "1.2.3.4".to_string(), port: 4433 }),
        am_relay: Some(true),
        relay_id: Some("RELAYID".to_string()),
        relay_sig: Some("relsig".to_string()),
        date: "2025-01-01T12:34:56Z".to_string(),
        sig: Some("nodesig".to_string()),
    };
    let json = serde_json::to_string(&rec).unwrap();
    let back: AdvertRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(rec, back);
}
