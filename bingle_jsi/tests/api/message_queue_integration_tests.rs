use std::time::Duration;
use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::BingleJsiApiImpl;
use bingle_jsi::api::types::BingleJsiConfig;
use rust_comms::api::bingle_api::BingleApiInternal;
use tempfile::tempdir;

#[test]
fn test_message_queue_processing() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("test.json");
    
    let config = BingleJsiConfig {
        handle: Some("testuser".to_string()),
        passphrase: None,
        relay: false,
        static_ip: Some("127.0.0.1:12345".to_string()),
        stun_servers: None,
        stun_servers_file: None,
        node_file: None,
        log_level: Some("debug".to_string()),
        app_id: Some(123),
        asset_id: Some(456),
        handle_cache_expiry_secs: None,
        debug: true,
        local: Some(db_path.to_string_lossy().to_string()),
    };

    let jsi = BingleJsiApiImpl::init(config).unwrap();
    
    // 1. Setup: Generate keypair
    jsi.generate_keypair().unwrap();
    // We bypass funding/registration in test by using debug=true and manually adding a message with status 0.0

    // 2. Add a message and force it to be pending
    let timestamp = 123456789i64;
    jsi.add_message("testuser".to_string(), vec!["recipient".to_string()], timestamp, "Hello".to_string(), None).unwrap();
    jsi.update_message_status(timestamp, 0.0, None).unwrap();

    let messages = jsi.get_messages().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].progress, 0.0);

    // 3. Start the engine
    jsi.start().unwrap();
    assert!(jsi.is_started());

    // 4. Simulate listening state
    jsi.api_for_tests().notify_listening(true, rust_comms::engine::NatType::Restricted);

    // 5. Wait for processing loop to pick it up. 
    // The loop sleeps for 5 seconds, so we need to wait a bit.
    // To speed up tests, we might want to make the sleep interval configurable, but for now 6 seconds.
    
    let mut success = false;
    for _ in 0..15 { // Wait up to 15 seconds
        std::thread::sleep(Duration::from_secs(1));
        let msgs = jsi.get_messages().unwrap();
        if msgs[0].progress > 0.0 || msgs[0].failure_reason.is_some() {
            success = true;
            break;
        }
    }

    assert!(success, "Message progress was not updated");
    
    jsi.stop().unwrap();
}

// We need access to BingleApiImpl to call notify_listening.
// I'll add a helper to BingleJsiApiImpl for tests.
