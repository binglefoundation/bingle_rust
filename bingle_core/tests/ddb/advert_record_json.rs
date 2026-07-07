use bingle_core::ddb::AdvertRecord;
use bingle_core::ddb::InetSocketAddress;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn advert_record_serde_roundtrip() {
    let mut rec = AdvertRecord::new_unsigned(
        "SOMEALGOWALLETADDR".to_string(),
        Some(InetSocketAddress {
            host: "1.2.3.4".to_string(),
            port: 4433,
        }),
        Some(true),
        Some("RELAYID".to_string()),
        Some("relsig".to_string()),
        "2025-01-01T12:34:56Z".to_string(),
    );
    rec.sig = Some("nodesig".to_string());
    let json = serde_json::to_string(&rec).unwrap();
    let back: AdvertRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(rec, back);
}
