//! Tests for ordering messages by send time (issue #69): `Message::order_time`'s fallback chain,
//! `get_messages` in that order, and the pending sender choosing by it.

use std::collections::HashMap;
use std::time::Instant;

use bingle_core::crypto::sealed_envelope::{
    OpenedMessage, SUITE_HPKE_X25519_HKDF_SHA256_CHACHA20POLY1305,
};
use bingle_local::api::{
    BingleApiLocalImpl, BingleLocalApi, LocalApiConfig, Message, select_sendable_message,
};
use serde_json::json;

fn message(timestamp: i64, sent_time: Option<i64>, delivered_time: Option<i64>) -> Message {
    Message {
        sender_handle: "alice".to_string(),
        recipient_handles: vec!["me".to_string()],
        timestamp,
        text: format!("m{timestamp}"),
        cipher_suite: None,
        progress: Some(0.0),
        failure_reason: None,
        failure_kind: None,
        sent_time,
        delivered_time,
        signature: None,
        delivery_route: None,
    }
}

#[test]
fn order_time_prefers_sent_then_delivered_then_timestamp() {
    assert_eq!(message(300, Some(100), Some(200)).order_time(), 100);
    assert_eq!(message(300, None, Some(200)).order_time(), 200);
    assert_eq!(message(300, None, None).order_time(), 300);
}

#[test]
fn get_messages_is_in_send_time_order() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());

    // Stored in arrival order, which is not send order:
    // our own message, queued at 100 (its timestamp is its send time).
    api.add_message("me".into(), vec!["alice".into()], 100, "own".into(), None)
        .expect("own");
    // A signed live message that arrived at 200 but was sent at 50.
    api.add_received_message(
        "alice".into(),
        vec!["me".into()],
        200,
        &json!({ "text": "live", "sent_time": 50, "signature": "AwMDAw==" }),
    )
    .expect("live");
    // An unsigned live message from an older client, arriving at 150.
    api.add_received_message(
        "bob".into(),
        vec!["me".into()],
        150,
        &json!({ "text": "unsigned" }),
    )
    .expect("unsigned");
    // A Mailbox message sent at 75 and read at 300.
    let opened = OpenedMessage {
        sender_id: [0x07u8; 32],
        sent_time: 75,
        message_id: [0x09u8; 16],
        text: "mailbox".to_string(),
        signature: [0x03u8; 64],
        suite_id: SUITE_HPKE_X25519_HKDF_SHA256_CHACHA20POLY1305,
    };
    let path = bingle_test::temp_file_helpers::project_tmp_file_path("bingle-local-order", ".json");
    let path_str = path.to_string_lossy().to_string();
    api.save(&path_str).expect("save");
    let mut saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");
    saved["messages"]
        .as_array_mut()
        .expect("messages array")
        .push(
            serde_json::to_value(Message::from_opened(
                &opened,
                "carol".into(),
                vec!["me".into()],
                300,
            ))
            .expect("to json"),
        );
    std::fs::write(&path, saved.to_string()).expect("write");
    api.load(&path_str).expect("load");
    let _ = std::fs::remove_file(&path);

    let texts: Vec<String> = api
        .get_messages()
        .expect("messages")
        .into_iter()
        .map(|m| m.text)
        .collect();
    assert_eq!(texts, vec!["live", "mailbox", "own", "unsigned"]);
}

#[test]
fn messages_with_equal_times_keep_their_stored_order() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    for (i, text) in ["first", "second", "third"].iter().enumerate() {
        api.add_received_message(
            "alice".into(),
            vec!["me".into()],
            10 + i as i64,
            &json!({ "text": text, "sent_time": 5 }),
        )
        .expect("store");
    }

    let texts: Vec<String> = api
        .get_messages()
        .expect("messages")
        .into_iter()
        .map(|m| m.text)
        .collect();
    assert_eq!(texts, vec!["first", "second", "third"]);
}

#[test]
fn the_sender_picks_the_earliest_by_order_time() {
    // Queued at 10 but carrying a later send time (30), versus queued at 20 with none.
    let later_sent = message(10, Some(30), None);
    let earlier = message(20, None, None);

    let chosen =
        select_sendable_message(vec![later_sent, earlier], &HashMap::new(), Instant::now())
            .expect("a sendable message");

    assert_eq!(chosen.timestamp, 20);
}
