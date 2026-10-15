use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub mod advert_record;
pub mod client;

pub use advert_record::{AdvertRecord, InetSocketAddress};
pub use client::{DdbClient, DdbClientImpl, NullDdbClient};

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

    /// Get current number of live (unexpired) records in the backend.
    fn len(&self) -> usize;

    /// Delete records whose advert has expired (issue #280) and return their ids.
    fn sweep_expired(&mut self) -> Vec<String>;
}

/// Choose which relays in an epoch snapshot to evict because they are no longer permitted to relay
/// on chain (issue #120).
///
/// `permission` reports a relay's on-chain `allow_relay` state: `Some(true)` permitted,
/// `Some(false)` not permitted (flag cleared, or not opted in to the app), `None` unknown (the
/// chain could not be read). Only a definite `Some(false)` evicts, so a chain outage never empties
/// the relay set. `my_id` is never evicted: a relay's own permission is checked when it starts.
pub fn relays_to_evict(
    relay_ids: &[String],
    my_id: &str,
    mut permission: impl FnMut(&str) -> Option<bool>,
) -> Vec<String> {
    relay_ids
        .iter()
        .filter(|id| id.as_str() != my_id)
        .filter(|id| permission(id) == Some(false))
        .cloned()
        .collect()
}

/// Remove relays that are no longer permitted to relay from `backend`, so they leave the relay set
/// that [`DdbBackend::make_epoch_info`] builds for ripple and getRelaysStatus (issue #120).
///
/// The backend lock is held only to snapshot the relay ids and to delete; `permission` (which may
/// query the chain) runs without it. See [`relays_to_evict`] for which relays are removed. Returns
/// the evicted ids.
pub fn evict_disallowed_relays<B: DdbBackend + ?Sized>(
    backend: &std::sync::Mutex<B>,
    my_id: &str,
    permission: impl FnMut(&str) -> Option<bool>,
) -> Vec<String> {
    let relay_ids = match backend.lock() {
        Ok(b) => b.make_epoch_info().0,
        Err(_) => {
            tracing::error!("[ddb::evict_disallowed_relays] DDB backend lock poisoned");
            return Vec::new();
        }
    };
    let evict = relays_to_evict(&relay_ids, my_id, permission);
    if evict.is_empty() {
        return evict;
    }
    match backend.lock() {
        Ok(mut b) => {
            for id in &evict {
                tracing::info!(
                    "[ddb::evict_disallowed_relays] removing relay id={id}: allow_relay is no longer set on chain"
                );
                b.delete(id);
            }
        }
        Err(_) => {
            tracing::error!("[ddb::evict_disallowed_relays] DDB backend lock poisoned");
            return Vec::new();
        }
    }
    evict
}

/// Default time to live of a client advert held by a relay (issue #280). A client refreshes its
/// advert every third of this, so a live client's advert never lapses; a client that stops without
/// signing off (crash, lost network) is gone within this time.
///
/// Refresh load: each refresh is one upsert sent to one relay, which ripples it to every other
/// relay, so every relay handles `3 * N / ttl` upserts per second for `N` live clients. At the
/// default of 600 s that is one upsert every 200 s per client, about 15 per second per relay for
/// 1000 clients.
pub const DEFAULT_ADVERT_TTL: Duration = Duration::from_secs(600);

/// Clock used to time adverts. Injectable so tests can move time forward (issue #280).
pub type DdbClock = Arc<dyn Fn() -> Instant + Send + Sync>;

/// The time to live configured by `requested` (from `StartOptions::advert_ttl`): the default when
/// unset, and `None` (adverts never expire, and are not refreshed) when set to zero.
pub fn effective_advert_ttl(requested: Option<Duration>) -> Option<Duration> {
    match requested {
        None => Some(DEFAULT_ADVERT_TTL),
        Some(ttl) if ttl.is_zero() => None,
        Some(ttl) => Some(ttl),
    }
}

/// The advert registration a node last made, kept so it can be repeated to refresh the advert
/// before relays expire it (issue #280).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvertRegistration {
    /// A directly reachable endpoint, as made by [`DdbClient::register_ip`].
    Ip {
        endpoint: std::net::SocketAddr,
        am_relay: bool,
    },
    /// A relayed client's home relay, as made by [`DdbClient::register_relay`].
    Relay {
        relay_id: String,
        relay_sig: Option<String>,
    },
}

impl AdvertRegistration {
    /// True when this registers a relay, whose advert relays never expire.
    pub fn is_relay(&self) -> bool {
        matches!(self, AdvertRegistration::Ip { am_relay: true, .. })
    }

    /// Repeat this registration through `client`.
    pub fn register(
        &self,
        client: &dyn DdbClient,
    ) -> Result<(), crate::api::bingle_api::BingleError> {
        match self {
            AdvertRegistration::Ip { endpoint, am_relay } => {
                client.register_ip(*endpoint, *am_relay)
            }
            AdvertRegistration::Relay {
                relay_id,
                relay_sig,
            } => client.register_relay(relay_id.clone(), relay_sig.clone()),
        }
    }
}

/// A record held by [`InMemoryDdbBackend`] with the local time it was last written.
struct StoredRecord {
    record: AdvertRecord,
    received: Instant,
}

/// Simple in-memory DDB backend backed by a HashMap.
///
/// With a time to live set, a client advert not rewritten within it is treated as absent and is
/// removed by [`DdbBackend::sweep_expired`] (issue #280). Age is measured from when this node
/// received the record, not from the record's `date`, which clients do not set and which would be
/// exposed to clock skew between devices. Relay adverts and this node's own advert never expire:
/// the relay set is kept in step with the chain by the permission refresh (issue #120), and a relay
/// dropped from its peers' sets because one refresh was late would split the ripple set.
pub struct InMemoryDdbBackend {
    map: HashMap<String, StoredRecord>,
    advert_ttl: Option<Duration>,
    clock: DdbClock,
    own_id: Option<String>,
}

