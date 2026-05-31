use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::json;
use crate::themes;

use crate::api::bingle_api::NetworkEndpoint;
use data_encoding::BASE32_NOPAD;
use std::collections::{HashMap, HashSet};
use crate::engine::{BingleAccess, RelayState};

pub use crate::relay::relay_info_cache::RelayInfoCache;
use crate::relay::relay_updater::RelayUpdater;

#[derive(Debug, Clone)]
pub struct RelayInfo {
    pub id: String,           // Algorand address of the relay
    pub address: SocketAddr,  // Relay IP:port
    pub is_root: bool,
    pub state: Option<RelayState>, // Optional known state from RelayCheck
    pub ttl: Option<u64>,
    pub last_updated: Instant, // Time this entry was last created or updated
}

impl RelayInfo {
    pub fn root(id: impl Into<String>, address: SocketAddr) -> Self {
        Self::root_with(id, address, None, None)
    }

    pub fn root_with(
        id: impl Into<String>,
        address: SocketAddr,
        state: Option<RelayState>,
        ttl: Option<u64>,
    ) -> Self {
        Self {
            id: id.into(),
            address,
            is_root: true,
            state,
            ttl,
            last_updated: Instant::now(),
        }
    }

    pub fn non_root(id: impl Into<String>, address: SocketAddr) -> Self {
        Self::non_root_with(id, address, None, None)
    }

