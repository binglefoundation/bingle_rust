use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use crate::themes;

use crate::api::bingle_api::NetworkEndpoint;
use crate::engine::RelayState;
use std::collections::{HashMap, HashSet};

use crate::ddb::AdvertRecord;
pub use crate::relay::relay_info_cache::RelayInfoCache;
use crate::relay::relay_updater::RelayUpdater;

#[derive(Debug, Clone)]
pub struct RelayInfo {
    pub advert_record: AdvertRecord,
    pub is_root: bool,
    pub state: Option<RelayState>, // Optional known state from RelayCheck
    pub ttl: Option<u64>,
    pub last_updated: Instant, // Time this entry was last created or updated
}

impl RelayInfo {
    pub fn id(&self) -> &str {
        &self.advert_record.id
    }

    pub fn address(&self) -> SocketAddr {
        self.advert_record
            .endpoint
            .as_ref()
            .and_then(|ep| SocketAddr::try_from(ep.clone()).ok())
            .expect("RelayInfo must have a valid address")
    }

    pub fn root(advert_record: AdvertRecord) -> Self {
        Self::root_with(advert_record, None, None)
    }

    pub fn root_with(
        advert_record: AdvertRecord,
        state: Option<RelayState>,
        ttl: Option<u64>,
    ) -> Self {
        Self {
            advert_record,
            is_root: true,
            state,
            ttl,
            last_updated: Instant::now(),
        }
    }

    pub fn non_root(advert_record: AdvertRecord) -> Self {
        Self::non_root_with(advert_record, None, None)
    }

    pub fn non_root_with(
        advert_record: AdvertRecord,
        state: Option<RelayState>,
        ttl: Option<u64>,
    ) -> Self {
        Self {
            advert_record,
            is_root: false,
            state,
            ttl,
            last_updated: Instant::now(),
        }
    }

    /// Refresh the `last_updated` timestamp to now, returning `self`.
    pub fn with_updated(mut self) -> Self {
        self.last_updated = Instant::now();
        self
    }
}

pub trait RelayFinderTrait: Send + Sync {
    fn list_all_relays(&self, my_id: &str, include_self: bool) -> Vec<RelayInfo>;
    fn find_relay(&self, my_id: &str) -> Result<RelayInfo, String>;
    fn find_relay_excluding(
        &self,
        my_id: &str,
        exclude: &[SocketAddr],
    ) -> Result<RelayInfo, String>;
    fn list_root_relays(&self, my_id: &str, include_self: bool) -> Vec<RelayInfo>;
    fn clear_state_cache(&self);
    fn lookup_root_id(&self, id: &str) -> Option<NetworkEndpoint>;
    /// Remove a relay entry from the cache by id. No-op if not present.
    fn remove_relay(&self, relay_id: &str);
}

pub trait RelayFinderTestTrait {
    fn select_indices(&self, relays: &[RelayInfo], my_id: &str) -> (usize, usize);
    fn relays_available(&self) -> HashMap<RelayState, usize>;
    fn find_root_relay(&self, my_id: &str) -> Result<RelayInfo, String>;
}

/// RelayFinder: finds and caches the preferred root relay per spec (root relays only).
///
/// Behaviour (root relays only):
/// - Discover root relays (ids with RelayIP in local state) — discovery is provided by a user-supplied closure.
/// - Compute preferred relay index by sorting relay ids lexicographically and partitioning by count as per spec.
///   For simplicity we map the caller id to a u32 by taking the first 4 bytes of its base32-decoded public key when available;
///   if parsing fails, we fall back to a hash of the id string.
/// - Perform RelayCheck against the preferred; if unavailable, try the alternate (next index modulo n).
/// - Cache the successful root relay (id + address) for cache_ttl; subsequent calls return cached until it expires.
pub struct RelayFinder {
    api: crate::api::bingle_api::BingleApiBothType,
    // Injection point for discovery (keeps this component testable and avoids tying to indexer here)
    discover_roots: Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync>,
    unavailable_relays: Mutex<HashSet<String>>,
    relay_updater: OnceLock<RelayUpdater>,
}

pub type RootRelayInfo = RelayInfo;

