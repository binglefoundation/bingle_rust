//! Tests for store-and-forward post-on-delivery-fail (epic #200, story #214).
//!
//! The network post to a live Sidewinder node is exercised by the skip-clean
//! `sidewinder_mailbox_e2e` target; here we cover the parts that need no node: the gate and
//! per-recipient idempotency logic, the restart-safe persistence of the posted-set, and that with
//! the send gate off a failed delivery attempts no post.

use bingle_local::api::sidewinder::{pending_forward_recipients, should_forward_send};
use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig, MailboxConfig};
use bingle_test::temp_file_helpers::project_tmp_file_path;
use std::collections::HashSet;

const TEST_MNEMONIC: &str = "square flat curtain negative three april hobby culture unit fit drip bronze cactus stage vault pluck captain nation pond pizza grief domain coin abstract path";

#[test]
fn should_forward_send_requires_gate_and_configuration() {
    assert!(
        should_forward_send(true, true),
        "gate on and a node configured forwards"
    );
    assert!(
        !should_forward_send(true, false),
        "gate on but no node configured does not forward"
    );
    assert!(
        !should_forward_send(false, true),
        "gate off does not forward even with a node"
    );
    assert!(!should_forward_send(false, false));
}

#[test]
fn pending_recipients_excludes_already_forwarded() {
    let recipients = vec!["alice".to_string(), "bob".to_string(), "carol".to_string()];
    let mut forwarded: HashSet<(String, String)> = HashSet::new();
    forwarded.insert(("m100".to_string(), "bob".to_string()));
    // A different message's forward to alice must not mask this message's alice.
    forwarded.insert(("m999".to_string(), "alice".to_string()));

    let pending = pending_forward_recipients("m100", &recipients, &forwarded);
    assert_eq!(
        pending,
        vec!["alice".to_string(), "carol".to_string()],
        "only bob (posted for message 100) is skipped; alice's other-message entry does not count"
    );
}

#[test]
fn all_recipients_pending_when_nothing_forwarded() {
    let recipients = vec!["alice".to_string(), "bob".to_string()];
    let forwarded: HashSet<(String, String)> = HashSet::new();
    assert_eq!(
        pending_forward_recipients("m1", &recipients, &forwarded),
        recipients
    );
}

#[test]
fn no_recipients_pending_when_all_forwarded() {
    let recipients = vec!["alice".to_string(), "bob".to_string()];
    let mut forwarded: HashSet<(String, String)> = HashSet::new();
    forwarded.insert(("m7".to_string(), "alice".to_string()));
    forwarded.insert(("m7".to_string(), "bob".to_string()));
    assert!(pending_forward_recipients("m7", &recipients, &forwarded).is_empty());
}

