use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use bingle_core::api::bingle_api::BingleError;

/// Enum describing how a contact was added to the local store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContactSource {
    Manual,
    Received,
}

/// Contact information stored locally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub handle: String,
    pub id: String,
    /// Arbitrary additional fields (e.g., platform-specific metadata)
    pub fields: HashMap<String, String>,
}

/// Message record stored locally.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub sender_handle: String,
    pub recipient_handles: Vec<String>,
    /// Timestamp (e.g., epoch millis)
    pub timestamp: i64,
    pub text: String,
    /// The cipher suite negotiated for the DTLS session on which this message was received.
    /// Derived by the receiving client from the connection; not transmitted on the wire.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cipher_suite: Option<String>,
    // Delivery tracking
    #[serde(default)]
    pub progress: Option<f32>, // 0.0 to 1.0 (1.0 = completed/sent/failed-permanently)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
}

/// Generated Algorand keypair details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keypair {
    /// Algorand address (public id)
    pub id: String,
    /// Algorand mnemonic passphrase
    pub passphrase: String,
}

/// Required ALGO balance (in ALGOs) for a keypair to be considered funded.
pub const REQUIRED_ALGO: f64 = 1.5;

/// Result of checking the keypair status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeypairStatus {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    #[serde(rename = "requiredAlgo", skip_serializing_if = "Option::is_none")]
    pub required_algo: Option<f64>,
    /// True when this is a last-known status returned during a blockchain outage rather than a
    /// fresh read (issue #18 A2 / #31). Lets the UI show "account status unavailable" instead of
    /// implying the cached value was just confirmed on-chain. Defaults to false (a fresh read).
    #[serde(default)]
    pub stale: bool,
}

/// Trait describing the local API for storing messages and contacts.
/// Do not provide default method implementations per project guidelines.
pub trait BingleLocalApi: Send + Sync {
    /// Generate a new Algorand keypair (id and passphrase) and set it as current.
    fn generate_keypair(&mut self) -> Result<Keypair, BingleError>;

    /// Register the current keypair with Bingle (requires credited funds).
    /// Returns Ok(()) on success, or Err(message) on failure.
    /// Parameter:
    /// - handle: the user's unique handle to register on-chain
    fn register_keypair(&self, handle: String) -> Result<bool, BingleError>;

    /// Migrate this account's local state to the configured (new) app from a blessed ancestor
    /// app, if needed. Intended to be called once at activation, before deciding the account
    /// needs a fresh registration. Returns Ok(Some(txid)) if a migration was performed,
    /// Ok(None) if the account is already registered on the new app or there is nothing to
    /// migrate (a genuinely fresh install). The migration is user-signed.
    fn ensure_local_migrated(&self) -> Result<Option<String>, BingleError>;

    /// Get an AlgoOps instance configured with the current keypair.
    fn get_algo_ops(&self) -> Result<bingle_core::blockchain::algo_ops::AlgoOps, BingleError>;

    /// Add a contact to the local store.
    fn add_contact(
        &mut self,
        handle: String,
        id: String,
        source: ContactSource,
    ) -> Result<(), BingleError>;

    /// Block a contact by id.
    fn block_contact(&mut self, id: String) -> Result<(), BingleError>;

    /// Remove a contact by id (without blocking it).
    fn remove_contact(&mut self, id: String) -> Result<(), BingleError>;

    /// Check if a contact id is blocked.
    fn is_blocked(&self, id: &str) -> Result<bool, BingleError>;

    /// Get the list of unblocked contacts.
    fn get_contacts(&self) -> Result<Vec<Contact>, BingleError>;

    /// Add a message to the local store.
    fn add_message(
        &mut self,
        sender_handle: String,
        recipient_handles: Vec<String>,
        timestamp: i64,
        text: String,
        cipher_suite: Option<String>,
    ) -> Result<(), BingleError>;

    /// Queue a message to be sent by the background processor.
    fn queue_message(
        &mut self,
        recipient_handles: Vec<String>,
        text: String,
    ) -> Result<(), BingleError>;

    /// Update the status of a message.
    fn update_message_status(
        &mut self,
        timestamp: i64,
        progress: f32,
        failure_reason: Option<String>,
    ) -> Result<(), BingleError>;

    /// Get all messages that are pending (progress < 1.0).
    fn get_pending_messages(&self) -> Result<Vec<Message>, BingleError>;

    /// Get the list of stored messages.
    fn get_messages(&self) -> Result<Vec<Message>, BingleError>;

    /// Save all local state to a JSON file at the given path.
    fn save(&self, path: &str) -> Result<(), BingleError>;

    /// Load all local state from a JSON file at the given path, replacing current state.
    fn load(&mut self, path: &str) -> Result<(), BingleError>;

    /// Whether the configured Algorand node is currently reachable.
    ///
    /// Probes the node (an `account_balance` read, the same check `start` uses) and maps an
    /// unreachable-host error to `false`. Returns `false` when there is no keypair yet (nothing to
    /// send or register). This is a **blockchain-node** reachability probe — used to gate actions
    /// that genuinely need the chain (registration/funding). It is *not* the send-availability
    /// signal: message delivery uses the P2P transport, so that is answered at the JSI layer from
    /// the transport state (issue #31), not from this probe. Results are cached for a short
    /// interval; pass `force_recheck = true` to bypass the cache (e.g. on a network-change event).
    fn network_available(&self, force_recheck: bool) -> Result<bool, BingleError>;

    /// Check the status of the current keypair.
    /// Returns a KeypairStatus indicating None, UNFUNDED, FUNDED, or ACTIVE.
    fn keypair_status(&self) -> Result<KeypairStatus, BingleError>;

    /// Return the current keypair, if one has been generated.
    fn get_keypair(&self) -> Result<Option<Keypair>, BingleError>;
}
