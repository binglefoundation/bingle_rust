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
}

/// Server version information.
#[derive(uniffi::Record, Debug, Clone)]
pub struct VersionInfo {
    pub version: String,
    pub git_sha: Option<String>,
    pub build_timestamp: String,
    pub build_number: String,
}

/// An Algorand keypair (address + passphrase).
#[derive(uniffi::Record, Debug, Clone)]
pub struct Keypair {
    pub id: String,
    pub passphrase: String,
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
}

/// Keypair funding / registration status.
#[derive(uniffi::Enum, Debug, Clone, PartialEq)]
pub enum KeypairStatus {
    None,
    Unfunded,
    Funded,
    Active,
}

/// Full keypair status response.
#[derive(uniffi::Record, Debug, Clone)]
pub struct KeypairStatusResponse {
    pub status: KeypairStatus,
    pub id: Option<String>,
    pub handle: Option<String>,
    pub required_algo: Option<f64>,
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
