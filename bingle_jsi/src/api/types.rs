use std::collections::HashMap;

/// Hostname/IP and port pair.
#[derive(uniffi::Record, Debug, Clone)]
pub struct InetSocketAddress {
    pub host: String,
    pub port: u16,
}

/// Identifies where to send network traffic (direct or via relay).
#[derive(uniffi::Record, Debug, Clone)]
pub struct NetworkSourceKey {
    pub inet_socket_address: Option<InetSocketAddress>,
    pub relay_channel: Option<u16>,
    pub relay_address: Option<InetSocketAddress>,
    pub relay_id: Option<String>,
}

/// A Bingle message — plain text or typed.
#[derive(uniffi::Record, Debug, Clone)]
pub struct BingleMessage {
    pub app: Option<String>,
    pub r#type: Option<String>,
    pub tag: Option<String>,
    pub response_tag: Option<String>,
    pub text: Option<String>,
    pub data: Option<String>,
    /// The cipher suite negotiated for the DTLS session on which this message was received.
    /// Derived by the receiving client from the connection; not transmitted on the wire.
    pub cipher_suite: Option<String>,
}

/// Server version information.
#[derive(uniffi::Record, Debug, Clone)]
pub struct VersionInfo {
    pub version: String,
    pub git_sha: Option<String>,
    pub build_timestamp: String,
    pub build_number: String,
}

pub type VersionsMap = HashMap<String, VersionInfo>;

/// An Algorand keypair (address + passphrase).
#[derive(uniffi::Record, Debug, Clone)]
pub struct Keypair {
    pub id: String,
    pub passphrase: String,
}

/// Result of a partial (prefix) handle lookup.
///
/// `id` is the Algorand address of the matching account; `canonical_handle` is the
/// handle exactly as written in that account's blockchain local state.
#[derive(uniffi::Record, Debug, Clone)]
pub struct HandleLookupPartialResult {
    pub id: String,
    pub canonical_handle: String,
}

/// How a contact was added.
#[derive(uniffi::Enum, Debug, Clone, PartialEq)]
pub enum ContactSource {
    Manual,
    Received,
}

/// A contact entry.
#[derive(uniffi::Record, Debug, Clone)]
pub struct Contact {
    pub handle: String,
    pub id: String,
    pub fields: HashMap<String, String>,
}

/// Typed cause of a send failure, exposed to the client (issue #99).
///
/// Mirrors `bingle_core`'s `SendFailureKind` (named `FailureKind` here to match the other FFI
/// enums, which drop the crate-internal prefix — cf. `NatType`, `KeypairStatus`). A client uses
/// this to process failures reliably — distinguishing e.g. an unknown handle from a recipient who
/// is simply offline — instead of parsing the human-readable `failure_reason` string. Whether a
/// kind is transient/retryable is derived, not stored: call [`failure_kind_is_retryable`].
#[derive(uniffi::Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// The handle is not registered, so it resolves to no account (permanent).
    HandleNotFound,
    /// Resolving the handle failed because the blockchain/node call errored (transient).
    HandleLookupFailed,
    /// The recipient has no advert record: they are not currently connected (transient).
    RecipientNotAdvertised,
    /// The recipient address is not valid (permanent).
    InvalidRecipientId,
    /// No relay was available to route the message (transient).
    NoRelayAvailable,
    /// A relay channel could not be allocated for the recipient (transient).
    RelayAllocationFailed,
    /// The recipient's endpoint resolved but the peer could not be reached (transient).
    PeerUnreachable,
    /// A request was sent but the peer or relay did not answer in time (transient).
    NoResponse,
    /// The recipient's connection record was present but unusable (permanent).
    MalformedAdvert,
    /// A peer returned an unexpected/protocol-invalid response (permanent).
    ProtocolError,
    /// The local engine or account was not ready to send (transient).
    NotReady,
    /// The cause was not captured (treated as permanent).
    Unknown,
}

impl FailureKind {
    /// Map this FFI kind back to the canonical `bingle_core` kind, so the retryable classification
    /// stays single-sourced in `SendFailureKind::is_retryable` rather than duplicated here.
    fn to_core(self) -> bingle_core::api::bingle_api::SendFailureKind {
        use bingle_core::api::bingle_api::SendFailureKind as K;
        match self {
            FailureKind::HandleNotFound => K::HandleNotFound,
            FailureKind::HandleLookupFailed => K::HandleLookupFailed,
            FailureKind::RecipientNotAdvertised => K::RecipientNotAdvertised,
            FailureKind::InvalidRecipientId => K::InvalidRecipientId,
            FailureKind::NoRelayAvailable => K::NoRelayAvailable,
            FailureKind::RelayAllocationFailed => K::RelayAllocationFailed,
            FailureKind::PeerUnreachable => K::PeerUnreachable,
            FailureKind::NoResponse => K::NoResponse,
            FailureKind::MalformedAdvert => K::MalformedAdvert,
            FailureKind::ProtocolError => K::ProtocolError,
            FailureKind::NotReady => K::NotReady,
            FailureKind::Unknown => K::Unknown,
        }
    }
}

/// Whether a message that failed with `kind` will keep being retried (transient) or is permanent.
///
/// Exposed as a helper (issue #99 review) so the retryable/permanent split is *derived* from the
/// `FailureKind` rather than stored as a duplicate field on every [`Message`]. Delegates to
/// `bingle_core`'s single source of truth so the two cannot drift.
#[uniffi::export]
pub fn failure_kind_is_retryable(kind: FailureKind) -> bool {
    kind.to_core().is_retryable()
}

