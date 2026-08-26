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
    let mut forwarded: HashSet<(i64, String)> = HashSet::new();
    forwarded.insert((100, "bob".to_string()));
    // A different message's forward to alice must not mask this message's alice.
    forwarded.insert((999, "alice".to_string()));

    let pending = pending_forward_recipients(100, &recipients, &forwarded);
    assert_eq!(
        pending,
        vec!["alice".to_string(), "carol".to_string()],
        "only bob (posted for message 100) is skipped; alice's other-message entry does not count"
    );
}

#[test]
fn all_recipients_pending_when_nothing_forwarded() {
    let recipients = vec!["alice".to_string(), "bob".to_string()];
    let forwarded: HashSet<(i64, String)> = HashSet::new();
    assert_eq!(
        pending_forward_recipients(1, &recipients, &forwarded),
        recipients
    );
}

#[test]
fn no_recipients_pending_when_all_forwarded() {
    let recipients = vec!["alice".to_string(), "bob".to_string()];
    let mut forwarded: HashSet<(i64, String)> = HashSet::new();
    forwarded.insert((7, "alice".to_string()));
    forwarded.insert((7, "bob".to_string()));
    assert!(pending_forward_recipients(7, &recipients, &forwarded).is_empty());
}

#[test]
fn forwarded_set_persists_across_save_and_load() {
    // A restart must not re-post an already-forwarded message: the posted-set survives save/load.
    let path = project_tmp_file_path("bingle-local-s-and-f-persist", ".json");
    let path_str = path.to_string_lossy().to_string();

    {
        let api = BingleApiLocalImpl::new(LocalApiConfig::default());
        api.mark_forwarded_for_tests(111, "alice");
        api.mark_forwarded_for_tests(111, "bob");
        api.mark_forwarded_for_tests(222, "carol");
        api.save(&path_str).expect("save state");
    }

    let mut reloaded = BingleApiLocalImpl::new(LocalApiConfig::default());
    reloaded.load(&path_str).expect("load state");
    let restored = reloaded.forwarded_for_tests();

    let expected: HashSet<(i64, String)> = [
        (111, "alice".to_string()),
        (111, "bob".to_string()),
        (222, "carol".to_string()),
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
        4242,
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
