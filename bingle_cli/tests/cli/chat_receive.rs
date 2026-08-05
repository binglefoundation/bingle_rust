// Unit tests for the chat receive path (bingle_cli::chat_receive::receive_message).
use bingle_cli::chat::parse_chat_args;
use bingle_cli::chat_receive::receive_message;
use bingle_cli::chat_state::ChatState;
use bingle_local::api::bingle_local_api::{BingleLocalApi, ContactSource};
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use serde_json::json;
use tempfile::TempDir;

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// Write a BingleLocal state file registered as `handle`, optionally seeding one existing contact,
/// then build a `ChatState` over it. Returns the loaded state and the temp dir (kept alive).
fn state_registered_as(handle: &str, seed_contact: Option<(&str, &str)>) -> (ChatState, TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut local = BingleApiLocalImpl::new(LocalApiConfig::default());
    local.generate_keypair().expect("keypair");
    local.seed_own_handle_for_tests(handle.to_string());
    if let Some((h, id)) = seed_contact {
        local
            .add_contact(h.to_string(), id.to_string(), ContactSource::Manual)
            .expect("seed contact");
    }
    let path = dir.path().join("state.json").to_string_lossy().into_owned();
    local.save(&path).expect("save");

    let chat_args = parse_chat_args(args(&["--state_file", &path])).expect("parse");
    let state = ChatState::from_chat_args(&chat_args).expect("bridge");
    (state, dir)
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn plaintext_message_prints_persists_and_adds_contact() {
    let (mut state, _dir) = state_registered_as("alice", None);

    let received = receive_message(&mut state, "PEER_BOB", "bob", &json!({ "text": "hello" }))
        .expect("plaintext message should be accepted");
    assert_eq!(received.sender_handle, "bob");
    assert_eq!(received.text, "hello");

    // The sender was recorded as a contact, resolvable for later --to.
    assert_eq!(state.resolve_recipient("bob"), Some("PEER_BOB"));

    // The message was appended with our handle as recipient.
    let messages = state.messages().expect("messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].sender_handle, "bob");
    assert_eq!(messages[0].text, "hello");
    assert_eq!(messages[0].recipient_handles, vec!["alice".to_string()]);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn unknown_sender_handle_falls_back_to_id() {
    let (mut state, _dir) = state_registered_as("alice", None);

    let received = receive_message(&mut state, "PEER_RAW", "", &json!({ "text": "hi" }))
        .expect("plaintext message should be accepted");
    // Falls back to the id as the display handle when the sender handle is unknown.
    assert_eq!(received.sender_handle, "PEER_RAW");
    assert_eq!(received.text, "hi");
    // Contact keyed by id when the handle is unknown.
    assert_eq!(state.resolve_recipient("PEER_RAW"), Some("PEER_RAW"));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn typed_message_is_ignored() {
    let (mut state, _dir) = state_registered_as("alice", None);

    // A protocol message (has app/type) is not chat plaintext.
    let out = receive_message(
        &mut state,
        "PEER_BOB",
        "bob",
        &json!({ "app": "ping", "type": "ping", "text": "probe" }),
    );
    assert_eq!(out, None);
    assert_eq!(state.messages().expect("messages").len(), 0);
    assert!(state.resolve_recipient("bob").is_none());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn message_without_text_is_ignored() {
    let (mut state, _dir) = state_registered_as("alice", None);

    let out = receive_message(&mut state, "PEER_BOB", "bob", &json!({ "foo": "bar" }));
    assert_eq!(out, None);
    assert_eq!(state.messages().expect("messages").len(), 0);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn known_sender_is_not_re_added() {
    // Pre-seed bob as a Manual contact; receiving from bob must not duplicate/downgrade the contact.
    let (mut state, _dir) = state_registered_as("alice", Some(("bob", "PEER_BOB")));

    let received = receive_message(&mut state, "PEER_BOB", "bob", &json!({ "text": "yo" }))
        .expect("plaintext message should be accepted");
    assert_eq!(received.sender_handle, "bob");
    assert_eq!(received.text, "yo");
    // Still exactly one contact for bob, and the message was stored.
    assert_eq!(state.contacts.len(), 1);
    assert_eq!(state.resolve_recipient("bob"), Some("PEER_BOB"));
    assert_eq!(state.messages().expect("messages").len(), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn receive_persists_across_reload() {
    // The state file written on receive should reload with the message and contact intact.
    let dir = tempfile::tempdir().expect("tempdir");
    let mut local = BingleApiLocalImpl::new(LocalApiConfig::default());
    local.generate_keypair().expect("keypair");
    local.seed_own_handle_for_tests("alice".to_string());
    let path = dir.path().join("state.json").to_string_lossy().into_owned();
    local.save(&path).expect("save");

    {
        let chat_args = parse_chat_args(args(&["--state_file", &path])).expect("parse");
        let mut state = ChatState::from_chat_args(&chat_args).expect("bridge");
        receive_message(
            &mut state,
            "PEER_BOB",
            "bob",
            &json!({ "text": "persisted" }),
        );
    }

    // Fresh bridge over the same file.
    let chat_args = parse_chat_args(args(&["--state_file", &path])).expect("parse");
    let reloaded = ChatState::from_chat_args(&chat_args).expect("reload");
    assert_eq!(reloaded.resolve_recipient("bob"), Some("PEER_BOB"));
    let messages = reloaded.messages().expect("messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].text, "persisted");
}