    pub fn non_root_with(
        id: impl Into<String>,
        address: SocketAddr,
        state: Option<RelayState>,
        ttl: Option<u64>,
    ) -> Self {
        Self {
            id: id.into(),
            address,
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

#[derive(Debug, Clone)]
struct CachedRelay {
    pub id: String,
    pub address: SocketAddr,
    pub ttl: Option<u64>,
    pub expires_at: Instant,
}


pub trait RelayFinderTrait: Send + Sync {
    fn list_all_relays(&self, my_id: &str, include_self: bool) -> Vec<RelayInfo>;
    fn find_relay(&self, my_id: &str) -> Result<RelayInfo, String>;
    fn find_relay_excluding(&self, my_id: &str, exclude: &[SocketAddr]) -> Result<RelayInfo, String>;
    fn list_root_relays(&self, my_id: &str, include_self: bool) -> Vec<RelayInfo>;
    fn load_relay_states(&self, my_id: &str);
    fn clear_state_cache(&self);
    fn lookup_root_id(&self, id: &str) -> Option<NetworkEndpoint>;
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
    cache_ttl: Duration,
    cache: Mutex<Option<CachedRelay>>, // single selected relay cache
    // Injection point for discovery (keeps this component testable and avoids tying to indexer here)
    discover_roots: Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync>,
    unavailable_relays: Mutex<HashSet<String>>,
    relay_updater: OnceLock<RelayUpdater>,
}

pub type RootRelayInfo = RelayInfo;

impl RelayFinderTrait for RelayFinder {
    fn list_all_relays(&self, my_id: &str, include_self: bool) -> Vec<RelayInfo> {
        self.clear_unavailable_relays_internal();
        self.list_all_relays_internal(my_id, include_self)
    }

    fn find_relay(&self, my_id: &str) -> Result<RelayInfo, String> {
        self.clear_unavailable_relays_internal();
        self.find_relay_internal(my_id)
    }

    fn find_relay_excluding(&self, my_id: &str, exclude: &[SocketAddr]) -> Result<RelayInfo, String> {
        self.clear_unavailable_relays_internal();
        self.find_relay_excluding_internal(my_id, exclude)
    }

    fn list_root_relays(&self, my_id: &str, include_self: bool) -> Vec<RelayInfo> {
        self.clear_unavailable_relays_internal();
        self.list_root_relays_internal(my_id, include_self)
    }

    fn load_relay_states(&self, my_id: &str) {
        self.clear_unavailable_relays_internal();
        self.load_relay_states_internal(my_id)
    }

    fn clear_state_cache(&self) {
        self.clear_unavailable_relays_internal();
        self.clear_state_cache_internal()
    }

    fn lookup_root_id(&self, id: &str) -> Option<NetworkEndpoint> {
        self.clear_unavailable_relays_internal();
        self.lookup_root_id_internal(id)
    }
}

impl RelayFinderTestTrait for RelayFinder {
    fn select_indices(&self, relays: &[RelayInfo], my_id: &str) -> (usize, usize) {
        self.select_indices_internal(relays, my_id)
    }

    fn relays_available(&self) -> HashMap<RelayState, usize> {
        self.relays_available_internal()
    }

    fn find_root_relay(&self, my_id: &str) -> Result<RelayInfo, String> {
        self.clear_unavailable_relays_internal();
        self.find_root_relay_internal(my_id)
    }
}

impl RelayFinder {
    pub fn new(
        api: crate::api::bingle_api::BingleApiBothType,
        cache_ttl: Duration,
        discover_roots: Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync>,
    ) -> Self {
        Self {
            api,
            cache_ttl,
            cache: Mutex::new(None),
            discover_roots,
            unavailable_relays: Mutex::new(HashSet::new()),
            relay_updater: OnceLock::new(),
        }
    }

    /// Return the list of all relays (root and non-root) using the DDB client get_relays.
    /// Requires that the root relay cache has been populated (list_root_relays called recently).
    fn list_all_relays_internal(&self, my_id: &str, include_self: bool) -> Vec<RelayInfo> {
        debug_theme!(themes::RELAY, "[RelayFinder] list_all_relays: my_id={} include_self={}", my_id, include_self);

        let my_id_norm = my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);

        // 1) Ensure relay_updater is initialised and cache is populated/refreshed
        let updater = self.relay_updater.get_or_init(|| {
            RelayUpdater::new_with_api(my_id_norm.to_string(), self.api.clone(), self.discover_roots.clone())
        });
        if updater.relay_info_cache().is_empty() {
            updater.init_from_blockchain();
        } else {
            updater.update_when_expired();
        }
        debug_theme!(themes::RELAY, "[RelayFinder] list_all_relays: relay cache populated, querying relay status");

        // 2) Select a root relay, query its status and update the cache with all relay details
        updater.relay_select_and_query();
        // If the query wiped the cache (e.g. relay DDB has no entries), re-seed from blockchain
        if updater.relay_info_cache().is_empty() {
            updater.init_from_blockchain();
        }
        // Mark any relays that are now Off as unavailable so relay_check skips them
        {
            let all = updater.relay_info_cache().list_all_relays(my_id_norm, true);
            if let Ok(mut unavailable) = self.unavailable_relays.lock() {
                for relay in all.iter().filter(|r| r.state == Some(crate::engine::RelayState::Off)) {
                    unavailable.insert(relay.id.clone());
                }
            }
        }
        let mut relays: Vec<RelayInfo> = updater.relay_info_cache().list_all_relays(my_id_norm, true);
        debug_theme!(themes::RELAY, "[RelayFinder] list_all_relays: relays from cache: {:?}", relays);

        // 3) Ensure our own relay is included in the list if it was not returned by the query.
        {
            let has_self = relays.iter().any(|r| r.id == my_id_norm);
            if !has_self {
                let mut added = false;
                if let Some(api) = self.api.upgrade() {
                    if api.is_relay() {
                        if let Some(addr) = api.get_last_public_addr() {
                            debug_theme!(themes::RELAY, "[RelayFinder] list_all_relays: adding self (from api) to relay list for cache: {}", my_id_norm);
                            relays.push(RelayInfo::root(my_id_norm.to_string(), addr));
                            added = true;
                        }
                    }
                }
                if !added {
                    let roots = updater.relay_info_cache().list_root_relays(my_id_norm, true);
                    if let Some(me) = roots.iter().find(|r| r.id == my_id_norm) {
                        tracing::info!("[RelayFinder] list_all_relays: adding self (from relay_info_cache) to relay list for cache: {}", me.id);
                        relays.push(if me.is_root {
                            RelayInfo::root_with(me.id.clone(), me.address, None, me.ttl)
                        } else {
                            RelayInfo::non_root_with(me.id.clone(), me.address, None, me.ttl)
                        });
                    }
                }
            }
        }
        relays.sort_by(|a, b| a.id.cmp(&b.id));

        if !include_self { relays.retain(|r| r.id != my_id_norm); }
        tracing::info!("[RelayFinder] list_all_relays: returning relay list: {:?}", relays);
        relays
    }

    // Deprecated convenience constructor using AlgoBingle discovery has been removed.

    /// Find any relay suitable for us (root or non-root). Uses DDB getRelaysStatus via list_all_relays.
    fn find_relay_internal(&self, my_id: &str) -> Result<RelayInfo, String> {
        self.find_relay_excluding_internal(my_id, &[])
    }

    /// Find any relay suitable for us (root or non-root), excluding specific addresses.
    /// Does not use the single-relay cache if exclusions are provided.
    fn find_relay_excluding_internal(&self, my_id: &str, exclude: &[SocketAddr]) -> Result<RelayInfo, String> {
        tracing::info!("[RelayFinder] find_relay_excluding: my_id={} exclude={:?}", my_id, exclude);
        let my_id_norm = my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);

        // 1) If no exclusions, try returning cached if valid AND not ourselves
        if exclude.is_empty() {
            if let Some(c) = self.cache.lock().map_err(|e| format!("cache lock poisoned: {}", e))?.as_ref() {
                if Instant::now() < c.expires_at && c.id != my_id_norm {
                    tracing::info!("find_relay: using cached relay: {} {}", c.id, c.address);
                    return Ok(RelayInfo::root_with(c.id.clone(), c.address, None, c.ttl));
                }
            }
        }

        // 2) Get all relays (requires root cache populated; list_all_relays ensures this)
        let mut relays = self.list_all_relays_internal(my_id, false);
        if !exclude.is_empty() {
            relays.retain(|r| !exclude.contains(&r.address));
        }
        if relays.is_empty() { return Err("no relays discovered (after excluding self and specified endpoints)".to_string()); }
        tracing::info!("[RelayFinder] find_relay: candidates = {:?}", relays);

        // 3) Choose preferred and alternate per partitioning rule
        let (pref_idx, alt_idx) = self.select_indices_internal(&relays, my_id_norm);

        // 4) Try preferred then alternate via RelayCheck
        for &idx in &[pref_idx, alt_idx] {
            let cand = &relays[idx];
            if self.relay_check(my_id, &*cand.id, cand.address) {
                tracing::info!("[RelayFinder] find_relay: check passed and using relay {}: {} {}", idx, cand.id, cand.address);
                let info = if cand.is_root {
                    RelayInfo::root_with(cand.id.clone(), cand.address, None, cand.ttl)
                } else {
                    RelayInfo::non_root_with(cand.id.clone(), cand.address, None, cand.ttl)
                };
                // Cache selection only if no exclusions were specified
                if exclude.is_empty() {
                    let expires = Instant::now() + self.cache_ttl;
                    *self.cache.lock().map_err(|e| format!("cache lock poisoned: {}", e))? = Some(CachedRelay { id: info.id.clone(), address: info.address, ttl: info.ttl, expires_at: expires });
                }
                return Ok(info);
            } else {
                tracing::info!("[RelayFinder] find_relay: relay {} failed RelayCheck: {} {}", idx, cand.id, cand.address);
            }
        }
        Err("no available relay (preferred and alternate failed RelayCheck)".to_string())
    }

    /// Return the list of root relays discovered via the blockchain (discover_roots), optionally including ourselves.
    /// Uses RelayUpdater: calls init_from_blockchain when cache is empty, otherwise update_when_expired.
    fn list_root_relays_internal(&self, my_id: &str, include_self: bool) -> Vec<RelayInfo> {
        tracing::info!("[RelayFinder] list_root_relays: my_id={} include_self={}", my_id, include_self);
        let my_id_norm = my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);

        // 1) Lazy-initialise RelayUpdater on first call
        let updater = self.relay_updater.get_or_init(|| {
            RelayUpdater::new_with_api(my_id_norm.to_string(), self.api.clone(), self.discover_roots.clone())
        });

        // 2) If the cache is empty seed it from the blockchain; otherwise refresh any expired entries
        if updater.relay_info_cache().is_empty() {
            updater.init_from_blockchain();
        } else {
            updater.update_when_expired();
        }

        // 3) Obtain relays from the updater's RelayInfoCache
        let mut relays = updater.relay_info_cache().list_root_relays(my_id_norm, true);
        relays.sort_by(|a, b| a.id.cmp(&b.id));

        if !include_self {
            relays.retain(|r| r.id != my_id_norm);
        }
        tracing::info!("[RelayFinder] list_root_relays: returning root relays: {:?}", relays);
        relays
    }