impl RelayFinderTrait for RelayFinder {
    fn list_all_relays(&self, my_id: &str, include_self: bool) -> Vec<RelayInfo> {
        debug_theme!(
            themes::RELAY,
            "[RelayFinder] list_all_relays: my_id={} include_self={}",
            my_id,
            include_self
        );

        let my_id_norm = my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);

        // 1) Ensure relay_updater is initialised and cache is populated/refreshed
        let updater = self.get_or_init_updater(my_id_norm);
        debug_theme!(
            themes::RELAY,
            "[RelayFinder] list_all_relays: relay cache populated, querying relay status"
        );

        // 2) Select a root relay, query its status and update the cache with all relay details
        updater.relay_select_and_query(&[]);
        // If the query wiped the cache (e.g. relay DDB has no entries), re-seed from blockchain
        if updater.relay_info_cache().is_empty() {
            updater.init_from_blockchain();
        }

        let mut relays: Vec<RelayInfo> =
            updater.relay_info_cache().list_all_relays(my_id_norm, true);
        debug_theme!(
            themes::RELAY,
            "[RelayFinder] list_all_relays: relays from cache: {:?}",
            relays
        );

        relays.sort_by(|a, b| a.id().cmp(b.id()));

        if !include_self {
            relays.retain(|r| r.id() != my_id_norm);
        }
        tracing::info!(
            "[RelayFinder] list_all_relays: returning relay list: {:?}",
            relays
        );
        relays
    }

    fn find_relay(&self, my_id: &str) -> Result<RelayInfo, String> {
        self.find_relay_excluding_internal(my_id, &[])
    }

    fn find_relay_excluding(
        &self,
        my_id: &str,
        exclude: &[SocketAddr],
    ) -> Result<RelayInfo, String> {
        self.find_relay_excluding_internal(my_id, exclude)
    }

    fn list_root_relays(&self, my_id: &str, include_self: bool) -> Vec<RelayInfo> {
        self.list_root_relays_internal(my_id, include_self)
    }

    fn clear_state_cache(&self) {
        if let Some(updater) = self.relay_updater.get() {
            updater.relay_info_cache().clear_state_cache();
        }
    }

    fn lookup_root_id(&self, id: &str) -> Option<NetworkEndpoint> {
        let id_norm = id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        // Ensure root list is populated (include_self=true to avoid filtering out the queried id)
        let _ = self.list_root_relays_internal(id_norm, true);
        self.relay_updater
            .get()
            .and_then(|u| u.relay_info_cache().lookup_root_id(id_norm))
    }

    fn remove_relay(&self, relay_id: &str) {
        if let Some(updater) = self.relay_updater.get() {
            updater.relay_info_cache().delete_relay(relay_id);
        }
    }
}

impl RelayFinderTestTrait for RelayFinder {
    fn select_indices(&self, relays: &[RelayInfo], my_id: &str) -> (usize, usize) {
        self.select_indices_internal(relays, my_id)
    }

    fn relays_available(&self) -> HashMap<RelayState, usize> {
        let mut out: HashMap<RelayState, usize> = HashMap::new();
        let mut accumulate = |v: &Vec<RelayInfo>| {
            for r in v {
                if let Some(st) = r.state {
                    *out.entry(st).or_insert(0) += 1;
                }
            }
        };
        if let Some(updater) = self.relay_updater.get() {
            let relays = updater.relay_info_cache().list_all_relays("", true);
            accumulate(&relays);
        }
        out
    }

    fn find_root_relay(&self, my_id: &str) -> Result<RelayInfo, String> {
        self.clear_unavailable_relays_internal();
        let updater = self.get_or_init_updater(my_id);
        updater.init_from_blockchain();
        Ok(updater
            .relay_select_and_query(&[])
            .expect("relay_select_and_query failed"))
    }
}

impl RelayFinder {
    pub fn new(
        api: crate::api::bingle_api::BingleApiBothType,
        discover_roots: Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync>,
    ) -> Self {
        Self {
            api,
            discover_roots,
            unavailable_relays: Mutex::new(HashSet::new()),
            relay_updater: OnceLock::new(),
        }
    }

