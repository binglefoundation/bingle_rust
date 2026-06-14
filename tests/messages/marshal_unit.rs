use rust_comms::messages::marshal::from_json_str;
use rust_comms::messages::types::{Message, RelayMessage};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn unit_plain_text_from_json() {
    let json = r#"{"text":"Hello"}"#;
    let msg = from_json_str(json).expect("decode");
    match msg {
        Message::PlainText(pt) => assert_eq!(pt.text, "Hello"),
        _ => panic!("expected PlainText"),
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn unit_triangle_test1_from_json() {
    let json = r#"{"app":null,"type":"TriangleTest1","checkingEndpoint":{"host":"127.0.0.1","port":12345}}"#;
    let msg = from_json_str(json).expect("decode");
    match msg {
        Message::Relay(RelayMessage::TriangleTest1(m)) => {
            assert_eq!(m.checking_endpoint.to_string(), "127.0.0.1:12345");
        }
        _ => panic!("expected Relay TriangleTest1"),
    }
}
