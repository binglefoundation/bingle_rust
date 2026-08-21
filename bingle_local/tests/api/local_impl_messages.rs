use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig, Message};

#[test]
fn messages_initially_empty() {
    let api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let msgs = api.get_messages().expect("get_messages");
    assert!(msgs.is_empty());
}

#[test]
fn add_and_get_single_message() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.add_message("alice".into(), vec!["bob".into()], 123, "hi".into(), None)
        .expect("add_message");
    let msgs = api.get_messages().expect("get_messages");
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].sender_handle, "alice");
    assert_eq!(msgs[0].recipient_handles, vec!["bob".to_string()]);
    assert_eq!(msgs[0].timestamp, 123);
    assert_eq!(msgs[0].text, "hi");
}

#[test]
fn messages_preserve_insertion_order() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.add_message("alice".into(), vec!["bob".into()], 1, "m1".into(), None)
        .unwrap();
    api.add_message(
        "carol".into(),
        vec!["dave".into(), "erin".into()],
        2,
        "m2".into(),
        None,
    )
    .unwrap();
    api.add_message("frank".into(), vec!["george".into()], 3, "m3".into(), None)
        .unwrap();
    let msgs = api.get_messages().unwrap();
    let texts: Vec<String> = msgs.into_iter().map(|m| m.text).collect();
    assert_eq!(texts, vec!["m1", "m2", "m3"]);
}

#[test]
fn add_message_rejects_empty_inputs() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    // empty sender
    assert!(
        api.add_message("".into(), vec!["x".into()], 0, "t".into(), None)
            .is_err()
    );
    // empty recipients
    assert!(
        api.add_message("s".into(), vec![], 0, "t".into(), None)
            .is_err()
    );
    // empty recipient in list
    assert!(
        api.add_message("s".into(), vec!["".into()], 0, "t".into(), None)
            .is_err()
    );
    // empty text
    assert!(
        api.add_message("s".into(), vec!["x".into()], 0, "".into(), None)
            .is_err()
    );
}

#[test]
fn get_messages_returns_clone() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.add_message("alice".into(), vec!["bob".into()], 1, "m1".into(), None)
        .unwrap();
    let mut msgs: Vec<Message> = api.get_messages().unwrap();
    // mutate the returned vector
    msgs.push(Message {
        sender_handle: "z".into(),
        recipient_handles: vec!["y".into()],
        timestamp: 9,
        text: "zzz".into(),
        cipher_suite: None,
        progress: Some(1.0),
        failure_reason: None,
        failure_kind: None,
    });
    // fetch again and ensure original store is unchanged
    let msgs2 = api.get_messages().unwrap();
    assert_eq!(msgs2.len(), 1);
    assert_eq!(msgs2[0].text, "m1");
}