#[test]
fn forwarded_set_persists_across_save_and_load() {
    // A restart must not re-post an already-forwarded message: the posted-set survives save/load.
    let path = project_tmp_file_path("bingle-local-s-and-f-persist", ".json");
    let path_str = path.to_string_lossy().to_string();

    {
        let api = BingleApiLocalImpl::new(LocalApiConfig::default());
        api.mark_forwarded_for_tests("m111", "alice");
        api.mark_forwarded_for_tests("m111", "bob");
        api.mark_forwarded_for_tests("m222", "carol");
        api.save(&path_str).expect("save state");
    }

    let mut reloaded = BingleApiLocalImpl::new(LocalApiConfig::default());
    reloaded.load(&path_str).expect("load state");
    let restored = reloaded.forwarded_for_tests();

    let expected: HashSet<(String, String)> = [
        ("m111".to_string(), "alice".to_string()),
        ("m111".to_string(), "bob".to_string()),
        ("m222".to_string(), "carol".to_string()),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        restored, expected,
        "the posted-set is restored intact after a restart"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_state_file_without_the_forwarded_field_loads_clean() {
    // Backward compatibility: state written before #214 has no forwarded_messages field.
    let path = project_tmp_file_path("bingle-local-s-and-f-compat", ".json");
    let path_str = path.to_string_lossy().to_string();
    std::fs::write(&path_str, r#"{"keypair":null,"contacts":[],"messages":[]}"#)
        .expect("write legacy state");

    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.load(&path_str).expect("legacy state loads");
    assert!(
        api.forwarded_for_tests().is_empty(),
        "a legacy state file loads with an empty posted-set"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn gate_off_attempts_no_post_on_delivery_failure() {
    // Send gate off (the default): a failed delivery must not post anything, so the posted-set stays
    // empty even though a Sidewinder node is configured.
    let config = LocalApiConfig {
        sidewinder: Some(MailboxConfig::new("http://localhost:9", "tok")),
        store_and_forward_send: false,
        ..LocalApiConfig::default()
    };
    let mut api = BingleApiLocalImpl::new(config);
    api.import_keypair(TEST_MNEMONIC.to_string())
        .expect("import test keypair");
    api.add_message(
        "me".to_string(),
        vec!["alice".to_string()],
        4242,
        "hello".to_string(),
        None,
    )
    .expect("add message");

    api.update_message_status(
        &api.id_of_timestamp_for_tests(4242),
        0.5,
        Some("Recipient unreachable — will keep retrying".to_string()),
        None,
    )
    .expect("update status");

    assert!(
        api.forwarded_for_tests().is_empty(),
        "with the send gate off, a delivery failure posts nothing"
    );
}

#[test]
fn a_fully_forwarded_message_stops_retrying_direct_delivery() {
    // Once a message is in every recipient's Mailbox, direct Bingle delivery must stop retrying: the
    // message is marked complete and its transient failure cleared, so it leaves the pending set.
    // Simulate the "already forwarded" state with the seam so no live node is needed.
    let config = LocalApiConfig {
        sidewinder: Some(MailboxConfig::new("http://localhost:9", "tok")),
        store_and_forward_send: true,
        ..LocalApiConfig::default()
    };
    let mut api = BingleApiLocalImpl::new(config);
    api.import_keypair(TEST_MNEMONIC.to_string())
        .expect("import test keypair");
    let ts = 5150;
    api.add_message(
        "me".to_string(),
        vec!["alice".to_string()],
        ts,
        "already in the mailbox".to_string(),
        None,
    )
    .expect("add message");
    api.mark_forwarded_for_tests(&api.id_of_timestamp_for_tests(ts), "alice");

    // A failed retry: the message is fully forwarded, so it is completed rather than kept pending.
    api.update_message_status(
        &api.id_of_timestamp_for_tests(ts),
        0.5,
        Some("Recipient unreachable — will keep retrying".to_string()),
        None,
    )
    .expect("update status");

    let pending = api.get_pending_messages().expect("pending");
    assert!(
        !pending.iter().any(|m| m.timestamp == ts),
        "a fully-forwarded message is no longer pending (direct retries stop)"
    );
    let stored = api
        .get_messages()
        .expect("messages")
        .into_iter()
        .find(|m| m.timestamp == ts)
        .expect("message present");
    assert_eq!(stored.progress, Some(1.0), "marked complete");
    assert!(
        stored.failure_reason.is_none(),
        "the transient failure is cleared once handed off to store-and-forward"
    );
}

#[test]
fn a_refused_recipient_is_not_treated_as_handed_off() {
    // #305: a recipient whose Mailbox post the node refused terminally is skipped for the rest of
    // the session, but a refusal is not a hand-off, so direct delivery keeps retrying the message.
    let config = LocalApiConfig {
        sidewinder: Some(MailboxConfig::new("http://localhost:9", "tok")),
        store_and_forward_send: true,
        ..LocalApiConfig::default()
    };
    let mut api = BingleApiLocalImpl::new(config);
    api.import_keypair(TEST_MNEMONIC.to_string())
        .expect("import test keypair");
    let ts = 6160;
    api.add_message(
        "me".to_string(),
        vec!["alice".to_string(), "bob".to_string()],
        ts,
        "refused for alice".to_string(),
        None,
    )
    .expect("add message");
    let id = api.id_of_timestamp_for_tests(ts);
    api.mark_forwarded_for_tests(&id, "bob");
    api.mark_forward_refused_for_tests(&id, "alice");

    api.update_message_status(
        &id,
        0.5,
        Some("Recipient unreachable — will keep retrying".to_string()),
        None,
    )
    .expect("update status");

    assert!(
        api.get_pending_messages()
            .expect("pending")
            .iter()
            .any(|m| m.timestamp == ts),
        "a message refused for one recipient stays pending for direct delivery"
    );
    assert!(
        !api.forwarded_for_tests()
            .contains(&(id.clone(), "alice".to_string())),
        "a refused recipient is not recorded as forwarded"
    );
}

#[test]
fn an_identity_refusal_holds_off_mailbox_posts_and_reads() {
    // #309: after the node refuses this client's identity, store-and-forward leaves the Mailbox
    // alone for the back-off window: a failed delivery is not handed off and a poll reads nothing.
    let config = LocalApiConfig {
        sidewinder: Some(MailboxConfig::new("http://localhost:9", "tok")),
        store_and_forward_send: true,
        store_and_forward_receive: true,
        ..LocalApiConfig::default()
    };
    let mut api = BingleApiLocalImpl::new(config);
    api.import_keypair(TEST_MNEMONIC.to_string())
        .expect("import test keypair");
    assert!(
        !api.mailbox_identity_refusal_holds(),
        "no hold-off before a refusal"
    );

    api.mark_mailbox_identity_refused_for_tests(std::time::Instant::now());
    assert!(api.mailbox_identity_refusal_holds());

    let ts = 7170;
    api.add_message(
        "me".to_string(),
        vec!["alice".to_string()],
        ts,
        "held off".to_string(),
        None,
    )
    .expect("add message");
    let id = api.id_of_timestamp_for_tests(ts);
    let started = std::time::Instant::now();
    api.update_message_status(
        &id,
        0.5,
        Some("Recipient unreachable — will keep retrying".to_string()),
        None,
    )
    .expect("update status");
    assert!(
        api.poll_mailbox().expect("poll").is_empty(),
        "a poll reads nothing while held off"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "held off without contacting the node or the chain"
    );

    assert!(
        api.forwarded_for_tests().is_empty(),
        "nothing is recorded as forwarded while held off"
    );
    assert!(
        api.get_pending_messages()
            .expect("pending")
            .iter()
            .any(|m| m.timestamp == ts),
        "the message stays pending for direct delivery"
    );
}