    /// Find the preferred root relay for the provided id, performing RelayCheck and caching the result.
    #[allow(dead_code)]
    fn find_root_relay_internal(&self, my_id: &str) -> Result<RelayInfo, String> {
        tracing::info!("[RelayFinder] find_root_relay: my_id={}", my_id);
        // Normalize our id to raw address
        let my_id_norm = my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);

        // 1) Return cached if valid AND not ourselves
        if let Some(c) = self.cache.lock().map_err(|e| format!("cache lock poisoned: {}", e))?.as_ref() {
            if Instant::now() < c.expires_at && c.id != my_id_norm {
                tracing::info!("find_root_relay: using cached root relay: {} {}", c.id, c.address);
                return Ok(RelayInfo::root_with(c.id.clone(), c.address, None, c.ttl));
            }
        }

        // 2) Get candidate list (cached when possible)
        let relays = self.list_root_relays_internal(my_id, false);
        if relays.is_empty() {
            return Err("no root relays discovered (after excluding self)".to_string());
        }
        tracing::info!("[RelayFinder] find_root_relay: candidates = {:?}", relays);

        // 3) Choose preferred and alternate per partitioning rule
        let (pref_idx, alt_idx) = self.select_indices_internal(&relays, my_id_norm);

        // 4) Try preferred then alternate via RelayCheck
        for &idx in &[pref_idx, alt_idx] {
            let cand = &relays[idx];
            if self.relay_check(my_id, &*cand.id, cand.address) {
                tracing::info!("[RelayFinder] find_root_relay: check passed and using relay {}: {} {}", idx, cand.id, cand.address);
                let info = if cand.is_root {
                    RelayInfo::root_with(cand.id.clone(), cand.address, None, cand.ttl)
                } else {
                    RelayInfo::non_root_with(cand.id.clone(), cand.address, None, cand.ttl)
                };
                // Cache single selection
                let expires = Instant::now() + self.cache_ttl;
                *self.cache.lock().map_err(|e| format!("cache lock poisoned: {}", e))? = Some(CachedRelay { id: info.id.clone(), address: info.address, ttl: info.ttl, expires_at: expires });
                return Ok(info);
            } else {
                tracing::info!("[RelayFinder] find_root_relay: relay {} failed RelayCheck: {} {}", idx, cand.id, cand.address);
            }
        }

