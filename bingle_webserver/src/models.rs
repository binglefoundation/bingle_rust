use bingle_core::api::network_endpoint::NetworkEndpoint;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::{SocketAddr, ToSocketAddrs};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InetSocketAddress {
    pub host: String,
    pub port: u16,
}

impl InetSocketAddress {
    pub fn to_socket_addr(&self) -> Option<SocketAddr> {
        format!("{}:{}", self.host, self.port)
            .to_socket_addrs()
            .ok()?
            .next()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSourceKey {
    pub inet_socket_address: Option<InetSocketAddress>,
    pub relay_channel: Option<u16>,
    pub relay_address: Option<InetSocketAddress>,
    pub relay_id: Option<String>,
}

impl From<NetworkSourceKey> for NetworkEndpoint {
    fn from(nsk: NetworkSourceKey) -> Self {
        if let Some(relay_id) = nsk.relay_id {
            NetworkEndpoint::new_relay(
                relay_id,
                nsk.relay_address.and_then(|a| a.to_socket_addr()),
                nsk.relay_channel,
            )
        } else if let Some(addr) = nsk.inet_socket_address.and_then(|a| a.to_socket_addr()) {
            NetworkEndpoint::new_direct(addr)
        } else {
            NetworkEndpoint::new_unset()
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageToIdRequest {
    pub user_id: String,
    pub message: Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageToHandleRequest {
    pub handle: String,
    pub message: Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageToNetworkRequest {
    pub network_source_key: NetworkSourceKey,
    pub user_id: String,
    pub message: Value,
}

// BingleMessage is just Value for now since it's a oneOf and we're a stub
pub type BingleMessage = Value;
pub type Response = Value;

// Local API models

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RegisterKeypairRequest {
    pub handle: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddContactRequest {
    pub handle: String,
    pub id: String,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IdRequest {
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddMessageRequest {
    pub sender_handle: String,
    pub recipient_handles: Vec<String>,
    pub timestamp: i64,
    pub text: String,
    pub cipher_suite: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PathRequest {
    pub path: String,
}
