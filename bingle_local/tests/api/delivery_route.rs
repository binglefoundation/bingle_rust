//! Tests for the delivery route recorded on each stored message (issue #291): direct for a message
//! sent or received over a live session, store-and-forward for one posted to or read from a
//! Sidewinder Mailbox, and none while a send is pending or after it failed.
//!
//! The Mailbox post itself needs a live node, so a forwarded send is simulated with the
//! already-posted test seam, as in `post_on_delivery_fail`. The shared pending sender's paths are
//! covered in `pending_sender`.

use bingle_core::api::bingle_api::SendFailureKind;
use bingle_core::crypto::sealed_envelope::{
    OpenedMessage, SUITE_HPKE_X25519_HKDF_SHA256_CHACHA20POLY1305,
};
use bingle_local::api::{
    BingleApiLocalImpl, BingleLocalApi, DeliveryRoute, LocalApiConfig, MailboxConfig, Message,
};
use bingle_test::temp_file_helpers::project_tmp_file_path;

const UNREACHABLE: &str = "Recipient unreachable — will keep retrying";

/// A local store with the store-and-forward send gate on and a (never contacted) node configured.
fn forwarding_store() -> BingleApiLocalImpl {
    BingleApiLocalImpl::new(LocalApiConfig {
        sidewinder: Some(MailboxConfig::new("http://localhost:9", "tok")),
        store_and_forward_send: true,
        ..LocalApiConfig::default()
    })
}

/// Store an outbound message to `recipients` and mark it pending, as the clients' send paths do.
fn queue_pending(api: &mut BingleApiLocalImpl, timestamp: i64, recipients: &[&str]) {
    api.add_message(
        "me".to_string(),
        recipients.iter().map(|r| r.to_string()).collect(),
        timestamp,
        "hello".to_string(),
        None,
    )
    .expect("add message");
    api.update_message_status(&api.id_of_timestamp_for_tests(timestamp), 0.0, None, None)
        .expect("mark pending");
}

fn route_of(api: &BingleApiLocalImpl, timestamp: i64) -> Option<DeliveryRoute> {
    api.get_messages()
        .expect("messages")
        .into_iter()
        .find(|m| m.timestamp == timestamp)
        .expect("message present")
        .delivery_route
}

#[test]
fn a_message_received_over_a_live_session_is_direct() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.add_message(
        "alice".to_string(),
        vec!["me".to_string()],
        1,
        "hi".to_string(),
        Some("TLS_AES_256_GCM_SHA384".to_string()),
    )
    .expect("add message");

    assert_eq!(route_of(&api, 1), Some(DeliveryRoute::Direct));
}

#[test]
fn a_message_read_from_the_mailbox_is_store_and_forward() {
    let opened = OpenedMessage {
        sender_id: [0x07u8; 32],
        sent_time: 1_700_000_000_123,
        message_id: [0x09u8; 16],
        text: "hello from the mailbox".to_string(),
        signature: [0x03u8; 64],
        suite_id: SUITE_HPKE_X25519_HKDF_SHA256_CHACHA20POLY1305,
    };

    let msg = Message::from_opened(
        &opened,
        "alice".into(),
        vec!["me".into()],
        1_700_000_050_000,
    );

    assert_eq!(msg.delivery_route, Some(DeliveryRoute::StoreAndForward));
}

#[test]
fn a_pending_send_has_no_route() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    queue_pending(&mut api, 2, &["alice"]);
    assert_eq!(route_of(&api, 2), None);

    // Progress reported part-way through a send does not set one either.
    api.update_message_status(&api.id_of_timestamp_for_tests(2), 0.5, None, None)
        .expect("progress");
    assert_eq!(route_of(&api, 2), None);
}

#[test]
fn a_send_completed_by_direct_delivery_is_direct() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    queue_pending(&mut api, 3, &["alice"]);

    api.update_message_status(&api.id_of_timestamp_for_tests(3), 1.0, None, None)
        .expect("delivered");

    assert_eq!(route_of(&api, 3), Some(DeliveryRoute::Direct));
}

#[test]
fn a_failed_send_has_no_route() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());

    // A transient failure: still pending.
    queue_pending(&mut api, 4, &["alice"]);
    api.update_message_status(
        &api.id_of_timestamp_for_tests(4),
        0.0,
        Some(UNREACHABLE.to_string()),
        Some(SendFailureKind::PeerUnreachable),
    )
    .expect("transient failure");
    assert_eq!(route_of(&api, 4), None);

    // A permanent failure: complete, but not delivered.
    queue_pending(&mut api, 5, &["nobody"]);
    api.update_message_status(
        &api.id_of_timestamp_for_tests(5),
        1.0,
        Some("No such handle".to_string()),
        Some(SendFailureKind::HandleNotFound),
    )
    .expect("permanent failure");
    assert_eq!(route_of(&api, 5), None);
}

#[test]
fn a_failure_after_reported_completion_clears_the_route() {
    // The send path can report 100% progress before the send as a whole is found to have failed.
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    queue_pending(&mut api, 6, &["alice"]);
    api.update_message_status(&api.id_of_timestamp_for_tests(6), 1.0, None, None)
        .expect("progress 100%");

    api.update_message_status(
        &api.id_of_timestamp_for_tests(6),
        0.0,
        Some(UNREACHABLE.to_string()),
        Some(SendFailureKind::PeerUnreachable),
    )
    .expect("failure");

    assert_eq!(route_of(&api, 6), None);
}

