//! Tests for offering pending messages to the Mailbox without waiting for a send to fail (issue
//! #352), which the `bingle_jsi` client does on `backgrounding`. Uses the forwarded and refusal
//! seams, so no Sidewinder node or chain is needed.

use bingle_local::api::{
    BingleApiLocalImpl, BingleLocalApi, DeliveryRoute, LocalApiConfig, MailboxConfig, Message,
};

const TEST_MNEMONIC: &str = "square flat curtain negative three april hobby culture unit fit drip bronze cactus stage vault pluck captain nation pond pizza grief domain coin abstract path";

/// A local store with a Mailbox configured and the send gate as given, holding one pending
/// message to `bob` (no send tried yet). Returns the store and the message id.
fn store_with_pending_message(send_gate: bool) -> (BingleApiLocalImpl, String) {
    let config = LocalApiConfig {
        sidewinder: Some(MailboxConfig::new("http://localhost:9", "tok")),
        store_and_forward_send: send_gate,
        ..LocalApiConfig::default()
    };
    let mut api = BingleApiLocalImpl::new(config);
    api.import_keypair(TEST_MNEMONIC.to_string())
        .expect("import test keypair");
    api.add_message(
        "me".to_string(),
        vec!["bob".to_string()],
        9_100,
        "hello".to_string(),
        None,
    )
    .expect("add message");
    let id = api.id_of_timestamp_for_tests(9_100);
    api.update_message_status(&id, 0.0, None, None)
        .expect("make pending");
    (api, id)
}

fn stored(api: &BingleApiLocalImpl, id: &str) -> Message {
    api.get_messages()
        .expect("messages")
        .into_iter()
        .find(|m| m.id == id)
        .expect("message present")
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_pending_message_posted_for_every_recipient_is_complete_by_store_and_forward() {
    let (api, id) = store_with_pending_message(true);
    api.mark_forwarded_for_tests(&id, "bob");

    assert_eq!(api.forward_pending_to_mailbox().expect("flush"), 1);

    let m = stored(&api, &id);
    assert_eq!(m.progress, Some(1.0), "complete: {m:?}");
    assert_eq!(m.delivery_route, Some(DeliveryRoute::StoreAndForward));
    assert_eq!(m.failure_reason, None);
    assert_eq!(m.store_and_forward_failure, None);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_pending_message_not_posted_stays_pending_with_the_reason() {
    let (api, id) = store_with_pending_message(true);
    api.mark_forward_refused_for_tests(&id, "bob");

    assert_eq!(api.forward_pending_to_mailbox().expect("flush"), 0);

    let m = stored(&api, &id);
    assert_eq!(
        m.progress,
        Some(0.0),
        "still pending for direct retry: {m:?}"
    );
    assert_eq!(m.delivery_route, None);
    let sf = m.store_and_forward_failure.expect("a reason is kept");
    assert!(
        sf.contains("refused the post to 'bob'"),
        "names the recipient: {sf}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn with_the_send_gate_off_nothing_is_offered() {
    let (api, id) = store_with_pending_message(false);
    api.mark_forwarded_for_tests(&id, "bob");

    assert_eq!(api.forward_pending_to_mailbox().expect("flush"), 0);

    let m = stored(&api, &id);
    assert_eq!(m.progress, Some(0.0), "untouched: {m:?}");
    assert_eq!(m.delivery_route, None);
    assert_eq!(m.store_and_forward_failure, None);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_delivered_message_is_not_offered() {
    let (mut api, id) = store_with_pending_message(true);
    api.update_message_status(&id, 1.0, None, None)
        .expect("delivered");
    api.mark_forward_refused_for_tests(&id, "bob");

    assert_eq!(api.forward_pending_to_mailbox().expect("flush"), 0);

    let m = stored(&api, &id);
    assert_eq!(
        m.delivery_route,
        Some(DeliveryRoute::Direct),
        "unchanged: {m:?}"
    );
    assert_eq!(m.store_and_forward_failure, None, "no attempt was made");
}
