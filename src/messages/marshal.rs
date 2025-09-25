use crate::messages::types::*;
use serde_json::{self, Value as JsonValue};

#[derive(Debug)]
pub enum MarshalError {
    Json(serde_json::Error),
}

impl From<serde_json::Error> for MarshalError {
    fn from(e: serde_json::Error) -> Self { MarshalError::Json(e) }
}

pub fn from_json_str(input: &str) -> Result<Message, MarshalError> {
    let val: JsonValue = serde_json::from_str(input)?;
    from_json_value(val)
}

pub fn from_json_value(val: JsonValue) -> Result<Message, MarshalError> {
    // If app and type are both absent or null, treat as PlainTextMessage
    match val {
        JsonValue::Object(map) => {
            let has_app = map.get("app").is_some();
            let has_type = map.get("type").is_some();

            // Try relay typed messages when type present and app is null (or missing)
            if has_type {
                // We accept app missing or null for relay
                if let Ok(relay) = serde_json::from_value::<RelayMessage>(JsonValue::Object(map.clone())) {
                    return Ok(Message::Relay(relay));
                }
            }

            // Try PlainText
            if !has_app && !has_type {
                if let Ok(pt) = serde_json::from_value::<PlainTextMessage>(JsonValue::Object(map.clone())) {
                    return Ok(Message::PlainText(pt));
                }
            }

            // As a last resort, try to infer PlainText with explicit nulls
            if let Ok(pt) = serde_json::from_value::<PlainTextMessage>(JsonValue::Object(map.clone())) {
                return Ok(Message::PlainText(pt));
            }

            Ok(Message::Unknown(JsonValue::Object(map)))
        }
        other => Ok(Message::Unknown(other)),
    }
}

pub fn to_json_value(msg: &Message) -> JsonValue {
    match msg {
        Message::PlainText(pt) => serde_json::to_value(pt).unwrap_or(JsonValue::Null),
        Message::Relay(r) => serde_json::to_value(r).unwrap_or(JsonValue::Null),
        Message::Unknown(v) => v.clone(),
    }
}

pub fn to_json_string(msg: &Message) -> String {
    to_json_value(msg).to_string()
}

