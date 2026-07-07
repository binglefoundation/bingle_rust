use bingle_core::messages::marshal::from_json_str;
use bingle_core::messages::types::{Message, RelayMessage};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn unit_triangle_test1_response_from_json() {
    let json = r#"{"app":null,"type":"TriangleTest1Response"}"#;
    let msg = from_json_str(json).expect("decode");
    match msg {
        Message::Relay(RelayMessage::TriangleTest1Response(_)) => {}
        _ => panic!("expected Relay TriangleTest1Response"),
    }
}
