// tests/blockchain/algo_bingle/handle_lookup.rs
use rust_comms::blockchain::algo_bingle::AlgoBingle;
use serde_json::json;

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_extract_handle_match_found() {
    let app_id = 123;
    let handle = "alice";
    let acct = json!({
        "address": "ADDR1",
        "apps-local-state": [
            {
                "id": 123,
                "key-value": [
                    { "key": "SGFuZGxl", "value": { "bytes": "YWxpY2U=", "type": 1 } }, // Handle: alice
                    { "key": "SGFuZGxlVGltZQ==", "value": { "uint": 1000, "type": 2 } } // HandleTime: 1000
                ]
            }
        ]
    });
    
    let mut matches = Vec::new();
    AlgoBingle::extract_handle_match(&acct, app_id, handle, &mut matches);
    
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].0, "ADDR1");
    assert_eq!(matches[0].1, 1000);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_extract_handle_match_wrong_handle() {
    let app_id = 123;
    let handle = "bob";
    let acct = json!({
        "address": "ADDR1",
        "apps-local-state": [
            {
                "id": 123,
                "key-value": [
                    { "key": "SGFuZGxl", "value": { "bytes": "YWxpY2U=", "type": 1 } },
                    { "key": "SGFuZGxlVGltZQ==", "value": { "uint": 1000, "type": 2 } }
                ]
            }
        ]
    });
    
    let mut matches = Vec::new();
    AlgoBingle::extract_handle_match(&acct, app_id, handle, &mut matches);
    
    assert!(matches.is_empty());
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_pick_oldest_match() {
    let matches = vec![
        ("ADDR2".to_string(), 2000),
        ("ADDR1".to_string(), 1000),
        ("ADDR3".to_string(), 3000),
    ];
    
    let result = AlgoBingle::pick_oldest_match(matches);
    assert_eq!(result, Some("ADDR1".to_string()));
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_extract_handle_match_multiple_apps() {
    let app_id = 123;
    let handle = "alice";
    let acct = json!({
        "address": "ADDR1",
        "apps-local-state": [
            {
                "id": 456,
                "key-value": [
                    { "key": "SGFuZGxl", "value": { "bytes": "Ym9i", "type": 1 } }
                ]
            },
            {
                "id": 123,
                "key-value": [
                    { "key": "SGFuZGxl", "value": { "bytes": "YWxpY2U=", "type": 1 } },
                    { "key": "SGFuZGxlVGltZQ==", "value": { "uint": 1000, "type": 2 } }
                ]
            }
        ]
    });
    
    let mut matches = Vec::new();
    AlgoBingle::extract_handle_match(&acct, app_id, handle, &mut matches);
    
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].0, "ADDR1");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_normalize_handle_lowercase() {
    assert_eq!(AlgoBingle::normalize_handle("Fred123"), "fred123");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_normalize_handle_dots() {
    assert_eq!(AlgoBingle::normalize_handle("James.Jones"), "jamesjones");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_normalize_handle_dashes() {
    assert_eq!(AlgoBingle::normalize_handle("james-jones"), "jamesjones");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_normalize_handle_special_chars() {
    assert_eq!(AlgoBingle::normalize_handle("#user$100"), "user100");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_extract_handle_match_case_insensitive() {
    // Stored as "Alice" (registered form), looked up as "alice" (normalised)
    let app_id = 123;
    let handle = "alice";
    let acct = json!({
        "address": "ADDR1",
        "apps-local-state": [{
            "id": 123,
            "key-value": [
                { "key": "SGFuZGxl", "value": { "bytes": "QWxpY2U=", "type": 1 } },
                { "key": "SGFuZGxlVGltZQ==", "value": { "uint": 1000, "type": 2 } }
            ]
        }]
    });
    let mut matches = Vec::new();
    AlgoBingle::extract_handle_match(&acct, app_id, handle, &mut matches);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].0, "ADDR1");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_extract_handle_match_with_dots_in_stored() {
    // Stored as "james.jones", looked up as "jamesjones"
    let app_id = 123;
    let handle = "jamesjones";
    // base64("james.jones") = "amFtZXMuam9uZXM="
    let acct = json!({
        "address": "ADDR1",
        "apps-local-state": [{
            "id": 123,
            "key-value": [
                { "key": "SGFuZGxl", "value": { "bytes": "amFtZXMuam9uZXM=", "type": 1 } },
                { "key": "SGFuZGxlVGltZQ==", "value": { "uint": 1000, "type": 2 } }
            ]
        }]
    });
    let mut matches = Vec::new();
    AlgoBingle::extract_handle_match(&acct, app_id, handle, &mut matches);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].0, "ADDR1");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn test_pick_oldest_match_collision() {
    // Two accounts register the same handle in the same block (same timestamp)
    let matches = vec![
        ("ADDR2".to_string(), 1000),
        ("ADDR1".to_string(), 1000),
    ];
    
    let result = AlgoBingle::pick_oldest_match(matches.clone());
    
    // Now pick_oldest_match tie-breaks by address if timestamps are equal.
    // "ADDR1" < "ADDR2", so "ADDR1" should be picked regardless of input order.
    assert_eq!(result, Some("ADDR1".to_string()));
    
    // If the order was different:
    let matches_rev = vec![
        ("ADDR1".to_string(), 1000),
        ("ADDR2".to_string(), 1000),
    ];
    let result_rev = AlgoBingle::pick_oldest_match(matches_rev);
    assert_eq!(result_rev, Some("ADDR1".to_string()));
}