        Err("no available root relay (preferred and alternate failed RelayCheck)".to_string())
    }

    fn select_indices_internal(&self, relays: &[RelayInfo], my_id: &str) -> (usize, usize) {
        let n = relays.len();
        if n == 1 { return (0usize, 0usize); }
        let bucket = self.id_bucket_u32(my_id);
        let span = u64::from(u32::MAX) + 1; // 2^32
        let part_size = span / (n as u64);
        let idx = ((bucket as u64) / part_size) as usize % n;
        let alt = (idx + 1) % n;
        tracing::info!("[RelayFinder] select_indices: n={} id={} idx={} alt={}", n, my_id, idx, alt);
        (idx, alt)
    }

    fn id_bucket_u32(&self, id: &str) -> u32 {
        // Attempt to decode Algorand address to bytes and take first 4 bytes
        if let Ok(bytes) = crate::blockchain::algo_ops::address_to_byte_key(id) {
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

    fn parse_state_str(&self, s: &str) -> RelayState {
        match s.to_ascii_lowercase().as_str() {
            "unknown" => RelayState::Unknown,
            "off" => RelayState::Off,
            "starting" => RelayState::Starting,
            "available" => RelayState::Available,
            "own" => RelayState::Own,
            _ => RelayState::Unknown,
        }
    }

    fn update_cached_state(&self, id: &str, st: RelayState) {
        tracing::info!("[RelayFinder] update_cached_state: relay={} state={:?}", id, st);
        if let Some(updater) = self.relay_updater.get() {
            let cache = updater.relay_info_cache();
            let relays = cache.list_all_relays(id, true);
            if let Some(r) = relays.iter().find(|r| r.id == id) {
                let updated = if r.is_root {
                    RelayInfo::root_with(r.id.clone(), r.address, Some(st), r.ttl)
                } else {
                    RelayInfo::non_root_with(r.id.clone(), r.address, Some(st), r.ttl)
                };
                cache.update_relay(updated);
            }
        }
    }

    pub fn load_relay_states_internal(&self, my_id: &str) {
        // Ensure we have a list of all relays cached (includes self)
        let relays = self.list_all_relays_internal(my_id, true);
        for r in relays {
            let _ = self.relay_check(my_id, &r.id, r.address);
        }
    }

    #[allow(dead_code)]
    fn relays_available_internal(&self) -> HashMap<RelayState, usize> {
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

    fn relay_check(&self, my_id: &str, id: &str, addr: SocketAddr) -> bool {
        if self.is_unavailable(id) {
            self.update_cached_state(id, RelayState::Off);
            return false;
        }
        // If this is our own relay id, do not perform a network check; mark as Own and return false
        let my_id_norm = my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        if id == my_id_norm {
            self.update_cached_state(id, RelayState::Own);
            return false;
        }
        // Validate Algorand base32 address decodes to 36 bytes
        match BASE32_NOPAD.decode(id.as_bytes()) {
            Ok(bytes) if bytes.len() == 36 => {}
            Ok(bytes) => { tracing::warn!("[RelayFinder][relay_check] invalid relay id '{}': base32 decoded length {} != 36", id, bytes.len()); return false; }
            Err(e) => { tracing::warn!("[RelayFinder][relay_check] invalid relay id '{}': {}", id, e); return false; }
        }
        let nsk = NetworkEndpoint::new_direct(addr);
        let req = json!({ "app": null, "type": "Check" });
        tracing::info!("[RelayFinder] relay_check: sending request via API -> nsk={} user_id={} req={}", nsk, id, req);
        match self.api.access(|a| a.send_message_to_network_with_response(&nsk, &id.to_string(), req, None)) {
            Ok(resp) => {
                if resp.get("type").and_then(|v| v.as_str()) == Some("CheckResponse") {
                    if let Some(st_str) = resp.get("state").and_then(|v| v.as_str()) {
                        let st = self.parse_state_str(st_str);
                        self.update_cached_state(id, st);
                        return st == RelayState::Available;
                    }
                }
                false
            }
            Err(_) => {
                if let Ok(mut g) = self.unavailable_relays.lock() {
                    g.insert(id.to_string());
                }
                self.update_cached_state(id, RelayState::Off);
                false
            },
        }
    }

    /// Clear cached relay selection and expire lists; reset all cached states to None.
    /// After calling this, subsequent list/load calls will refresh via discovery and checks.
    fn clear_state_cache_internal(&self) {
        // Clear single-relay cache
        if let Ok(mut g) = self.cache.lock() { *g = None; }
        // Clear states in relay_info_cache
        if let Some(updater) = self.relay_updater.get() {
            updater.relay_info_cache().clear_state_cache();
        }
    }

    /// Lookup a root relay id and return its direct NetworkEndpoint if known.
    /// Ensures root list is populated via list_root_relays, then searches relay_info_cache.
    fn lookup_root_id_internal(&self, id: &str) -> Option<NetworkEndpoint> {
        let id_norm = id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        // Ensure root list is populated (include_self=true to avoid filtering out the queried id)
        let _ = self.list_root_relays_internal(id_norm, true);
        self.relay_updater.get()
            .and_then(|u| u.relay_info_cache().lookup_root_id(id_norm))
    }

    fn clear_unavailable_relays_internal(&self) {
        if let Ok(mut g) = self.unavailable_relays.lock() {
            g.clear();
        }
    }

    fn is_unavailable(&self, id: &str) -> bool {
        if let Ok(g) = self.unavailable_relays.lock() {
            return g.contains(id);
        }
        false
    }
}
