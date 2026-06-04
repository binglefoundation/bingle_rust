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

            // Try typed messages based on app first
            if let Some(JsonValue::String(app_str)) = map.get("app") {
                if app_str == "ping" {
                    if let Ok(ping) = serde_json::from_value::<PingMessage>(JsonValue::Object(map.clone())) {
                        return Ok(Message::Ping(ping));
                    }
                } else if app_str == "ddb" {
                    if let Ok(ddb) = serde_json::from_value::<DdbMessage>(JsonValue::Object(map.clone())) {
                        return Ok(Message::Ddb(ddb));
                    }
                } else if app_str == "mutex" {
                    if let Ok(mx) = serde_json::from_value::<MutexMessage>(JsonValue::Object(map.clone())) {
                        return Ok(Message::Mutex(mx));
                    }
                } else if app_str == "reportFail" {
                    if let Ok(rf) = serde_json::from_value::<ReportFailMessage>(JsonValue::Object(map.clone())) {
                        return Ok(Message::ReportFail(rf));
                    }
                }
            }

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
        Message::Ddb(d) => serde_json::to_value(d).unwrap_or(JsonValue::Null),
        Message::Ping(p) => serde_json::to_value(p).unwrap_or(JsonValue::Null),
        Message::Mutex(m) => serde_json::to_value(m).unwrap_or(JsonValue::Null),
        Message::ReportFail(rf) => serde_json::to_value(rf).unwrap_or(JsonValue::Null),
        Message::Unknown(v) => v.clone(),
    }
}

pub fn to_json_string(msg: &Message) -> String {
    to_json_value(msg).to_string()
}

