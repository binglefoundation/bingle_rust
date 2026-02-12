use rust_comms::messages::*;
use serde_json::Value;

fn as_json_value(msg: &Message) -> Value { to_json_value(msg) }

#[test]
fn unit_serialize_relay_call_and_roundtrip() {
    let msg = Message::Relay(RelayMessage::Call(RelayCall { app: None, called_id: "abc".to_string() }));
    let val = as_json_value(&msg);
    // Ensure shape
    let obj = val.as_object().expect("json object");
    assert_eq!(obj.get("type").and_then(|v: &serde_json::Value| v.as_str()).expect("type"), "Call");
    assert!(obj.get("app").expect("app").is_null());
    assert_eq!(obj.get("calledId").and_then(|v: &serde_json::Value| v.as_str()).expect("calledId"), "abc");

    // Round-trip
    let s = to_json_string(&msg);
    let back = from_json_str(&s).expect("decode back");
    match back {
        Message::Relay(RelayMessage::Call(m)) => {
            assert_eq!(m.called_id, "abc");
            assert!(m.app.is_none());
        }
        _ => panic!("expected Relay Call"),
    }
}

#[test]
fn unit_serialize_relay_listen() {
    let msg = Message::Relay(RelayMessage::Listen(RelayListen { app: None }));
    let val = as_json_value(&msg);
    let obj = val.as_object().expect("obj");
    assert_eq!(obj.get("type").and_then(|v: &serde_json::Value| v.as_str()).expect("type"), "Listen");
    assert!(obj.get("app").expect("app").is_null());
}

#[test]
fn unit_serialize_relay_listen_response() {
    let msg = Message::Relay(RelayMessage::ListenResponse(RelayListenResponse { app: None }));
    let val = as_json_value(&msg);
    let obj = val.as_object().expect("obj");
    assert_eq!(obj.get("type").and_then(|v: &serde_json::Value| v.as_str()).expect("type"), "ListenResponse");
    assert!(obj.get("app").expect("app").is_null());
}

#[test]
fn unit_serialize_relay_call_response_and_roundtrip() {
    let msg = Message::Relay(RelayMessage::CallResponse(RelayCallResponse { app: None, called_id: "callee".to_string(), channel: 42 }));
    let val = as_json_value(&msg);
    let obj = val.as_object().expect("obj");
    assert_eq!(obj.get("type").and_then(|v: &serde_json::Value| v.as_str()).expect("type"), "CallResponse");
    assert!(obj.get("app").expect("app").is_null());
    assert_eq!(obj.get("calledId").and_then(|v: &serde_json::Value| v.as_str()).expect("calledId"), "callee");
    assert_eq!(obj.get("channel").and_then(|v: &serde_json::Value| v.as_u64()).expect("channel"), 42);

    let s = to_json_string(&msg);
    let back = from_json_str(&s).expect("decode back");
    match back {
        Message::Relay(RelayMessage::CallResponse(m)) => {
            assert_eq!(m.called_id, "callee");
            assert_eq!(m.channel, 42);
            assert!(m.app.is_none());
        }
        _ => panic!("expected Relay CallResponse"),
    }
}
