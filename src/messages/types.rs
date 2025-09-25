use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

// Core message enum representing all known classes we currently model from the OpenAPI
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    PlainText(PlainTextMessage),
    Relay(RelayMessage),
    // Fallback for any unknown message shapes
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlainTextMessage {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<Option<String>>, // allow explicit null via Some(None)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<Option<String>>, // allow explicit null via Some(None)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RelayMessage {
    #[serde(rename = "Call")]
    Call(RelayCall),
    #[serde(rename = "RelayResponse")]
    RelayResponse(RelayResponse),
    #[serde(rename = "TriangleTest1")]
    TriangleTest1(RelayTriangleTest1),
    #[serde(rename = "TriangleTest2")]
    TriangleTest2(RelayTriangleTest2),
    #[serde(rename = "TriangleTest3")]
    TriangleTest3(RelayTriangleTest3),
    #[serde(rename = "Listen")]
    Listen(RelayListen),
    #[serde(rename = "Check")]
    Check(RelayCheck),
    #[serde(rename = "ListenResponse")]
    ListenResponse(RelayListenResponse),
    #[serde(rename = "CheckResponse")]
    CheckResponse(RelayCheckResponse),
    #[serde(rename = "CallResponse")]
    CallResponse(RelayCallResponse),
    #[serde(rename = "KeepAlive")]
    KeepAlive(RelayKeepAlive),
}

// Note: In the OpenAPI, these relay message schemas require app: null. We model this as Option<Option<String>>
// so deserialization will accept either absence or explicit null, but reject non-null values if provided.

mod nullable_app {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn as_app_null<S>(value: &Option<String>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            None => serializer.serialize_none(),
            Some(v) => {
                if v.is_empty() {
                    serializer.serialize_none()
                } else {
                    serializer.serialize_some(v)
                }
            }
        }
    }

    pub fn deserialize_null<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<String>::deserialize(deserializer)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayCall {
    #[serde(default, deserialize_with = "nullable_app::deserialize_null")]
    pub app: Option<String>, // must be None (null)
    pub calledId: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayResponse {
    #[serde(default, deserialize_with = "nullable_app::deserialize_null")]
    pub app: Option<String>, // must be None (null)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayTriangleTest1 {
    #[serde(default, deserialize_with = "nullable_app::deserialize_null")]
    pub app: Option<String>, // must be None (null)
    pub checkingEndpoint: SocketAddr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayTriangleTest2 {
    #[serde(default, deserialize_with = "nullable_app::deserialize_null")]
    pub app: Option<String>, // must be None (null)
    pub checkingId: String,
    pub checkingEndpoint: SocketAddr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayTriangleTest3 {
    #[serde(default, deserialize_with = "nullable_app::deserialize_null")]
    pub app: Option<String>, // must be None (null)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayListen {
    #[serde(default, deserialize_with = "nullable_app::deserialize_null")]
    pub app: Option<String>, // must be None (null)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayCheck {
    #[serde(default, deserialize_with = "nullable_app::deserialize_null")]
    pub app: Option<String>, // must be None (null)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayListenResponse {
    #[serde(default, deserialize_with = "nullable_app::deserialize_null")]
    pub app: Option<String>, // must be None (null)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayCheckResponse {
    #[serde(default, deserialize_with = "nullable_app::deserialize_null")]
    pub app: Option<String>, // must be None (null)
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayCallResponse {
    #[serde(default, deserialize_with = "nullable_app::deserialize_null")]
    pub app: Option<String>, // must be None (null)
    pub calledId: String,
    pub channel: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayKeepAlive {
    #[serde(default, deserialize_with = "nullable_app::deserialize_null")]
    pub app: Option<String>, // must be None (null)
}
