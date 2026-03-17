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
