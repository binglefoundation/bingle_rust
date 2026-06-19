use std::net::SocketAddr;
use std::sync::Arc;

use serde_json::Value as JsonValue;

use crate::api::bingle_api::{BingleError, NetworkEndpoint};
use crate::ddb::client::DdbClient;
use crate::messages::{Message, RelayMessage};
use crate::messages::marshal::to_json_value;
use crate::messages::types::RelayCall;

/// RelayClient: opens a relay channel to a target id via a given relay NSK.
///
/// Behaviour:
/// - Accept a relay NetworkEndpoint (NSK) that carries a relay_id and optionally a relay_address.
/// - If relay_address is missing, look it up via DdbClient::lookup(relay_id) and extract the direct address.
/// - Send Relay::Call(calledId = target_id) to the relay using BingleApi and await response.
/// - Parse the response (RelayResponse or RelayCallResponse) to obtain the allocated channel.
/// - Return a populated NetworkEndpoint with relay_id, relay_address and relay_channel.
#[derive(Clone)]
pub struct RelayClient {
    api: crate::api::bingle_api::BingleApiBothType,
    ddb: Arc<dyn DdbClient>,
}

impl RelayClient {
    pub fn new(api: crate::api::bingle_api::BingleApiBothType, ddb: Arc<dyn DdbClient>) -> Self {
        Self { api, ddb }
    }

    fn api(&self) -> Result<Arc<dyn crate::api::bingle_api::BingleApiBoth>, BingleError> {
        self.api.upgrade().ok_or_else(|| BingleError::Other("BingleApi dropped".to_string()))
    }

    /// Open a channel via the relay identified in `relay_nsk` to the provided `target_id`.
    /// Returns a NetworkEndpoint suitable for sending data via the relay (with channel and address set).
    pub fn call(&self, relay_nsk: &NetworkEndpoint, target_id: &str) -> Result<NetworkEndpoint, BingleError> {
        tracing::info!("[RelayClient::call] my_id={:?}, relay_nsk: {:?}, target_id: {}", 
            self.api.upgrade().and_then(|a| a.get_my_id()), relay_nsk, target_id);
        
        // Validate the relay id is present
        let relay_id = relay_nsk
            .relay_id()
            .ok_or_else(|| BingleError::Other("RelayClient::call: relay_nsk has no relay_id".to_string()))?
            .to_string();

        // Ensure we have a relay address (SocketAddr)
        let relay_addr: SocketAddr = if let Some(addr) = relay_nsk.relay_address() {
            addr
        } else {
            // Resolve via DDB using the relay's id
            let resolved = self.ddb.lookup(&relay_id).map_err(|e| BingleError::Other(e.to_string()))?;
            // The relay should advertise a direct endpoint in DDB
            resolved
                .inet_socket_address()
                .ok_or_else(|| BingleError::Other("RelayClient::call: DDB lookup for relay did not return a direct endpoint".to_string()))?
        };

        // Build the Relay::Call message
        let msg = Message::Relay(RelayMessage::Call(RelayCall { app: None, called_id: target_id.to_string(), tag: None }));
        let json: JsonValue = to_json_value(&msg);

        // Send to the relay and await response
        let relay_endpoint = NetworkEndpoint::new_direct(relay_addr);
        let resp = self.api()?.send_message_to_network_with_response(&relay_endpoint, &relay_id, json, None)?;

        // Parse channel from either RelayResponse or RelayCallResponse
        let ty = resp.get("type").and_then(|v: &serde_json::Value| v.as_str()).unwrap_or("");
        let channel_opt = match ty {
            "RelayResponse" => resp.get("channel").and_then(|v: &serde_json::Value| v.as_u64()).map(|v| v as u16),
            "CallResponse" | "RelayCallResponse" => resp.get("channel").and_then(|v: &serde_json::Value| v.as_u64()).map(|v| v as u16),
            _ => None,
        };
        let channel = channel_opt.ok_or_else(|| BingleError::Other("RelayClient::call: unexpected response (missing channel)".to_string()))?;

        // Return a relay endpoint configured with address + channel + relay id
        Ok(NetworkEndpoint::new_relay(relay_id, Some(relay_addr), Some(channel)))
    }
}
