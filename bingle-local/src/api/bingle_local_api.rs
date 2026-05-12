use std::collections::HashMap;
use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub sender_handle: String,
    pub recipient_handles: Vec<String>,
    /// Timestamp (e.g., epoch millis)
    pub timestamp: i64,
    pub text: String,
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
}

/// Trait describing the local API for storing messages and contacts.
/// Do not provide default method implementations per project guidelines.
pub trait BingleLocalApi: Send + Sync {
    /// Generate a new Algorand keypair (id and passphrase) and set it as current.
    fn generate_keypair(&mut self) -> Result<Keypair, String>;

    /// Register the current keypair with Bingle (requires credited funds).
    /// Returns Ok(()) on success, or Err(message) on failure.
    /// Parameter:
    /// - handle: the user's unique handle to register on-chain
    fn register_keypair(&self, handle: String) -> Result<(), String>;

    /// Get an AlgoOps instance configured with the current keypair.
    fn get_algo_ops(&self) -> Result<rust_comms::blockchain::algo_ops::AlgoOps, String>;

    /// Add a contact to the local store.
    fn add_contact(&mut self, handle: String, id: String, source: ContactSource) -> Result<(), String>;

    /// Block a contact by id.
    fn block_contact(&mut self, id: String) -> Result<(), String>;

    /// Remove a contact by id (without blocking it).
    fn remove_contact(&mut self, id: String) -> Result<(), String>;

    /// Check if a contact id is blocked.
    fn is_blocked(&self, id: &str) -> Result<bool, String>;

    /// Get the list of unblocked contacts.
    fn get_contacts(&self) -> Result<Vec<Contact>, String>;

    /// Add a message to the local store.
    fn add_message(
        &mut self,
        sender_handle: String,
        recipient_handles: Vec<String>,
        timestamp: i64,
        text: String,
    ) -> Result<(), String>;

    /// Get the list of stored messages.
    fn get_messages(&self) -> Result<Vec<Message>, String>;

    /// Save all local state to a JSON file at the given path.
    fn save(&self, path: &str) -> Result<(), String>;

    /// Load all local state from a JSON file at the given path, replacing current state.
    fn load(&mut self, path: &str) -> Result<(), String>;

    /// Check the status of the current keypair.
    /// Returns a KeypairStatus indicating None, UNFUNDED, FUNDED, or ACTIVE.
    fn keypair_status(&self) -> Result<KeypairStatus, String>;

    /// Return the current keypair, if one has been generated.
    fn get_keypair(&self) -> Result<Option<Keypair>, String>;
}
