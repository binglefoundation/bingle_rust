use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod client;

pub use client::{DdbClient, DdbClientImpl, NullDdbClient};

/// InetSocketAddress as defined in BINGLE_SPEC.md
/// Hostname/IP and UDP port number.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InetSocketAddress {
    pub host: String,
    pub port: u16,
}

impl From<std::net::SocketAddr> for InetSocketAddress {
    fn from(addr: std::net::SocketAddr) -> Self {
        Self {
            host: addr.ip().to_string(),
            port: addr.port(),
        }
    }
}

impl std::convert::TryFrom<InetSocketAddress> for std::net::SocketAddr {
    type Error = std::net::AddrParseError;
    fn try_from(val: InetSocketAddress) -> Result<Self, Self::Error> {
        format!("{}:{}", val.host, val.port).parse()
    }
}

impl std::fmt::Display for InetSocketAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

impl std::str::FromStr for InetSocketAddress {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let addr: std::net::SocketAddr = s.parse().map_err(|e| format!("{}", e))?;
        Ok(Self::from(addr))
    }
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

    /// Get current number of records in the backend.
    fn len(&self) -> usize;
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
        // Collect relay records preserving association between id and endpoint
        let mut rels: Vec<(String, Option<InetSocketAddress>)> = self
            .map
            .values()
            .filter(|rec| rec.am_relay.unwrap_or(false))
            .map(|rec| (rec.id.clone(), rec.endpoint.clone()))
            .collect();
        // Deterministic ordering by id
        rels.sort_by(|a, b| a.0.cmp(&b.0));
        let ids: Vec<String> = rels.iter().map(|(id, _)| id.clone()).collect();
        // Only include endpoints when all relays have one; keep order aligned with ids
        if rels.iter().all(|(_, ep)| ep.is_some()) {
            let endpoints: Vec<InetSocketAddress> = rels.into_iter().map(|(_, ep)| ep.expect("checked" )).collect();
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
        tracing::info!("[InMemoryDdbBackend::handle_init] nsk={} user_id={} response_tag={:?} db_size={}", nsk, user_id, response_tag, self.map.len());
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
                    response_tag: response_tag.clone(),
                    text: None,
                    data: None,
                }
            )
        );
        let init_json = crate::messages::marshal::to_json_value(&init_resp);
        tracing::info!("[InMemoryDdbBackend::handle_init] sending InitResponse: {}", init_json);
        let _ = sender(nsk, user_id, init_json);

        // Send one DumpResolve per record
        for rec in records.into_iter() {
            let dump = crate::messages::types::Message::Ddb(
                crate::messages::types::DdbMessage::DumpResolve(
                    crate::messages::types::DdbDumpResolve { app: "ddb".into(), record: rec, tag: None, text: None, data: None }
                )
            );
            let dump_json = crate::messages::marshal::to_json_value(&dump);
            tracing::info!("[InMemoryDdbBackend::handle_init] sending DumpResolve: {}", dump_json);
            let _ = sender(nsk, user_id, dump_json);
        }
        tracing::info!("[InMemoryDdbBackend::handle_init] done, sent {} DumpResolve messages", db_count);
    }

    fn len(&self) -> usize {
        self.map.len()
    }
}
