use bingle_core::messages::marshal::{from_json_str, to_json_value};
use bingle_core::messages::types::{Message, MutexMessage, MutexRelease, MutexResponse};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn unit_mutex_request_from_json() {
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
#[cfg(not(target_os = "ios"))]
pub fn unit_mutex_response_to_json() {
    let mut known_ids = std::collections::HashSet::new();
    known_ids.insert("ID1".to_string());
    known_ids.insert("ID2".to_string());
    let resp = MutexResponse {
        app: "mutex".into(),
        response_tag: Some("t1".into()),
        known_ids: Some(known_ids),
    };
    let msg = Message::Mutex(MutexMessage::Response(resp));
    let v = to_json_value(&msg);
    assert_eq!(v.get("app").and_then(|x| x.as_str()), Some("mutex"));
    assert_eq!(v.get("type").and_then(|x| x.as_str()), Some("response"));
    assert!(v.get("known_ids").is_some());
    let ids = v
        .get("known_ids")
        .and_then(|x| x.as_array())
        .expect("array");
    assert_eq!(ids.len(), 2);
    // Ensure forbidden fields are absent
    assert!(v.get("response_tag").is_none());
    assert!(v.get("text").is_none());
    assert!(v.get("data").is_none());

    let s = serde_json::to_string(&v).expect("serialize");
    let back = from_json_str(&s).expect("parse back");
    match back {
        Message::Mutex(MutexMessage::Response(r)) => {
            assert_eq!(r.known_ids.as_ref().unwrap().len(), 2);
            assert!(r.known_ids.as_ref().unwrap().contains("ID1"));
            assert!(r.known_ids.as_ref().unwrap().contains("ID2"));
        }
        _ => panic!("expected Mutex Response back"),
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn unit_mutex_release_roundtrip() {
    let rel = MutexRelease {
        app: "mutex".into(),
        tag: None,
        known_ids: None,
    };
    let msg = Message::Mutex(MutexMessage::Release(rel));
    let v = to_json_value(&msg);
    assert_eq!(v.get("type").and_then(|x| x.as_str()), Some("release"));
    assert!(v.get("response_tag").is_none());
    assert!(v.get("text").is_none());
    assert!(v.get("data").is_none());

    let s = v.to_string();
    let parsed = from_json_str(&s).expect("parse back");
    match parsed {
        Message::Mutex(MutexMessage::Release(_)) => {}
        _ => panic!("expected Mutex Release after roundtrip"),
    }
}
