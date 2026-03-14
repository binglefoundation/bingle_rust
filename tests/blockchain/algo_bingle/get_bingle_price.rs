// tests/blockchain/algo_bingle/get_bingle_price.rs
// Unit-style tests for price extraction logic without requiring a live Algod node.

use rust_comms::algo_bingle::AlgoBingle;

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_extract_bingle_price_ok() {
    let entries = vec![
        ("SomeOtherKey".to_string(), "42".to_string()),
        ("BinglePrice".to_string(), "123456".to_string()),
    ];
    let price = AlgoBingle::extract_bingle_price(&entries);
    assert!(price.is_some(), "Expected Some(price) but got None");
    assert_eq!(price.unwrap(), 123456u64);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_extract_bingle_price_missing() {
    let entries = vec![
        ("Another".to_string(), "1".to_string()),
        ("Yet".to_string(), "2".to_string()),
    ];
    let price = AlgoBingle::extract_bingle_price(&entries);
    assert!(price.is_none(), "Expected None when BinglePrice is absent");
}
