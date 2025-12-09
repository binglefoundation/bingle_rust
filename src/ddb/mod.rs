use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

/// DDB backend trait: minimal CRUD required by issue
pub trait DdbBackend {
    /// Insert or update an AdvertRecord, using its id as the key
    fn upsert(&mut self, record: AdvertRecord);
    /// Delete a record by id (no-op if missing)
    fn delete(&mut self, id: &str);
    /// Lookup and return a copy of the record, if it exists
    fn lookup(&self, id: &str) -> Option<AdvertRecord>;
}

/// Simple in-memory DDB backend backed by a HashMap
#[derive(Default)]
pub struct InMemoryDdbBackend {
    map: HashMap<String, AdvertRecord>,
}

impl InMemoryDdbBackend {
    pub fn new() -> Self { Self { map: HashMap::new() } }
}

impl DdbBackend for InMemoryDdbBackend {
    fn upsert(&mut self, record: AdvertRecord) {
        self.map.insert(record.id.clone(), record);
    }

    fn delete(&mut self, id: &str) {
        let _ = self.map.remove(id);
    }

    fn lookup(&self, id: &str) -> Option<AdvertRecord> {
        self.map.get(id).cloned()
    }
}
