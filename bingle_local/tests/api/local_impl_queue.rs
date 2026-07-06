use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig};
use rust_comms::api::bingle_api::BingleError;

#[test]
fn test_queue_message_fails_without_handle() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());

    // Verifying it fails when no handle is registered
    let res = api.queue_message(vec!["bob".to_string()], "hello".to_string());
    assert!(res.is_err());
    if let Err(BingleError::Other(e)) = res {
        assert!(e.contains("No handle registered"));
    }
}

#[test]
fn test_add_message_and_update_status() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let timestamp = 123456789i64;

    api.add_message(
        "alice".into(),
        vec!["bob".into()],
        timestamp,
        "hi".into(),
        None,
    )
    .expect("add_message");

    // Check initial state (add_message sets progress to 1.0)
    let msgs = api.get_messages().expect("get_messages");
    assert_eq!(msgs[0].progress, Some(1.0));

    // Update status
    api.update_message_status(timestamp, 0.5, Some("Sending...".to_string()))
        .expect("update_message_status");

    let msgs = api.get_messages().expect("get_messages");
    assert_eq!(msgs[0].progress, Some(0.5));
    assert_eq!(msgs[0].failure_reason, Some("Sending...".to_string()));

    // Update to success
    api.update_message_status(timestamp, 1.0, None)
        .expect("update_message_status");

    let msgs = api.get_messages().expect("get_messages");
    assert_eq!(msgs[0].progress, Some(1.0));
    assert_eq!(msgs[0].failure_reason, None);
}

#[test]
fn test_get_pending_messages() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());

    api.add_message("alice".into(), vec!["bob".into()], 1, "msg1".into(), None)
        .unwrap();
    api.add_message("alice".into(), vec!["bob".into()], 2, "msg2".into(), None)
        .unwrap();

    // Initially both are 1.0 (since add_message was used)
    assert_eq!(api.get_pending_messages().unwrap().len(), 0);

    // Force one to be pending
    api.update_message_status(1, 0.2, None).unwrap();

    let pending = api.get_pending_messages().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].timestamp, 1);
}