impl Default for InMemoryDdbBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryDdbBackend {
    /// A backend whose records never expire.
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            advert_ttl: None,
            clock: Arc::new(Instant::now),
            own_id: None,
        }
    }

    /// A backend whose client adverts expire after `advert_ttl` (`None`: never).
    pub fn with_advert_ttl(advert_ttl: Option<Duration>) -> Self {
        Self {
            advert_ttl,
            ..Self::new()
        }
    }

    /// Replace the clock used to time adverts (for tests).
    pub fn with_clock(mut self, clock: DdbClock) -> Self {
        self.clock = clock;
        self
    }

    /// The configured time to live of client adverts, if any.
    pub fn advert_ttl(&self) -> Option<Duration> {
        self.advert_ttl
    }

    /// Set this node's own id, whose advert never expires.
    pub fn set_own_id(&mut self, own_id: &str) {
        self.own_id = Some(own_id.to_string());
    }

    fn is_expired(&self, stored: &StoredRecord, now: Instant) -> bool {
        let Some(ttl) = self.advert_ttl else {
            return false;
        };
        if stored.record.am_relay.unwrap_or(false)
            || self.own_id.as_deref() == Some(stored.record.id.as_str())
        {
            return false;
        }
        now.saturating_duration_since(stored.received) >= ttl
    }

    fn live_records(&self) -> impl Iterator<Item = &AdvertRecord> {
        let now = (self.clock)();
        self.map
            .values()
            .filter(move |s| !self.is_expired(s, now))
            .map(|s| &s.record)
    }
}

impl DdbBackend for InMemoryDdbBackend {
    fn upsert(&mut self, record: AdvertRecord) {
        let received = (self.clock)();
        self.map
            .insert(record.id.clone(), StoredRecord { record, received });
    }

    fn delete(&mut self, id: &str) {
        let _ = self.map.remove(id);
    }

    fn lookup(&self, id: &str) -> Option<AdvertRecord> {
        let now = (self.clock)();
        self.map
            .get(id)
            .filter(|s| !self.is_expired(s, now))
            .map(|s| s.record.clone())
    }

    fn make_epoch_info(&self) -> (Vec<String>, Option<Vec<InetSocketAddress>>) {
        // Collect relay records preserving association between id and endpoint
        let mut rels: Vec<(String, Option<InetSocketAddress>)> = self
            .live_records()
            .filter(|rec| rec.am_relay.unwrap_or(false))
            .map(|rec| (rec.id.clone(), rec.endpoint.clone()))
            .collect();
        // Deterministic ordering by id
        rels.sort_by(|a, b| a.0.cmp(&b.0));
        let ids: Vec<String> = rels.iter().map(|(id, _)| id.clone()).collect();
        // Only include endpoints when all relays have one; keep order aligned with ids
        if rels.iter().all(|(_, ep)| ep.is_some()) {
            let endpoints: Vec<InetSocketAddress> = rels
                .into_iter()
                .map(|(_, ep)| ep.expect("checked"))
                .collect();
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
        tracing::info!(
            "[InMemoryDdbBackend::handle_init] nsk={} user_id={} response_tag={:?} db_size={}",
            nsk,
            user_id,
            response_tag,
            self.map.len()
        );
        // Snapshot the live records to avoid holding the map across sends
        let mut records: Vec<AdvertRecord> = self.live_records().cloned().collect();
        // Deterministic ordering for tests
        records.sort_by(|a, b| a.id.cmp(&b.id));
        let db_count = records.len() as i64;

        // Send InitResponse with dbCount
        let init_resp =
            crate::messages::types::Message::Ddb(crate::messages::types::DdbMessage::InitResponse(
                crate::messages::types::DdbInitResponse {
                    app: "ddb".to_string(),
                    db_count,
                    response_tag: response_tag.clone(),
                    text: None,
                    data: None,
                },
            ));
        let init_json = crate::messages::marshal::to_json_value(&init_resp);
        tracing::info!(
            "[InMemoryDdbBackend::handle_init] sending InitResponse: {}",
            init_json
        );
        let _ = sender(nsk, user_id, init_json);

        // Send one DumpResolve per record
        for rec in records.into_iter() {
            let dump = crate::messages::types::Message::Ddb(
                crate::messages::types::DdbMessage::DumpResolve(
                    crate::messages::types::DdbDumpResolve {
                        app: "ddb".into(),
                        record: rec,
                        tag: None,
                        response_tag: None,
                        text: None,
                        data: None,
                    },
                ),
            );
            let dump_json = crate::messages::marshal::to_json_value(&dump);
            tracing::info!(
                "[InMemoryDdbBackend::handle_init] sending DumpResolve: {}",
                dump_json
            );
            let _ = sender(nsk, user_id, dump_json);
        }
        tracing::info!(
            "[InMemoryDdbBackend::handle_init] done, sent {} DumpResolve messages",
            db_count
        );
    }

    fn len(&self) -> usize {
        self.live_records().count()
    }

    fn sweep_expired(&mut self) -> Vec<String> {
        let now = (self.clock)();
        let expired: Vec<String> = self
            .map
            .values()
            .filter(|s| self.is_expired(s, now))
            .map(|s| s.record.id.clone())
            .collect();
        for id in &expired {
            self.map.remove(id);
        }
        expired
    }
}
