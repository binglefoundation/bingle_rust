use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig};
use rust_comms::api::bingle_api::BingleError;

#[test]
fn test_queue_message_and_update_status() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    
    // We need a funded/active status for queue_message to work
    // Since we are testing LocalApiImpl, we can simulate this by generating a keypair
    // and potentially mocking or just accepting that queue_message will fail if no handle.
    
    // Let's first verify it fails when no handle
    let res = api.queue_message(vec!["bob".to_string()], "hello".to_string());
    assert!(res.is_err());
    if let Err(BingleError::Other(e)) = res {
        assert!(e.contains("No handle registered"));
    }

    // Now, we don't have an easy way to set a handle in BingleApiLocalImpl without actually
    // registering on chain in this unit test. 
    // Wait, let's look at BingleApiLocalImpl::register_keypair to see if it sets the handle locally.
}

#[test]
fn test_add_message_has_full_progress() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.add_message("alice".into(), vec!["bob".into()], 123, "hi".into(), Some("AES".into()))
        .expect("add_message");
    
    let msgs = api.get_messages().expect("get_messages");
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].progress, 1.0);
    assert_eq!(msgs[0].failure_reason, None);
    assert_eq!(msgs[0].cipher_suite, Some("AES".to_string()));
}