    /// Lazy-initialise the `RelayUpdater` and ensure the `RelayInfoCache` is populated.
    /// Calls `init_from_blockchain` when the cache is empty, otherwise `update_when_expired`.
    fn get_or_init_updater(&self, my_id: &str) -> &RelayUpdater {
        let updater = self.relay_updater.get_or_init(|| {
            RelayUpdater::new_with_api(
                my_id.to_string(),
                self.api.clone(),
                self.discover_roots.clone(),
            )
        });
        if updater.relay_info_cache().is_empty() {
            updater.init_from_blockchain();
        } else {
            updater.update_when_expired();
        }
        updater
    }

    /// Find any relay suitable for us (root or non-root), excluding specific addresses.
    /// Uses RelayUpdater::relay_select_and_query with an address-based exclusion list.
    /// Does not use the single-relay cache if exclusions are provided.
    fn find_relay_excluding_internal(
        &self,
        my_id: &str,
        exclude: &[SocketAddr],
    ) -> Result<RelayInfo, String> {
        tracing::info!(
            "[RelayFinder] find_relay_excluding: my_id={} exclude={:?}",
            my_id,
            exclude
        );
        let my_id_norm = my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);

        // 1) (self.cache removed)

        // 2) Ensure the relay updater is initialised (populates the cache if empty)
        let updater = self.get_or_init_updater(my_id_norm);

        // 3) Build exclusion id list from the excluded socket addresses
        let exclude_ids: Vec<String> = if exclude.is_empty() {
            Vec::new()
        } else {
            updater
                .relay_info_cache()
                .list_all_relays(my_id_norm, true)
                .into_iter()
                .filter(|r| exclude.contains(&r.address()))
                .map(|r| r.id().to_string())
                .collect()
        };

        // 4) Delegate to relay_select_and_query with the exclusion list
        let relay = updater
            .relay_select_and_query(&exclude_ids)
            .ok_or_else(|| {
                "no available relay (all candidates failed or were excluded)".to_string()
            })?;

        tracing::info!(
            "[RelayFinder] find_relay_excluding: selected relay: {} {}",
            relay.id(),
            relay.address()
        );

        Ok(relay)
    }

    /// Return the list of root relays discovered via the blockchain (discover_roots), optionally including ourselves.
    /// Uses RelayUpdater: calls init_from_blockchain when cache is empty, otherwise update_when_expired.
    fn list_root_relays_internal(&self, my_id: &str, include_self: bool) -> Vec<RelayInfo> {
        tracing::info!(
            "[RelayFinder] list_root_relays: my_id={} include_self={}",
            my_id,
            include_self
        );
        let my_id_norm = my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);

        // 1) Lazy-initialise RelayUpdater and ensure cache is populated/refreshed
        let updater = self.get_or_init_updater(my_id_norm);

        // 2) Obtain relays from the updater's RelayInfoCache
        let mut relays = updater
            .relay_info_cache()
            .list_root_relays(my_id_norm, true);
        relays.sort_by(|a, b| a.id().cmp(b.id()));

        if !include_self {
            relays.retain(|r| r.id() != my_id_norm);
        }
        tracing::info!(
            "[RelayFinder] list_root_relays: returning root relays: {:?}",
            relays
        );
        relays
    }

    fn select_indices_internal(&self, relays: &[RelayInfo], my_id: &str) -> (usize, usize) {
        let n = relays.len();
        if n == 1 {
            return (0usize, 0usize);
        }
        let bucket = self.id_bucket_u32(my_id);
        let span = u64::from(u32::MAX) + 1; // 2^32
        let part_size = span / (n as u64);
        let idx = ((bucket as u64) / part_size) as usize % n;
        let alt = (idx + 1) % n;
        tracing::info!(
            "[RelayFinder] select_indices: n={} id={} idx={} alt={}",
            n,
            my_id,
            idx,
            alt
        );
        (idx, alt)
    }

    fn id_bucket_u32(&self, id: &str) -> u32 {
        // Attempt to decode Algorand address to bytes and take first 4 bytes
        if let Ok(bytes) = algo_ops::address_to_byte_key(id) {
            let b0 = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            return b0;
        }
        // Fallback: hash the string and take lower 32 bits
        use sha2::{Digest, Sha512_256};
        let mut h = Sha512_256::new();
        h.update(id.as_bytes());
        let digest: [u8; 32] = h.finalize().into();
        u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]])
    }

    fn clear_unavailable_relays_internal(&self) {
        if let Ok(mut g) = self.unavailable_relays.lock() {
            g.clear();
        }
    }
}
