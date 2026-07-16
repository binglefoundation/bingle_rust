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
    pub failure_reason: Option<String>,
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
}
