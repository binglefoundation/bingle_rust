//! Bingle API trait definitions.
//!
//! This file defines the BingleApi trait and associated types only.
//! It does not provide a concrete implementation.

use std::sync::Arc;
use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Internal-only API for engine control from message handlers and router context.
/// Not part of the public BingleApi surface. Intended for in-process coordination only.
pub trait BingleApiInternal: Send + Sync {
    // Mutex message forwarding to Engine (default no-ops so existing tests/mocks need not implement)
    fn mutex_handle_request(&self, _from_id: String, _req: crate::messages::types::MutexRequest) {}
    fn mutex_handle_response(
        &self,
        _from_id: String,
        _resp: crate::messages::types::MutexResponse,
    ) {}
    fn mutex_handle_release(&self, _from_id: String, _rel: crate::messages::types::MutexRelease) {}
    /// Request the engine state to be set to the provided value. Implementations should
    /// delegate to the underlying Engine instance. Implementations may be best-effort
    /// and can ignore unsupported transitions.
    fn set_state(&self, state: crate::engine::EngineState);
    /// Get the current engine state.
    fn get_state(&self) -> crate::engine::EngineState;
    /// Set the detected NAT type on the engine.
    fn set_nat_type(&self, nat: crate::engine::NatType);
    /// Retrieve the last discovered public address (IP:port) if available.
    fn get_last_public_addr(&self) -> Option<SocketAddr>;
    /// Register an endpoint IP:port via the engine's DDB client.
    fn ddb_register_ip(&self, endpoint: SocketAddr, am_relay: bool) -> Result<(), String>;
    /// Register a relay association via the engine's DDB client.
    fn ddb_register_relay(&self, relay_id: String, relay_sig: Option<String>) -> Result<(), String>;

    // Update the TURN client listener relay - called after a Listen message has been sent.
    fn update_turn_listener_relay(&self, relay_id: String, relay_addr: SocketAddr) -> Result<(), String>;
    /// Client-side handler invoked on ListenResponse to register allowed relay id <-> addr mapping.
    fn turn_client_handle_listen_response(&self, relay_addr: SocketAddr, relay_id: String);
    /// Lookup a previously registered address for the given id (TURN mapping).
    fn turn_lookup_addr_by_id(&self, id: String) -> Option<SocketAddr>;
    /// Handle a Relay::Call by allocating or retrieving a TURN channel for the (source, dest) pair.
    /// Returns the channel number as i32 (negative on failure) to mirror TurnHandler::handle_call.
    fn turn_handle_call(&self, source: SocketAddr, dest: SocketAddr) -> i32;
    /// Handle a Relay::Listen on the relay side to register id -> source address.
    fn turn_handle_listen(&self, id: String, source: SocketAddr) -> bool;
    /// Handle a RelayCalled notification at the client side to register the channel mapping.
    fn turn_handle_called(&self, source: SocketAddr, dest: SocketAddr, channel: u16);

    /// Notify that the node's listening state changed.
    fn notify_listening(&self, listening: bool);

    /// Get current relay state string for CheckResponse ("off"|"starting"|"loading"|"loaded"|"available"|"own").
    fn get_relay_state(&self) -> String;

    /// Set the relay_state on the engine. Handlers use this to transition to Loaded after sync.
    fn set_relay_state(&self, _state: crate::engine::RelayState) { /* default no-op */ }

    /// If loading from a peer, return the target number of records expected from InitResponse.
    fn get_peer_ddb_target(&self) -> Option<usize> { None }

    /// Insert a DDB AdvertRecord into the local backend.
    fn ddb_upsert_record(&self, _record: crate::ddb::AdvertRecord) { /* default no-op */ }

    /// Get current number of records in the DDB backend.
    fn ddb_backend_size(&self) -> usize { 0 }

    /// Initialize relay state: discover peers, coordinate, and sync DDB.
    fn initialize_relay(&self) {}

    /// Returns true if this node is configured as a relay.
    fn is_relay(&self) -> bool { false }

    /// Signal that relay signon is complete.
    fn signal_signon_complete(&self) {}

    /// Reset relay signon completion signal.
    fn reset_signon_complete(&self) {}

    /// Send a message to all known relays (except ourselves and the message originator).
    fn ripple_message(&self, _message: JsonValue, _originator_id: String) {}
}

/// Convenience type aliases used by the Bingle API.
/// UserId: Algorand address in base32 (RFC 4648, no padding), representing a 32‑byte public key
/// followed by a 4‑byte checksum (total 36 bytes). Examples: "P577…", "4TKG…".
pub type UserId = String; // Algorand address (base32, 36-byte decoded)
pub type Handle = String; // User handle string
pub use super::network_endpoint::{NetworkEndpoint, NetworkEndpointKey};

/// Composite trait required by message handlers: implements both the public API and internal controls.
pub trait BingleApiBoth: BingleApi + BingleApiInternal {}
impl<T: BingleApi + BingleApiInternal> BingleApiBoth for T {}

pub type BingleApiBothType = std::sync::Weak<dyn BingleApiBoth>;

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

/// Handler invoked when the node starts/stops listening for network messages.
/// Parameter:
/// - listening: true when the node is listening; false when it has stopped.
pub type OnListeningHandler = dyn Fn(bool) + Send + Sync + 'static;

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
    /// Debug helper: print internal start options if available.
    fn debug_print_options(&self);
    /// Returns this node's id (Algorand address), if known.
    /// Implementations should derive this from the engine issuer (issuer without suffix).
    fn get_my_id(&self) -> Option<String>;
    /// Alias for get_my_id to match external nomenclature.
    fn get_user_id(&self) -> Option<String>;
    /// Returns the configured handle, if known.
    fn get_handle(&self) -> Option<String>;
    /// Returns the configured application id, if any, from StartOptions. Preferred over env vars.
    fn get_app_id(&self) -> Option<u64>;
    /// Returns the configured Algorand provider config from StartOptions, if any.
    fn get_algo_provider_config(&self) -> Option<crate::blockchain::algo_ops::AlgoChainConfig>;
    /// Start the node using the provided options. Implementations may spawn background tasks.
    fn start(&mut self, options: &StartOptions) -> Result<(), String>;

    /// Stop all threads/tasks and release resources.
    fn stop(&mut self);

    /// indicates the network connection has changed and we need to rescan for IP address/port.
    fn network_change(&mut self);

    /// Lookup the handle in the Algorand blockchain and return the associated id.
    /// If multiple entries exist, the oldest one is returned.
    /// Returns Ok(Some(id)) if found, Ok(None) if not found, or Err on failure.
    fn handle_lookup(&self, handle: &Handle) -> Result<Option<UserId>, String>;

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

    /// Set or clear the onListening callback. Pass None to clear.
    fn set_on_listening(&mut self, handler: Option<Arc<OnListeningHandler>>);
}
