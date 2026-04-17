use crate::api::callback::MessageCallback;
use crate::api::error::BingleJsiError;
use crate::api::types::{
    BingleMessage, Contact, ContactSource, Keypair, KeypairStatusResponse,
    Message, NatTypeResponse, NetworkSourceKey, VersionInfo,
};

/// Primary Bingle API exposed over JSI / uniffi.
///
/// Every method corresponds to an endpoint in `server_openapi.yaml`.
/// Methods are intentionally left unimplemented at this stage.
#[uniffi::export]
pub trait BingleJsiApi: Send + Sync {
    // ── Core messaging ───────────────────────────────────────────────

    /// Lookup an id by handle.
    fn handle_lookup(&self, handle: String) -> Result<String, BingleJsiError>;

    /// Send a message to a user id.
    fn send_message_to_id(
        &self,
        user_id: String,
        message: BingleMessage,
    ) -> Result<bool, BingleJsiError>;

    /// Send a message to a handle.
    fn send_message_to_handle(
        &self,
        handle: String,
        message: BingleMessage,
    ) -> Result<bool, BingleJsiError>;

    /// Send a message to a network source key and user id.
    fn send_message_to_network(
        &self,
        network_source_key: NetworkSourceKey,
        user_id: String,
        message: BingleMessage,
    ) -> Result<bool, BingleJsiError>;

    /// Send a message to a user id and wait for response.
    fn send_message_to_id_with_response(
        &self,
        user_id: String,
        message: BingleMessage,
    ) -> Result<BingleMessage, BingleJsiError>;

    /// Send a message to a handle and wait for response.
    fn send_message_to_handle_with_response(
        &self,
        handle: String,
        message: BingleMessage,
    ) -> Result<BingleMessage, BingleJsiError>;

    /// Send a message to a network source key and user id and wait for response.
    fn send_message_to_network_with_response(
        &self,
        network_source_key: NetworkSourceKey,
        user_id: String,
        message: BingleMessage,
    ) -> Result<BingleMessage, BingleJsiError>;

    /// Return all received messages queued in the server.
    fn queued(&self) -> Result<Vec<BingleMessage>, BingleJsiError>;

    /// Get current server version information.
    fn version(&self) -> Result<VersionInfo, BingleJsiError>;

    /// Get the current detected NAT type.
    fn get_nat_type(&self) -> Result<NatTypeResponse, BingleJsiError>;

    // ── Local storage and contacts ───────────────────────────────────

    /// Generate a new Algorand keypair and set it as current.
    fn generate_keypair(&self) -> Result<Keypair, BingleJsiError>;

    /// Register the current keypair with Bingle using the provided handle.
    fn register_keypair(&self, handle: String) -> Result<bool, BingleJsiError>;

    /// Add a contact to the local store.
    fn add_contact(
        &self,
        handle: String,
        id: String,
        source: ContactSource,
    ) -> Result<(), BingleJsiError>;

    /// Block a contact by id.
    fn block_contact(&self, id: String) -> Result<(), BingleJsiError>;

    /// Remove a contact by id without blocking it.
    fn remove_contact(&self, id: String) -> Result<(), BingleJsiError>;

    /// Check if a contact id is blocked.
    fn is_blocked(&self, id: String) -> Result<bool, BingleJsiError>;

    /// Get the list of unblocked contacts.
    fn get_contacts(&self) -> Result<Vec<Contact>, BingleJsiError>;

    /// Add a message to the local store.
    fn add_message(
        &self,
        sender_handle: String,
        recipient_handles: Vec<String>,
        timestamp: i64,
        text: String,
    ) -> Result<(), BingleJsiError>;

    /// Get the list of stored messages.
    fn get_messages(&self) -> Result<Vec<Message>, BingleJsiError>;

    /// Check the status of the local keypair.
    fn keypair_status(&self) -> Result<KeypairStatusResponse, BingleJsiError>;

    /// Save all local state to a JSON file.
    fn save(&self, path: String) -> Result<(), BingleJsiError>;

    /// Load all local state from a JSON file.
    fn load(&self, path: String) -> Result<(), BingleJsiError>;

    // ── Callbacks ────────────────────────────────────────────────────

    /// Register a callback to be invoked on each incoming message.
    /// Replaces any previously registered callback.
    fn set_message_callback(&self, callback: Box<dyn MessageCallback>);
}
