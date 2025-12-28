use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

// Re-export common DDB types for convenience
pub use crate::ddb::{AdvertRecord, InetSocketAddress};

// Core message enum representing all known classes we currently model from the OpenAPI
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    PlainText(PlainTextMessage),
    Relay(RelayMessage),
    Ddb(DdbMessage),
    Ping(PingMessage),
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
    #[serde(rename = "TriangleTest1Response")]
    TriangleTest1Response(RelayTriangleTest1Response),
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

    #[allow(dead_code)]
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
    #[serde(rename = "calledId")]
    pub called_id: String,
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
    #[serde(rename = "checkingEndpoint")]
    pub checking_endpoint: SocketAddr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayTriangleTest2 {
    #[serde(default, deserialize_with = "nullable_app::deserialize_null")]
    pub app: Option<String>, // must be None (null)
    #[serde(rename = "checkingId")]
    pub checking_id: String,
    #[serde(rename = "checkingEndpoint")]
    pub checking_endpoint: SocketAddr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayTriangleTest3 {
    #[serde(default, deserialize_with = "nullable_app::deserialize_null")]
    pub app: Option<String>, // must be None (null)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayTriangleTest1Response {
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
    #[serde(rename = "calledId")]
    pub called_id: String,
    pub channel: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayKeepAlive {
    #[serde(default, deserialize_with = "nullable_app::deserialize_null")]
    pub app: Option<String>, // must be None (null)
}

// ---------------- DDB messages (app = "ddb") ----------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DdbMessage {
    #[serde(rename = "upsertResolve")] UpsertResolve(DdbUpsertResolve),
    #[serde(rename = "deleteResolve")] DeleteResolve(DdbDeleteResolve),
    #[serde(rename = "queryResolve")] QueryResolve(DdbQueryResolve),
    #[serde(rename = "queryResponse")] QueryResponse(DdbQueryResponse),
    #[serde(rename = "updateResponse")] UpdateResponse(DdbUpdateResponse),
    #[serde(rename = "signon")] Signon(DdbSignon),
    #[serde(rename = "signonResponse")] SignonResponse(DdbSignonResponse),
    #[serde(rename = "getEpoch")] GetEpoch(DdbGetEpoch),
    #[serde(rename = "getEpochResponse")] EpochInfo(DdbEpochInfo),
    #[serde(rename = "initResolve")] InitResolve(DdbInitResolve),
    #[serde(rename = "initResponse")] InitResponse(DdbInitResponse),
    #[serde(rename = "dumpResolve")] DumpResolve(DdbDumpResolve),
    #[serde(rename = "dumpResolveResponse")] DumpResolveResponse(DdbDumpResolveResponse),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DdbUpsertResolve {
    pub app: String, // "ddb"
    #[serde(rename = "startId")]
    pub start_id: String,
    pub epoch: u64,
    pub record: AdvertRecord,
    #[serde(rename = "originalSignature")]
    pub original_signature: String,
    pub rippled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(rename = "responseTag", default, skip_serializing_if = "Option::is_none")]
    pub response_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DdbDeleteResolve {
    pub app: String, // "ddb"
    #[serde(rename = "startId")]
    pub start_id: String,
    pub epoch: u64,
    #[serde(rename = "originalSignature")]
    pub original_signature: String,
    pub rippled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(rename = "responseTag", default, skip_serializing_if = "Option::is_none")]
    pub response_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DdbQueryResolve {
    pub app: String, // "ddb"
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(rename = "responseTag", default, skip_serializing_if = "Option::is_none")]
    pub response_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DdbQueryResponse {
    pub app: String, // "ddb"
    pub found: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advert: Option<AdvertRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(rename = "responseTag", default, skip_serializing_if = "Option::is_none")]
    pub response_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DdbUpdateResponse {
    pub app: String, // "ddb"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(rename = "responseTag", default, skip_serializing_if = "Option::is_none")]
    pub response_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DdbSignon {
    pub app: String, // "ddb"
    #[serde(rename = "startId")]
    pub start_id: String,
    #[serde(rename = "originalSignature", default, skip_serializing_if = "Option::is_none")]
    pub original_signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rippled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(rename = "responseTag", default, skip_serializing_if = "Option::is_none")]
    pub response_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DdbSignonResponse {
    pub app: String, // "ddb"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(rename = "responseTag", default, skip_serializing_if = "Option::is_none")]
    pub response_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DdbGetEpoch {
    pub app: String, // "ddb"
    #[serde(rename = "epochId")]
    pub epoch_id: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(rename = "responseTag", default, skip_serializing_if = "Option::is_none")]
    pub response_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DdbEpochInfo {
    pub app: String, // "ddb"
    #[serde(rename = "epochId")]
    pub epoch_id: i64,
    #[serde(rename = "treeOrder")]
    pub tree_order: i32,
    #[serde(rename = "relayIds")]
    pub relay_ids: Vec<String>,
    #[serde(rename = "relayEndpoints", default, skip_serializing_if = "Option::is_none")]
    pub relay_endpoints: Option<Vec<InetSocketAddress>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(rename = "responseTag", default, skip_serializing_if = "Option::is_none")]
    pub response_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DdbInitResolve {
    pub app: String, // "ddb"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(rename = "responseTag", default, skip_serializing_if = "Option::is_none")]
    pub response_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DdbInitResponse {
    pub app: String, // "ddb"
    #[serde(rename = "dbCount")]
    pub db_count: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(rename = "responseTag", default, skip_serializing_if = "Option::is_none")]
    pub response_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DdbDumpResolve {
    pub app: String, // "ddb"
    pub record: AdvertRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(rename = "responseTag", default, skip_serializing_if = "Option::is_none")]
    pub response_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DdbDumpResolveResponse {
    pub app: String, // "ddb"
    #[serde(rename = "recordIndex")]
    pub record_index: i64,
    #[serde(rename = "recordId")]
    pub record_id: String,
    pub record: AdvertRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(rename = "responseTag", default, skip_serializing_if = "Option::is_none")]
    pub response_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// Ping messages (app: "ping")
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PingMessage {
    #[serde(rename = "ping")]
    Ping(PingPing),
    #[serde(rename = "response")]
    Response(PingResponse),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PingPing {
    pub app: String, // must be "ping"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(rename = "responseTag", default, skip_serializing_if = "Option::is_none")]
    pub response_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PingResponse {
    pub app: String, // must be "ping"
    #[serde(rename = "verifiedId")]
    pub verified_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(rename = "responseTag", default, skip_serializing_if = "Option::is_none")]
    pub response_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}