/// A stored message.
#[derive(uniffi::Record, Debug, Clone)]
pub struct Message {
    pub sender_handle: String,
    pub recipient_handles: Vec<String>,
    pub timestamp: i64,
    pub text: String,
    /// The cipher suite negotiated for the DTLS session on which this message was received.
    /// Derived by the receiving client from the connection; not transmitted on the wire.
    pub cipher_suite: Option<String>,
    pub progress: Option<f32>,
    /// Human-readable failure reason for display. Unchanged from before issue #99; kept so existing
    /// clients keep working.
    pub failure_reason: Option<String>,
    /// Typed failure cause (issue #99) for reliable processing. `None` while pending or delivered.
    /// Derive whether it is retryable with [`failure_kind_is_retryable`].
    pub failure_kind: Option<FailureKind>,
    /// Sender-stamped send time (epoch milliseconds) from a Sidewinder store-and-forward envelope
    /// (issue #204). `None` for a live message delivered over the Bingle DTLS session.
    pub sent_time: Option<i64>,
    /// Receiver's local clock (epoch milliseconds) when the message was fetched from the Sidewinder
    /// Mailbox (issue #204). Locally stamped; not on either transport. `None` for live messages.
    pub delivered_time: Option<i64>,
    /// Base64-encoded Ed25519 sender signature retained from the store-and-forward envelope, for
    /// later attachment to a content report (issue #94). `None` when no signed envelope was opened.
    pub signature: Option<String>,
}

/// Keypair funding / registration status.
#[derive(uniffi::Enum, Debug, Clone, PartialEq)]
pub enum KeypairStatus {
    None,
    Unfunded,
    Funded,
    Active,
    /// The configured app has been superseded by a newer deployment; the client must be
    /// upgraded before it can run.
    UpgradeRequired,
}

/// Full keypair status response.
#[derive(uniffi::Record, Debug, Clone)]
pub struct KeypairStatusResponse {
    pub status: KeypairStatus,
    pub id: Option<String>,
    pub handle: Option<String>,
    pub required_algo: Option<f64>,
    /// True when `status` is a last-known value returned during a blockchain outage rather than a
    /// fresh on-chain read (issue #18 A2 / #31). The UI can surface this as "account status
    /// unavailable" instead of implying the value was just confirmed.
    pub stale: bool,
}

/// Detected NAT type.
#[derive(uniffi::Enum, Debug, Clone, PartialEq)]
pub enum NatType {
    Unknown,
    NoConnection,
    Symmetric,
    Restricted,
    FullCone,
}

/// NAT type response.
#[derive(uniffi::Record, Debug, Clone)]
pub struct NatTypeResponse {
    pub nat_type: NatType,
}

/// Configuration for initializing the Bingle JSI API.
///
/// Each field corresponds to a command-line parameter of the bingle_webserver.
/// When generated as TypeScript, this becomes a plain object with typed properties.
#[derive(uniffi::Record, Debug, Clone)]
pub struct BingleJsiConfig {
    /// The user's unique handle (required unless `local` is set).
    pub handle: Option<String>,
    /// Algorand passphrase.
    pub passphrase: Option<String>,
    /// Become a relay node.
    pub relay: bool,
    /// Static external IP as `ip:port`.
    pub static_ip: Option<String>,
    /// Comma-separated STUN server list.
    pub stun_servers: Option<String>,
    /// File containing STUN servers.
    pub stun_servers_file: Option<String>,
    /// Algorand node configuration file path.
    pub node_file: Option<String>,
    /// Log level: trace|debug|info|warn|error.
    pub log_level: Option<String>,
    /// Algorand application id.
    pub app_id: Option<u64>,
    /// Algorand asset id.
    pub asset_id: Option<u64>,
    /// Cache expiry for handle lookups in seconds.
    pub handle_cache_expiry_secs: Option<u64>,
    /// Enable debug mode.
    pub debug: bool,
    /// Enable local API with the given state file path.
    pub local: Option<String>,
    /// Notify gateway base URL for the give-up nudge (bingle_notify #11). When set (and local mode
    /// is enabled), a message give-up POSTs a content-free `/alert` to `{url}/alert`. When null the
    /// nudge is dormant — nothing is signed or sent.
    pub notify_gateway_url: Option<String>,
    /// Override for the give-up nudge feature gate (bingle_notify #11). `null` keeps the default
    /// (enabled); `false` disables the nudge even when a gateway URL is set.
    pub notify_on_giveup: Option<bool>,
    /// APNs environment this build's device tokens belong to: `"sandbox"` (Xcode/dev builds) or
    /// `"production"` (TestFlight/App Store). Used as the `env` when the app registers its device
    /// token via `/register`. `null` defaults to `"sandbox"`.
    pub notify_env: Option<String>,
    /// Base URL of the Sidewinder node for store-and-forward (epic #200), for example
    /// `http://host:9101`. When set together with `sidewinder_token` (and local mode is enabled), the
    /// offline path can post to and read from the recipient Mailbox. `null` leaves store-and-forward
    /// unconfigured.
    pub sidewinder_node_url: Option<String>,
    /// Bearer token for the Sidewinder node's client endpoints (the v0.0.2 fixed shared token,
    /// Sidewinder #164). Required alongside `sidewinder_node_url`; `null` leaves store-and-forward
    /// unconfigured.
    pub sidewinder_token: Option<String>,
}
