//! Tests for the stable message id (issue #209): ids are generated for local messages, carried
//! over from the envelope for Mailbox messages, given to messages from older state files, and are
//! the key for status updates.

use bingle_core::crypto::sealed_envelope::{
    OpenedMessage, SUITE_HPKE_X25519_HKDF_SHA256_CHACHA20POLY1305,
};
use bingle_local::api::{
    BingleApiLocalImpl, BingleLocalApi, LocalApiConfig, Message, legacy_message_id,
};
use bingle_test::temp_file_helpers::project_tmp_file_path;

fn by_text(api: &BingleApiLocalImpl, text: &str) -> Message {
    api.get_messages()
        .expect("messages")
        .into_iter()
        .find(|m| m.text == text)
        .expect("message present")
}

#[test]
fn local_messages_get_distinct_hex_ids_even_with_equal_timestamps() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.add_message("me".into(), vec!["alice".into()], 7, "one".into(), None)
        .expect("one");
    api.add_message("me".into(), vec!["bob".into()], 7, "two".into(), None)
        .expect("two");

    let (one, two) = (by_text(&api, "one"), by_text(&api, "two"));
    assert_ne!(one.id, two.id);
    for id in [&one.id, &two.id] {
        assert_eq!(id.len(), 32, "16 bytes in hex: {id}");
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()), "hex: {id}");
    }
}

#[test]
fn a_status_update_reaches_only_the_message_with_that_id() {
    // Two messages stored in the same millisecond: keyed by timestamp, an update could hit the
    // wrong one.
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.add_message("me".into(), vec!["alice".into()], 7, "one".into(), None)
        .expect("one");
    api.add_message("me".into(), vec!["bob".into()], 7, "two".into(), None)
        .expect("two");
    let two = by_text(&api, "two");

    api.update_message_status(&two.id, 0.0, Some("unreachable".into()), None)
        .expect("update");

    assert_eq!(
        by_text(&api, "two").failure_reason.as_deref(),
        Some("unreachable")
    );
    assert_eq!(by_text(&api, "one").failure_reason, None);
    assert!(
        api.update_message_status("no-such-id", 1.0, None, None)
            .is_err()
    );
}

#[test]
fn a_mailbox_message_keeps_its_envelope_message_id() {
    let opened = OpenedMessage {
        sender_id: [0x07u8; 32],
        sent_time: 1,
        message_id: [0xabu8; 16],
        text: "from the mailbox".to_string(),
        signature: [0x03u8; 64],
        suite_id: SUITE_HPKE_X25519_HKDF_SHA256_CHACHA20POLY1305,
    };

    let msg = Message::from_opened(&opened, "alice".into(), vec!["me".into()], 2);

    assert_eq!(msg.id, "ab".repeat(16));
}

#[test]
fn ids_survive_save_and_load() {
    let path = project_tmp_file_path("bingle-local-message-id-persist", ".json");
    let path_str = path.to_string_lossy().to_string();
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.add_message("me".into(), vec!["alice".into()], 7, "kept".into(), None)
        .expect("store");
    let id = by_text(&api, "kept").id;
    api.save(&path_str).expect("save");

    let mut reloaded = BingleApiLocalImpl::new(LocalApiConfig::default());
    reloaded.load(&path_str).expect("load");

    assert_eq!(by_text(&reloaded, "kept").id, id);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn messages_and_forwards_from_an_older_state_file_get_stable_legacy_ids() {
    // State written before message ids: no `id` on messages, forwarded entries keyed by timestamp.
    let path = project_tmp_file_path("bingle-local-message-id-legacy", ".json");
    let path_str = path.to_string_lossy().to_string();
    std::fs::write(
        &path_str,
        r#"{"keypair":null,"contacts":[],
            "messages":[{"sender_handle":"me","recipient_handles":["bob"],"timestamp":5,"text":"old","progress":0.0}],
            "forwarded_messages":[{"timestamp":5,"handle":"bob"}]}"#,
    )
    .expect("write legacy state");

    let mut first = BingleApiLocalImpl::new(LocalApiConfig::default());
    first.load(&path_str).expect("load");
    let mut second = BingleApiLocalImpl::new(LocalApiConfig::default());
    second.load(&path_str).expect("load again");

    let id = by_text(&first, "old").id;
    assert_eq!(id, legacy_message_id(5));
    assert_eq!(by_text(&second, "old").id, id, "the same id on every load");
    assert!(
        first
            .forwarded_for_tests()
            .contains(&(id.clone(), "bob".to_string())),
        "the forwarded entry now names the message by its legacy id"
    );
    first
        .update_message_status(&id, 1.0, None, None)
        .expect("update by legacy id");
    assert_eq!(by_text(&first, "old").progress, Some(1.0));
    let _ = std::fs::remove_file(&path);
}
