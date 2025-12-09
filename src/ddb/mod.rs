use serde::{Deserialize, Serialize};

/// InetSocketAddress as defined in BINGLE_SPEC.md
/// Hostname/IP and UDP port number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InetSocketAddress {
    pub host: String,
    pub port: u16,
}

/// Advertisement record for a node (DDB AdvertRecord)
/// See generated/BINGLE_SPEC.md
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdvertRecord {
    /// Identifier of the node (Algorand address)
    pub id: String,
    /// Optional network endpoint (IP/hostname and port) for direct DTLS
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<InetSocketAddress>,
    /// True if this node provides relay services
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amRelay: Option<bool>,
    /// Optional identifier of the relay this node uses
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relayId: Option<String>,
    /// Optional signature from the relay verifying this record
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relaySig: Option<String>,
    /// Record creation or update timestamp (RFC 3339 date-time)
    pub date: String,
    /// Optional signature of this record by the node
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sig: Option<String>,
}
