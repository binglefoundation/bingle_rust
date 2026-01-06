//! Bingle API trait definitions.
//!
//! This file defines the BingleApi trait and associated types only.
//! It does not provide a concrete implementation.

use std::sync::Arc;
use std::net::SocketAddr;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Internal-only API for engine control from message handlers and router context.
/// Not part of the public BingleApi surface. Intended for in-process coordination only.
pub trait BingleApiInternal: Send + Sync {
    /// Request the engine state to be set to the provided value. Implementations should
    /// delegate to the underlying Engine instance. Implementations may be best-effort
    /// and can ignore unsupported transitions.
    fn set_state(&self, state: crate::engine::EngineState);
    /// Get the current engine state. Default: StunIdentify for mocks that don't track state.
    fn get_state(&self) -> crate::engine::EngineState { crate::engine::EngineState::StunIdentify }
    /// Set the detected NAT type on the engine. Default no-op to keep older tests/mocks compiling.
    fn set_nat_type(&self, _nat: crate::engine::NatType) { }
    /// Retrieve the last discovered public address (IP:port) if available. Default None.
    fn get_last_public_addr(&self) -> Option<SocketAddr> { None }
    /// Register an endpoint IP:port via the engine's DDB client. Default: not implemented.
    fn ddb_register_ip(&self, _endpoint: SocketAddr) -> Result<(), String> { Err("not implemented".to_string()) }
    /// Register a relay association via the engine's DDB client. Default: not implemented.
    fn ddb_register_relay(&self, _relay_id: String, _relay_sig: Option<String>) -> Result<(), String> { Err("not implemented".to_string()) }
}

/// Convenience type aliases used by the Bingle API.
/// UserId: Algorand address in base32 (RFC 4648, no padding), representing a 32‑byte public key
/// followed by a 4‑byte checksum (total 36 bytes). Examples: "P577…", "4TKG…".
pub type UserId = String; // Algorand address (base32, 36-byte decoded)
pub type Handle = String; // User handle string
/// NetworkSourceKey identifies where to send network traffic (direct or via relay).
/// Translated from the provided Kotlin data class.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NetworkEndpoint {
    /// Direct socket address if sending directly.
    pub inet_socket_address: Option<SocketAddr>,
    /// TURN relay channel number if using a relay (16-bit per RFC 5766).
    pub relay_channel: Option<u16>,
    /// Relay server socket address (IP:port) if using a relay.
    pub relay_address: Option<SocketAddr>,
    /// Optional relay id (Algorand base32 address) used when a channel has not yet been allocated.
    #[serde(default)]
    pub relay_id: Option<String>,
}

impl NetworkEndpoint {
    /// Construct a direct (non-relay) endpoint key.
    pub fn new_direct(addr: SocketAddr) -> Self {
        Self {
            inet_socket_address: Some(addr),
            relay_channel: None,
            relay_address: None,
            relay_id: None,
        }
    }

    /// Construct a relay endpoint key.
    pub fn new_relay(relay_address: SocketAddr, relay_channel: u16) -> Self {
        Self {
            inet_socket_address: None,
            relay_channel: Some(relay_channel),
            relay_address: Some(relay_address),
            relay_id: None,
        }
    }

    /// True if this key represents a relay endpoint.
    pub fn is_relay(&self) -> bool { self.relay_channel.is_some() }
}

impl fmt::Display for NetworkEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_relay() {
            write!(f, "NetworkSourceKey(relayChannel={:?}, relayAddress={:?}, relayId={:?})", self.relay_channel, self.relay_address, self.relay_id)
        } else if self.relay_id.is_some() {
            write!(f, "NetworkSourceKey(relayId={:?})", self.relay_id)
        } else {
            write!(f, "NetworkSourceKey(inetSocketAddress={:?})", self.inet_socket_address)
        }
    }
}

/// Progress callback reported during send operations.
/// Parameters:
/// - percent_done: 0..=100 indicating the percentage complete
/// - message: human-readable progress message
pub type ProgressCallback = dyn Fn(u8, String) + Send + Sync + 'static;

/// Handler invoked when a plaintext message is received.
/// Parameters:
/// - sender: id of the sender
/// - sender_handle: handle of the sender
/// - message: the inbound message deserialized from JSON
pub type OnMessageHandler = dyn Fn(UserId, Handle, JsonValue) + Send + Sync + 'static;

/// Handler invoked when a peer connects.
/// Parameters:
/// - sender: id of the sender
/// - sender_handle: handle of the sender
pub type OnConnectHandler = dyn Fn(UserId, Handle) + Send + Sync + 'static;

