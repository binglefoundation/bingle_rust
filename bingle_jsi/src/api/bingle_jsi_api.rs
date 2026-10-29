use crate::api::callback::{
    ListeningCallback, LogCallback, MailboxCheckCallback, MessageCallback, PushRegistrationCallback,
};
use crate::api::error::BingleJsiError;
use crate::api::types::{
    BingleMessage, Contact, ContactSource, HandleLookupPartialResult, Keypair,
    KeypairStatusResponse, Message, MessagingSettings, NatTypeResponse, NetworkSourceKey,
    VersionInfo,
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

    /// Partial (prefix) handle lookup.
    ///
    /// The handle is normalised by the handle matching rules and matched against the start
    /// of registered handles, so "abc" matches a registered "ab_cd". Returns the first
    /// (oldest) hit as an (id, canonical_handle) pair, where canonical_handle is the handle
    /// exactly as written in the blockchain local state. Errors with NotFound if no handle
    /// starts with the given prefix.
    fn handle_lookup_partial(
        &self,
        handle: String,
    ) -> Result<HandleLookupPartialResult, BingleJsiError>;

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

    /// Get version information for all library modules.
    fn get_versions(
        &self,
    ) -> Result<std::collections::HashMap<String, VersionInfo>, BingleJsiError>;

    /// Get the current detected NAT type.
    fn get_nat_type(&self) -> Result<NatTypeResponse, BingleJsiError>;

    // ── Local storage and contacts ───────────────────────────────────

    /// Generate a new Algorand keypair and set it as current.
    fn generate_keypair(&self) -> Result<Keypair, BingleJsiError>;

    /// Import an existing account from its 25-word Algorand mnemonic passphrase and set it as
    /// current. Errors if the passphrase is not a valid mnemonic.
    fn import_keypair(&self, passphrase: String) -> Result<Keypair, BingleJsiError>;

    /// Register the current keypair with Bingle using the provided handle.
    fn register_keypair(&self, handle: String) -> Result<(), BingleJsiError>;

    /// Sign the canonical bingle-notify envelope with the active keypair and return the base64
    /// signature. This is the signing primitive the RN app (which has no algosdk) uses to build
    /// the `/register` (and `/alert`) envelopes the notify gateway verifies.
    ///
    /// The signed message is the fixed, newline-delimited UTF-8 string
    /// `"bingle-notify:v1"\nroute\niss\naudience\nbodyHash\nnonce\nexp`, signed as
    /// `Ed25519(sk, "MX" || msg)` (the Algorand byte-signing prefix). `bodyHash` is the lowercase
    /// hex SHA-256 of the route-specific body: for `"register"`, `sha256(token + "\n" + env)`, with
    /// `"\n" + topic` appended when `topic` (the app's APNs topic, its bundle ID) is given; for
    /// `"alert"`, `sha256("")` (in which case `audience` is the recipient handle, `token`/`env` are
    /// ignored and `topic` must be `None`). Errors if no keypair is set.
    #[allow(clippy::too_many_arguments)]
    fn sign_notify_envelope(
        &self,
        route: String,
        iss: String,
        audience: String,
        token: String,
        env: String,
        topic: Option<String>,
        nonce: String,
        exp: i64,
    ) -> Result<String, BingleJsiError>;

    /// Sign the client's transactions in an app-store registration group built by the registration
    /// service (issue #344), with the current keypair. `transactions` is the service's whole
    /// unsigned group (base64 msgpack) and `sign_indexes` the positions the client signs. Every
    /// client transaction is checked against the registration template first (Bingle$ opt-in,
    /// Bingle DApp opt-in, 1 Bingle$ to the DApp, `register(handle)` with exactly this `handle`, the
    /// configured app and asset), and anything unexpected (receivers, amounts, rekeys, close-to
    /// addresses, extra fields) is refused with nothing signed. Returns the signed transactions as
    /// base64 msgpack, in `sign_indexes` order, for the service's `signed_transactions`.
    fn sign_registration_group(
        &self,
        transactions: Vec<String>,
        sign_indexes: Vec<u32>,
        handle: String,
    ) -> Result<Vec<String>, BingleJsiError>;

    /// Start (or resume) a handle clash recovery (issue #344): return the pending recovery keypair,
    /// the fresh account the current one moves to, whose address is the recovery request's
    /// `new_address`. It is generated offline, kept beside the current keypair and saved to the
    /// state file before this returns; if it can't be saved, this errors. Calling it again before
    /// [`complete_recovery_keypair`](Self::complete_recovery_keypair) returns the same keypair.
    fn begin_recovery_keypair(&self) -> Result<Keypair, BingleJsiError>;

    /// Sign the client's transactions in a handle clash recovery group (issue #344): positions 0–2
    /// with the current keypair and 3, 5, 6 and 7 with the pending recovery keypair. The template
    /// check is as for [`sign_registration_group`](Self::sign_registration_group), plus the old
    /// account's ClearState, its Bingle$ close-out to the funder (the sender of position 4's
    /// 1 Bingle$ transfer to the new address) and its Algo close to the new address. Errors if no
    /// recovery has begun.
    fn sign_recovery_group(
        &self,
        transactions: Vec<String>,
        sign_indexes: Vec<u32>,
        handle: String,
    ) -> Result<Vec<String>, BingleJsiError>;

    /// Finish a handle clash recovery once the registration service reports it `registered`: make
    /// the pending recovery keypair current and save it. Errors if no recovery is pending.
    fn complete_recovery_keypair(&self) -> Result<Keypair, BingleJsiError>;

    /// Ask the host to begin iOS push registration (bingle_notify #i). Invokes the registered
    /// [`PushRegistrationCallback`] so the thin Swift bridge performs the platform calls
    /// (`requestAuthorization` + `registerForRemoteNotifications`). Returns once the callback has
    /// been dispatched; the token arrives asynchronously via [`register_apns_token`]. Errors if no
    /// push-registration callback is set.
    fn request_push_registration(&self) -> Result<(), BingleJsiError>;

    /// Hand the raw APNs device token (exactly as iOS delivered it) to Rust, which hex-encodes,
    /// signs, and POSTs the `/register` envelope to the notify gateway (bingle_notify #i). The Swift
    /// bridge forwards the bytes and nothing else. Returns whether the gateway accepted it. Errors
    /// if the token is empty, there is no keypair/handle to sign with, or no gateway URL is
    /// configured.
    fn register_apns_token(&self, token: Vec<u8>) -> Result<bool, BingleJsiError>;

    /// Report that iOS push registration failed (permission denied or APNs error). Logged for
    /// diagnostics; there is no token to register.
    fn apns_registration_failed(&self, reason: String);

    /// Add a contact to the local store.
    fn add_contact(
        &self,
        handle: String,
        id: String,
        source: ContactSource,
    ) -> Result<(), BingleJsiError>;

    /// Block a contact by id. Its live and Mailbox messages are dropped on receipt.
    fn block_contact(&self, id: String) -> Result<(), BingleJsiError>;

    /// Unblock a previously blocked contact by id, so its messages are received again.
    fn unblock_contact(&self, id: String) -> Result<(), BingleJsiError>;

    /// Remove a contact by id without blocking it.
    fn remove_contact(&self, id: String) -> Result<(), BingleJsiError>;

    /// Check if a contact id is blocked.
    fn is_blocked(&self, id: String) -> Result<bool, BingleJsiError>;

    /// Get the list of unblocked contacts.
    fn get_contacts(&self) -> Result<Vec<Contact>, BingleJsiError>;

    /// Get the list of blocked contacts.
    fn get_blocked_contacts(&self) -> Result<Vec<Contact>, BingleJsiError>;

    /// Add a message to the local store.
    fn add_message(
        &self,
        sender_handle: String,
        recipient_handles: Vec<String>,
        timestamp: i64,
        text: String,
        cipher_suite: Option<String>,
    ) -> Result<(), BingleJsiError>;

    /// Get the stored messages, ordered by send time (issue #69): the sender's `sent_time` when known,
    /// else the Mailbox `delivered_time`, else `timestamp`. Equal times keep their stored order.
    fn get_messages(&self) -> Result<Vec<Message>, BingleJsiError>;

    /// Queue a message to be sent by the background processor.
    fn queue_message(
        &self,
        recipient_handles: Vec<String>,
        text: String,
    ) -> Result<(), BingleJsiError>;

    /// Update the status of the message stored at `timestamp`. Deprecated (issue #209): use
    /// [`update_message_status_by_id`](Self::update_message_status_by_id). Kept so apps that key
    /// messages by timestamp keep working until they migrate.
    fn update_message_status(
        &self,
        timestamp: i64,
        progress: f32,
        failure_reason: Option<String>,
    ) -> Result<(), BingleJsiError>;

    /// Update the status of the message with this `id` (issue #209).
    fn update_message_status_by_id(
        &self,
        id: String,
        progress: f32,
        failure_reason: Option<String>,
    ) -> Result<(), BingleJsiError>;

    /// Whether the network is currently available for sending (issue #31).
    ///
    /// Reflects the P2P transport only: `true` when listening with a usable route, `false` when
    /// not listening or when the engine reports `NoConnection`. It is deliberately independent of
    /// Algorand-node reachability — messages are delivered over the transport (DTLS relays) with
    /// cached handle lookups, so a node outage does not make sending unavailable. `force_recheck`
    /// is accepted for API compatibility but not needed (the transport state is always current).
    /// The app can use this to decide whether to send a message now or queue it as pending.
    fn network_available(&self, force_recheck: bool) -> Result<bool, BingleJsiError>;

    /// Check the status of the local keypair.
    fn keypair_status(&self) -> Result<KeypairStatusResponse, BingleJsiError>;

    // ── Runtime messaging settings (store-and-forward + notify, story #242) ───────────────

    /// Set the store-and-forward send / receive gates on the **running** session — the Settings-screen
    /// toggle. Takes effect on the next send/poll without a re-init (keypair, contacts, history and
    /// active connections are preserved). Errors if local mode is not enabled.
    fn set_store_and_forward(&self, send: bool, receive: bool) -> Result<(), BingleJsiError>;

    /// Set the give-up notify nudge on the **running** session: `enabled` toggles it and `gateway_url`
    /// sets the notify gateway base URL (`null` leaves the nudge dormant even when enabled). Takes
    /// effect immediately without a re-init. Errors if local mode is not enabled.
    fn set_notify(&self, enabled: bool, gateway_url: Option<String>) -> Result<(), BingleJsiError>;

    /// The current effective messaging settings (store-and-forward gates + notify), so the Settings
    /// screen can render live state. Errors if local mode is not enabled.
    fn messaging_settings(&self) -> Result<MessagingSettings, BingleJsiError>;

    /// Save all local state to a JSON file.
    fn save(&self, path: String) -> Result<(), BingleJsiError>;

    /// Load all local state from a JSON file.
    fn load(&self, path: String) -> Result<(), BingleJsiError>;

    // ── Callbacks ────────────────────────────────────────────────────

    /// Register a callback to be invoked on each incoming message.
    /// Replaces any previously registered callback.
    fn set_message_callback(&self, callback: Box<dyn MessageCallback>);

    /// Register a callback to be invoked on each log message.
    /// Replaces any previously registered callback.
    /// The callback receives timestamp (ms since epoch), level, and message.
    fn set_log_callback(&self, callback: Box<dyn LogCallback>);

    /// Register a callback to be invoked when the engine listening state changes.
    /// Replaces any previously registered callback.
    /// The callback receives a boolean (listening) and the NAT type as a string.
    fn set_listening_callback(&self, callback: Box<dyn ListeningCallback>);

    /// Register a callback invoked when [`request_push_registration`] asks the host to start iOS
    /// push registration. Replaces any previously registered callback.
    fn set_push_registration_callback(&self, callback: Box<dyn PushRegistrationCallback>);

    /// Register a callback invoked after each store-and-forward Mailbox check that reached the
    /// node, with the number of messages it read (issue #358). Replaces any previously registered
    /// callback.
    fn set_mailbox_check_callback(&self, callback: Box<dyn MailboxCheckCallback>);

    // ── Engine lifecycle ─────────────────────────────────────────────

    /// Start the bingle engine, enabling messaging.
    /// Requires the keypair to be in state FUNDED (or ACTIVE).
    fn start(&self) -> Result<(), BingleJsiError>;

    /// Stop the bingle engine.
    fn stop(&self) -> Result<(), BingleJsiError>;

    /// Return whether the engine has been started.
    fn is_started(&self) -> bool;

    /// Notify the engine that the host app has returned to the foreground.
    ///
    /// Used to proactively refresh the relay listener registration after a
    /// background/idle period that may have outlived the listener lease, so the
    /// node can receive inbound again without waiting for the next periodic tick
    /// (issue #50). Safe to call at any time; a no-op if not registered.
    fn foregrounding(&self);

    /// Notify the engine that the host app has gone to the background.
    ///
    /// Lets the engine pause battery-costly background work (e.g. the relay
    /// keep-alive) while suspended, and hands every undelivered message to the
    /// recipients' Mailboxes and saves the state, since the app may be killed
    /// before a pending send fails (issue #352). Blocks for that hand-off, up to
    /// [`BACKGROUND_FLUSH_TIMEOUT`](crate::api::bingle_jsi_api_impl::BACKGROUND_FLUSH_TIMEOUT),
    /// so call it off the UI thread and, on iOS, inside a background task. Safe to
    /// call at any time.
    fn backgrounding(&self);
}
