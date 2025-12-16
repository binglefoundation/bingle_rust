use rust_comms::messages::marshal::{from_json_str, to_json_value};
use rust_comms::messages::types::{Message, PingMessage};

#[test]
fn unit_ping_ping_from_json() {
    let json = r#"{"app":"ping","type":"ping","text":"hello"}"#;
    let msg = from_json_str(json).expect("decode");
    match msg {
        Message::Ping(PingMessage::Ping(p)) => {
            assert_eq!(p.app, "ping");
            assert_eq!(p.text.as_deref(), Some("hello"));
        }
        _ => panic!("expected Ping Ping"),
    }
}

#[test]
fn unit_ping_response_to_json() {
    // Build a PingResponse and ensure fields serialize per schema
    let resp = rust_comms::messages::types::PingResponse {
        app: "ping".into(),
        verified_id: "SOMEID".into(),
        tag: None,
        response_tag: Some("abc".into()),
        text: Some("ACK: hi".into()),
        data: None,
    };
    let msg = Message::Ping(PingMessage::Response(resp));
    let v = to_json_value(&msg);
    assert_eq!(v.get("app").and_then(|x| x.as_str()), Some("ping"));
    assert_eq!(v.get("type").and_then(|x| x.as_str()), Some("response"));
    assert_eq!(v.get("verifiedId").and_then(|x| x.as_str()), Some("SOMEID"));
    assert_eq!(v.get("responseTag").and_then(|x| x.as_str()), Some("abc"));
    assert_eq!(v.get("text").and_then(|x| x.as_str()), Some("ACK: hi"));
}