/// Options used to start the Bingle node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StartOptions {
    /// The local user's handle (unique globally).
    pub handle: Handle,
    /// The Algorand passphrase to the user account.
    /// JSON field name: "algoPassphrase" to match Kotlin/JS naming.
    #[serde(rename = "algoPassphrase", default)]
    pub algo_passphrase: Option<String>,
    /// Optional static IP address (and port) as seen externally, e.g. "203.0.113.5:4433".
    pub static_ip: Option<SocketAddr>,
    /// True if we are to become a relay.
    pub am_relay: bool,
    /// Optional array of STUN servers (IP:port) to determine our IP address.
    pub stun_servers: Option<Vec<SocketAddr>>,
    /// Optional Algorand provider configuration loaded from --node-file.
    #[serde(default)]
    pub algo_provider_config: Option<crate::blockchain::algo_ops::AlgoChainConfig>,
    /// Optional human-readable network name from the node file (e.g., mainnet, testnet).
    #[serde(default)]
    pub algo_network: Option<String>,
    /// Optional Algorand application id for the Bingle dApp (used for indexer discovery).
    #[serde(default)]
    pub app_id: Option<u64>,
    /// Optional Algorand asset id associated with the dApp (carried for completeness).
    #[serde(default)]
    pub asset_id: Option<u64>,
    /// Optional log level override (trace|debug|info|warn|error). If None, defaults to debug on debug builds and warn on release.
    #[serde(default)]
    pub log_level: Option<String>,
}

impl Default for StartOptions {
    fn default() -> Self {
        Self {
            handle: String::new(),
            algo_passphrase: None,
            static_ip: None,
            am_relay: false,
            stun_servers: None,
            algo_provider_config: None,
            algo_network: None,
            app_id: None,
            asset_id: None,
            log_level: None,
        }
    }
}

/// The Bingle API trait surface.
/// This describes the minimal shape expected by the Bingle client per spec.
pub trait BingleApi: Send + Sync {
    /// Debug helper: print internal start options if available. Default no-op.
    fn debug_print_options(&self) { /* default no-op for mocks */ }
    /// Returns this node's id (Algorand address), if known.
    /// Implementations should derive this from the engine issuer (issuer without suffix).
    fn get_my_id(&self) -> Option<String>;
    /// Alias for get_my_id to match external nomenclature.
    fn get_user_id(&self) -> Option<String> { self.get_my_id() }
    /// Returns the configured handle, if known.
    fn get_handle(&self) -> Option<String> { None }
    /// Returns the configured application id, if any, from StartOptions. Preferred over env vars.
    fn get_app_id(&self) -> Option<u64>;
    /// Returns the configured Algorand provider config from StartOptions, if any. Default: None.
    fn get_algo_provider_config(&self) -> Option<crate::blockchain::algo_ops::AlgoChainConfig> { None }
    /// Start the node using the provided options. Implementations may spawn background tasks.
    fn start(&mut self, options: &StartOptions) -> Result<(), String>;

    /// Stop all threads/tasks and release resources.
    fn stop(&mut self);

    /// Indicates the network connection has changed and we need to rescan for IP address/port.
    fn network_change(&mut self);

    // Outgoing message transfer methods (see BINGLE_SPEC.md - Outgoing message transfer):

    /// Send a message when we have the id and don't need a response.
    fn send_message_to_id(
        &self,
        user_id: &UserId,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> bool;

    /// Send a message when we have the handle and don't need a response.
    fn send_message_to_handle(
        &self,
        handle: &Handle,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> bool;

    /// Send a message when we have the endpoint NetworkSourceKey and the id.
    fn send_message_to_network(
        &self,
        network_source_key: &NetworkEndpoint,
        user_id: &UserId,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> bool;

    /// Send a message when we have the id and need a response.
    fn send_message_to_id_with_response(
        &self,
        user_id: &UserId,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, String>;

    /// Send a message when we have the handle and need a response.
    fn send_message_to_handle_with_response(
        &self,
        handle: &Handle,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, String>;

    /// Send a message when we have the endpoint NetworkSourceKey and the id and need a response.
    fn send_message_to_network_with_response(
        &self,
        network_source_key: &NetworkEndpoint,
        user_id: &UserId,
        message: JsonValue,
        progress: Option<Arc<ProgressCallback>>,
    ) -> Result<JsonValue, String>;

    // Handler properties:

    /// Set or clear the onMessage callback. Pass None to clear.
    fn set_on_message(&mut self, handler: Option<Arc<OnMessageHandler>>);

    /// Set or clear the onConnect callback. Pass None to clear.
    fn set_on_connect(&mut self, handler: Option<Arc<OnConnectHandler>>);
}
