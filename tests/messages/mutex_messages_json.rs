use rust_comms::messages::marshal::{from_json_str, to_json_value};
use rust_comms::messages::types::{Message, MutexMessage, MutexResponse, MutexRelease};

#[test]
fn unit_mutex_request_from_json() {
    let json = r#"{"app":"mutex","type":"request","lamport_timestamp":42}"#;
    let msg = from_json_str(json).expect("decode");
    match msg {
        Message::Mutex(MutexMessage::Request(req)) => {
            assert_eq!(req.app, "mutex");
            assert_eq!(req.lamport_timestamp, 42);
        }
        _ => panic!("expected Mutex Request"),
    }
}

#[test]
fn unit_mutex_response_to_json() {
    let resp = MutexResponse {
        app: "mutex".into(),
        tag: Some("t1".into()),
    };
    let msg = Message::Mutex(MutexMessage::Response(resp));
    let v = to_json_value(&msg);
    assert_eq!(v.get("app").and_then(|x| x.as_str()), Some("mutex"));
    assert_eq!(v.get("type").and_then(|x| x.as_str()), Some("response"));
    // Ensure forbidden fields are absent
    assert!(v.get("responseTag").is_none());
    assert!(v.get("text").is_none());
    assert!(v.get("data").is_none());
}

#[test]
fn unit_mutex_release_roundtrip() {
    let rel = MutexRelease { app: "mutex".into(), tag: None };
    let msg = Message::Mutex(MutexMessage::Release(rel));
    let v = to_json_value(&msg);
    assert_eq!(v.get("type").and_then(|x| x.as_str()), Some("release"));
    assert!(v.get("responseTag").is_none());
    assert!(v.get("text").is_none());
    assert!(v.get("data").is_none());

    let s = v.to_string();
    let parsed = from_json_str(&s).expect("parse back");
    match parsed {
        Message::Mutex(MutexMessage::Release(_)) => {}
        _ => panic!("expected Mutex Release after roundtrip"),
    }
}
