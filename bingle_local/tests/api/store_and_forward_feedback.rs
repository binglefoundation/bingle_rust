//! Tests for reporting why a store-and-forward post did not complete (issue #349): the reason is
//! kept on the message beside the send failure, so a device without log access can show it. Uses
//! the refusal and identity hold-off seams, so no Sidewinder node or chain is needed.

use bingle_local::api::{
    BingleApiLocalImpl, BingleLocalApi, LocalApiConfig, MailboxConfig, Message,
    with_store_and_forward_failure,
};

const TEST_MNEMONIC: &str = "square flat curtain negative three april hobby culture unit fit drip bronze cactus stage vault pluck captain nation pond pizza grief domain coin abstract path";
const UNREACHABLE: &str = "Recipient unreachable — will keep retrying";

/// A local store with a Mailbox configured, the send gate as given, and one pending message to
/// `recipients`. Returns the store and the message id.
fn store_with_message(send_gate: bool, recipients: &[&str]) -> (BingleApiLocalImpl, String) {
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
        recipients.iter().map(|r| r.to_string()).collect(),
        9_000,
        "hello".to_string(),
        None,
    )
    .expect("add message");
    let id = api.id_of_timestamp_for_tests(9_000);
    (api, id)
}

fn fail_send(api: &mut BingleApiLocalImpl, id: &str) {
    api.update_message_status(id, 0.0, Some(UNREACHABLE.to_string()), None)
        .expect("update status");
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
fn the_store_and_forward_failure_is_appended_in_brackets() {
    assert_eq!(
        with_store_and_forward_failure(UNREACHABLE, Some("no account for 'bob'")),
        "Recipient unreachable — will keep retrying (store-and-forward failed: no account for 'bob')"
    );
    assert_eq!(
        with_store_and_forward_failure(UNREACHABLE, None),
        UNREACHABLE
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_refused_recipient_is_reported_on_the_message() {
    let (mut api, id) = store_with_message(true, &["alice"]);
    api.mark_forward_refused_for_tests(&id, "alice");

    fail_send(&mut api, &id);

    let m = stored(&api, &id);
    let sf = m
        .store_and_forward_failure
        .clone()
        .expect("a reason is kept");
    assert!(
        sf.contains("refused the post to 'alice'"),
        "names the refused recipient: {sf}"
    );
    assert_eq!(m.failure_reason.as_deref(), Some(UNREACHABLE), "unchanged");
    let shown = m.display_failure_reason().expect("display reason");
    assert!(
        shown.starts_with(UNREACHABLE) && shown.contains("(store-and-forward failed: "),
        "the shown reason carries both: {shown}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn repeated_retries_do_not_accumulate_the_reason() {
    let (mut api, id) = store_with_message(true, &["alice"]);
    api.mark_forward_refused_for_tests(&id, "alice");

    for _ in 0..3 {
        fail_send(&mut api, &id);
    }
    // A progress update between retries also re-runs the post-on-fail hook.
    api.update_message_status(&id, 0.4, None, None)
        .expect("progress");

    let shown = stored(&api, &id)
        .display_failure_reason()
        .expect("display reason");
    assert_eq!(
        shown.matches("store-and-forward failed").count(),
        1,
        "one suffix only: {shown}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn an_identity_refusal_is_reported_on_the_message() {
    let (mut api, id) = store_with_message(true, &["alice"]);
    api.mark_mailbox_identity_refused_for_tests(std::time::Instant::now());

    fail_send(&mut api, &id);

    let sf = stored(&api, &id)
        .store_and_forward_failure
        .expect("a reason is kept");
    assert!(
        sf.contains("refused this account"),
        "names the identity refusal: {sf}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn every_outstanding_recipient_is_reported() {
    let (mut api, id) = store_with_message(true, &["alice", "bob"]);
    api.mark_forward_refused_for_tests(&id, "alice");
    api.mark_forward_refused_for_tests(&id, "bob");

    fail_send(&mut api, &id);

    let sf = stored(&api, &id)
        .store_and_forward_failure
        .expect("a reason is kept");
    assert!(sf.contains("'alice'") && sf.contains("'bob'"), "{sf}");
}

#[test]
#[cfg(not(target_os = "ios"))]
fn with_the_send_gate_off_no_reason_is_kept() {
    let (mut api, id) = store_with_message(false, &["alice"]);
    api.mark_forward_refused_for_tests(&id, "alice");

    fail_send(&mut api, &id);

    let m = stored(&api, &id);
    assert!(m.store_and_forward_failure.is_none());
    assert_eq!(m.display_failure_reason().as_deref(), Some(UNREACHABLE));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn the_reason_is_cleared_when_the_message_is_delivered() {
    let (mut api, id) = store_with_message(true, &["alice"]);
    api.mark_forward_refused_for_tests(&id, "alice");
    fail_send(&mut api, &id);
    assert!(stored(&api, &id).store_and_forward_failure.is_some());

    api.update_message_status(&id, 1.0, None, None)
        .expect("delivered");

    let m = stored(&api, &id);
    assert!(m.store_and_forward_failure.is_none());
    assert!(m.display_failure_reason().is_none());
}

#[test]
#[cfg(not(target_os = "ios"))]
fn the_reason_is_cleared_when_the_message_is_handed_off() {
    let (mut api, id) = store_with_message(true, &["alice", "bob"]);
    api.mark_forwarded_for_tests(&id, "bob");
    api.mark_forward_refused_for_tests(&id, "alice");
    fail_send(&mut api, &id);
    assert!(stored(&api, &id).store_and_forward_failure.is_some());

    // A later post to alice succeeds (simulated with the seam): the message is handed off.
    api.mark_forwarded_for_tests(&id, "alice");
    fail_send(&mut api, &id);

    let m = stored(&api, &id);
    assert_eq!(m.progress, Some(1.0), "handed off");
    assert!(m.store_and_forward_failure.is_none());
    assert!(m.display_failure_reason().is_none());
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_message_stored_without_the_field_loads() {
    let json = r#"{"id":"ab","sender_handle":"me","recipient_handles":["bob"],"timestamp":1,"text":"hi","progress":0.0,"failure_reason":"x"}"#;
    let m: Message = serde_json::from_str(json).expect("older message loads");
    assert!(m.store_and_forward_failure.is_none());
    assert_eq!(m.display_failure_reason().as_deref(), Some("x"));
}