#[test]
fn a_send_handed_off_to_the_mailbox_is_store_and_forward() {
    // The recipient is already posted (test seam), so recording the failed direct send completes
    // the hand-off without a live node.
    let mut api = forwarding_store();
    queue_pending(&mut api, 7, &["alice"]);
    api.mark_forwarded_for_tests(&api.id_of_timestamp_for_tests(7), "alice");

    api.update_message_status(
        &api.id_of_timestamp_for_tests(7),
        0.0,
        Some(UNREACHABLE.to_string()),
        Some(SendFailureKind::PeerUnreachable),
    )
    .expect("failure");

    let stored = api
        .get_messages()
        .expect("messages")
        .into_iter()
        .find(|m| m.timestamp == 7)
        .expect("message present");
    assert_eq!(stored.progress, Some(1.0));
    assert_eq!(stored.failure_reason, None);
    assert_eq!(stored.delivery_route, Some(DeliveryRoute::StoreAndForward));
}

#[test]
fn a_send_with_any_recipient_reached_through_a_mailbox_is_store_and_forward() {
    // Bob's copy was posted to his Mailbox on an earlier attempt; a later attempt then delivers
    // direct to everyone. The message has one route, and a Mailbox was involved.
    let mut api = forwarding_store();
    queue_pending(&mut api, 8, &["alice", "bob"]);
    api.mark_forwarded_for_tests(&api.id_of_timestamp_for_tests(8), "bob");

    api.update_message_status(&api.id_of_timestamp_for_tests(8), 1.0, None, None)
        .expect("delivered");

    assert_eq!(route_of(&api, 8), Some(DeliveryRoute::StoreAndForward));
}

#[test]
fn another_messages_forward_does_not_change_the_route() {
    let mut api = forwarding_store();
    queue_pending(&mut api, 9, &["alice"]);
    api.mark_forwarded_for_tests("another-message", "alice");

    api.update_message_status(&api.id_of_timestamp_for_tests(9), 1.0, None, None)
        .expect("delivered");

    assert_eq!(route_of(&api, 9), Some(DeliveryRoute::Direct));
}

#[test]
fn the_route_survives_save_and_load() {
    let path = project_tmp_file_path("bingle-local-delivery-route-persist", ".json");
    let path_str = path.to_string_lossy().to_string();

    {
        let mut api = forwarding_store();
        // Direct (received live), store-and-forward (handed off), and none (pending).
        api.add_message("alice".into(), vec!["me".into()], 1, "hi".into(), None)
            .expect("add message");
        queue_pending(&mut api, 2, &["alice"]);
        api.mark_forwarded_for_tests(&api.id_of_timestamp_for_tests(2), "alice");
        api.update_message_status(
            &api.id_of_timestamp_for_tests(2),
            0.0,
            Some(UNREACHABLE.to_string()),
            None,
        )
        .expect("failure");
        queue_pending(&mut api, 3, &["alice"]);
        api.save(&path_str).expect("save state");
    }

    let mut reloaded = BingleApiLocalImpl::new(LocalApiConfig::default());
    reloaded.load(&path_str).expect("load state");

    assert_eq!(route_of(&reloaded, 1), Some(DeliveryRoute::Direct));
    assert_eq!(route_of(&reloaded, 2), Some(DeliveryRoute::StoreAndForward));
    assert_eq!(route_of(&reloaded, 3), None);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_state_file_without_the_route_loads_with_none() {
    // State written by a release before #291: messages carry no delivery_route.
    let path = project_tmp_file_path("bingle-local-delivery-route-compat", ".json");
    let path_str = path.to_string_lossy().to_string();
    std::fs::write(
        &path_str,
        r#"{"keypair":null,"contacts":[],"messages":[
            {"sender_handle":"me","recipient_handles":["alice"],"timestamp":1,"text":"sent","progress":1.0},
            {"sender_handle":"alice","recipient_handles":["me"],"timestamp":2,"text":"from the mailbox","progress":1.0,"sent_time":1,"delivered_time":2,"signature":"AwMDAw=="}
        ]}"#,
    )
    .expect("write legacy state");

    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.load(&path_str).expect("legacy state loads");

    assert_eq!(api.get_messages().expect("messages").len(), 2);
    assert_eq!(route_of(&api, 1), None);
    assert_eq!(route_of(&api, 2), None);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_route_serializes_as_its_name_and_is_omitted_when_none() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.add_message("alice".into(), vec!["me".into()], 1, "hi".into(), None)
        .expect("add message");
    queue_pending(&mut api, 2, &["alice"]);
    let messages = api.get_messages().expect("messages");

    let direct = serde_json::to_value(&messages[0]).expect("serialize");
    assert_eq!(direct["delivery_route"], "Direct");
    let pending = serde_json::to_value(&messages[1]).expect("serialize");
    assert!(pending.get("delivery_route").is_none());

    assert_eq!(
        serde_json::to_value(DeliveryRoute::StoreAndForward).expect("serialize"),
        "StoreAndForward"
    );
}
