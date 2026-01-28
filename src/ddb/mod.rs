use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod client;

pub use client::{DdbClient, DdbClientImpl, NullDdbClient};

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
    #[serde(rename = "amRelay")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub am_relay: Option<bool>,
    /// Optional identifier of the relay this node uses
    #[serde(rename = "relayId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay_id: Option<String>,
    /// Optional signature from the relay verifying this record
    #[serde(rename = "relaySig")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay_sig: Option<String>,
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

    /// Construct relayIds and relayEndpoints for the current epoch snapshot.
    /// Returns (relay_ids, relay_endpoints) where relay_endpoints is None when no endpoints are known.
    fn make_epoch_info(&self) -> (Vec<String>, Option<Vec<InetSocketAddress>>);

    /// Handle InitResolve: take a snapshot and send InitResponse and DumpResolve messages to the requester.
    ///
    /// Implementations should not block extensively; they should iterate over a consistent snapshot of the DB.
    /// The sender callback will be used to send messages to the new peer (identified by `nsk` and `user_id`).
    fn handle_init(
        &self,
        nsk: &crate::api::bingle_api::NetworkEndpoint,
        user_id: &str,
        response_tag: Option<String>,
        sender: &dyn Fn(&crate::api::bingle_api::NetworkEndpoint, &str, serde_json::Value) -> bool,
    );
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

    fn make_epoch_info(&self) -> (Vec<String>, Option<Vec<InetSocketAddress>>) {
        // Collect all records that are relays (am_relay == Some(true))
        let mut ids: Vec<String> = Vec::new();
        let mut endpoints: Vec<InetSocketAddress> = Vec::new();
        for rec in self.map.values() {
            if rec.am_relay.unwrap_or(false) {
                ids.push(rec.id.clone());
                if let Some(ep) = &rec.endpoint {
                    endpoints.push(ep.clone());
                }
            }
        }
        // Deterministic ordering
        ids.sort();
        // Keep endpoint order aligned by sorting by host:port to avoid flakiness in tests
        if !endpoints.is_empty() {
            endpoints.sort_by(|a, b| {
                let ha = format!("{}:{}", a.host, a.port);
                let hb = format!("{}:{}", b.host, b.port);
                ha.cmp(&hb)
            });
            (ids, Some(endpoints))
        } else {
            (ids, None)
        }
    }

    fn handle_init(
        &self,
        nsk: &crate::api::bingle_api::NetworkEndpoint,
        user_id: &str,
        response_tag: Option<String>,
        sender: &dyn Fn(&crate::api::bingle_api::NetworkEndpoint, &str, serde_json::Value) -> bool,
    ) {
        // Snapshot keys to avoid holding the map across sends
        let mut records: Vec<AdvertRecord> = self.map.values().cloned().collect();
        // Deterministic ordering for tests
        records.sort_by(|a, b| a.id.cmp(&b.id));
        let db_count = records.len() as i64;

        // Send InitResponse with dbCount
        let init_resp = crate::messages::types::Message::Ddb(
            crate::messages::types::DdbMessage::InitResponse(
                crate::messages::types::DdbInitResponse {
                    app: "ddb".to_string(),
                    db_count,
                    tag: None,
                    response_tag: response_tag.clone(),
                    text: None,
                    data: None,
                }
            )
        );
        let init_json = crate::messages::marshal::to_json_value(&init_resp);
        let _ = sender(nsk, user_id, init_json);

        // Send one DumpResolve per record
        for rec in records.into_iter() {
            let dump = crate::messages::types::Message::Ddb(
                crate::messages::types::DdbMessage::DumpResolve(
                    crate::messages::types::DdbDumpResolve { app: "ddb".into(), record: rec, tag: None, response_tag: None, text: None, data: None }
                )
            );
            let dump_json = crate::messages::marshal::to_json_value(&dump);
            let _ = sender(nsk, user_id, dump_json);
        }
    }
}
