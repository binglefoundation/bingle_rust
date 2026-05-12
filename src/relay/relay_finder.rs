use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;
use crate::themes;

use crate::api::bingle_api::NetworkEndpoint;
use data_encoding::BASE32_NOPAD;
use std::collections::HashMap;
use crate::engine::{BingleAccess, RelayState};

#[derive(Debug, Clone)]
pub struct RelayInfo {
    pub id: String,           // Algorand address of the relay
    pub address: SocketAddr,  // Relay IP:port
    pub state: Option<RelayState>, // Optional known state from RelayCheck
}

#[derive(Debug, Clone)]
struct CachedRelay {
    pub id: String,
    pub address: SocketAddr,
    pub expires_at: Instant,
}

#[derive(Debug, Clone)]
struct CachedRelayList {
    root_relays: Vec<RelayInfo>,
    pub expires_at: Instant,
}

#[derive(Debug, Clone)]
struct CachedAllRelayList {
    relays: Vec<RelayInfo>,
    pub expires_at: Instant,
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
    root_list_cache: Mutex<Option<CachedRelayList>>, // cached list of roots
    all_list_cache: Mutex<Option<CachedAllRelayList>>, // cached list of all relays
    // Injection point for discovery (keeps this component testable and avoids tying to indexer here)
    discover_roots: Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync>,
}

pub type RootRelayInfo = RelayInfo;

impl RelayFinder {
    pub fn new(
        api: crate::api::bingle_api::BingleApiBothType,
        cache_ttl: Duration,
        discover_roots: Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync>,
    ) -> Self {
        Self { api, cache_ttl, cache: Mutex::new(None), root_list_cache: Mutex::new(None), all_list_cache: Mutex::new(None), discover_roots }
    }

    /// Return the list of all relays (root and non-root) using the DDB client get_relays.
    /// Requires that the root relay cache has been populated (list_root_relays called recently).
    pub fn list_all_relays(&self, my_id: &str, include_self: bool) -> Vec<RelayInfo> {
        info_theme!(themes::RELAY, "[RelayFinder] list_all_relays: my_id={} include_self={}", my_id, include_self);

        let my_id_norm = my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        // 0) Return cached all-relays list if valid
        // if let Ok(guard) = self.all_list_cache.lock() {
        //     if let Some(list) = &*guard {
        //         if Instant::now() < list.expires_at {
        //             let mut result = list.relays.clone();
        //             if !include_self { result.retain(|r| r.id != my_id_norm); }
        //             result.sort_by(|a, b| a.id.cmp(&b.id));
        //             tracing::info!("[RelayFinder] list_all_relays: returning cached relay list: {:?}", result);
        //             return result;
        //         }
        //     }
        // }
        // 1) Ensure root relays are cached; list_root_relays performs discovery and caches
        info_theme!(themes::RELAY, "[RelayFinder] list_all_relays: ensuring root relays are cached, call list_root_relays");
        let _ = self.list_root_relays(my_id, true);
        info_theme!(themes::RELAY, "[RelayFinder] list_all_relays: root relays are cached, access DDB from root");

        // 2) Pick preferred root relay and fetch all relays via DDB from that single root only
        let cli = crate::ddb::DdbClientImpl::with_discovery(self.api.clone(), self.discover_roots.clone());
        let mut relays: Vec<RelayInfo> = {
            // Access cached root list
            let roots_opt = self.root_list_cache.lock().ok().and_then(|g| g.clone());
            if let Some(roots) = roots_opt {
                // Exclude self from the roots list before selecting which root to query via DDB
                let candidates: Vec<RelayInfo> = roots
                    .root_relays
                    .iter()
                    .cloned()
                    .filter(|r| r.id != my_id_norm)
                    .collect();

                    if !candidates.is_empty() {
                        let (pref_idx, _alt_idx) = self.select_indices(&candidates, my_id_norm);
                        let chosen = &candidates[pref_idx];
                        match cli.get_relays_from(chosen) {
                            Ok(pairs) => pairs
                                .into_iter()
                                .map(|(id, addr)| RelayInfo { id, address: addr, state: None })
                                .collect(),
                            Err(_e) => Vec::new(),
                        }
                    } else {
                        Vec::new()
                    }
            } else {
                Vec::new()
            }
        };
        info_theme!(themes::RELAY, "[RelayFinder] list_all_relays: relays from DDB: {:?}", relays);

        // 3) Fallback: if none from DDB, use the cached root list
        if relays.is_empty() {
            if let Ok(g) = self.root_list_cache.lock() {
                if let Some(roots) = &*g {
                    warn_theme!(themes::RELAY, "[RelayFinder] list_all_relays: no relays from DDB, using cached root list: {:?}", roots);
                    relays = roots.root_relays.clone();
                }
            }
        }
        // 4) Cache all-relays list. We ensure our own relay is included in the cached list if it was excluded by DDB.
        {
            let has_self = relays.iter().any(|r| r.id == my_id_norm);
            if !has_self {
                let mut added = false;
                if let Some(api) = self.api.upgrade() {
                    if api.is_relay() {
                        if let Some(addr) = api.get_last_public_addr() {
                            info_theme!(themes::RELAY, "[RelayFinder] list_all_relays: adding self (from api) to relay list for cache: {}", my_id_norm);
                            relays.push(RelayInfo { id: my_id_norm.to_string(), address: addr, state: None });
                            added = true;
                        }
                    }
                }
                if !added {
                    if let Ok(g) = self.root_list_cache.lock() {
                        if let Some(roots) = &*g {
                            if let Some(me) = roots.root_relays.iter().find(|r| r.id == my_id_norm) {
                                tracing::info!("[RelayFinder] list_all_relays: adding self (from roots cache) to relay list for cache: {}", me.id);
                                relays.push(RelayInfo { id: me.id.clone(), address: me.address, state: None });
                            }
                        }
                    }
                }
            }
        }
        relays.sort_by(|a, b| a.id.cmp(&b.id));
        let expires = Instant::now() + self.cache_ttl;
        if let Ok(mut g) = self.all_list_cache.lock() { *g = Some(CachedAllRelayList { relays: relays.clone(), expires_at: expires }); }

        if !include_self { relays.retain(|r| r.id != my_id_norm); }
        tracing::info!("[RelayFinder] list_all_relays: returning relay list: {:?}", relays);
        relays
    }

    // Deprecated convenience constructor using AlgoBingle discovery has been removed.

    /// Find any relay suitable for us (root or non-root). Uses DDB getEpoch via list_all_relays.
    pub fn find_relay(&self, my_id: &str) -> Result<RelayInfo, String> {
        self.find_relay_excluding(my_id, &[])
    }

    /// Find any relay suitable for us (root or non-root), excluding specific addresses.
    /// Does not use the single-relay cache if exclusions are provided.
    pub fn find_relay_excluding(&self, my_id: &str, exclude: &[SocketAddr]) -> Result<RelayInfo, String> {
        tracing::info!("[RelayFinder] find_relay_excluding: my_id={} exclude={:?}", my_id, exclude);
        let my_id_norm = my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);

        // 1) If no exclusions, try returning cached if valid AND not ourselves
        if exclude.is_empty() {
            if let Some(c) = self.cache.lock().map_err(|e| format!("cache lock poisoned: {}", e))?.as_ref() {
                if Instant::now() < c.expires_at && c.id != my_id_norm {
                    tracing::info!("find_relay: using cached relay: {} {}", c.id, c.address);
                    return Ok(RelayInfo { id: c.id.clone(), address: c.address, state: None });
                }
            }
        }

        // 2) Get all relays (requires root cache populated; list_all_relays ensures this)
        let mut relays = self.list_all_relays(my_id, false);
        if !exclude.is_empty() {
            relays.retain(|r| !exclude.contains(&r.address));
        }
        if relays.is_empty() { return Err("no relays discovered (after excluding self and specified endpoints)".to_string()); }
        tracing::info!("[RelayFinder] find_relay: candidates = {:?}", relays);

        // 3) Choose preferred and alternate per partitioning rule
        let (pref_idx, alt_idx) = self.select_indices(&relays, my_id_norm);

        // 4) Try preferred then alternate via RelayCheck
        for &idx in &[pref_idx, alt_idx] {
            let cand = &relays[idx];
            if self.relay_check(my_id, &*cand.id, cand.address) {
                tracing::info!("[RelayFinder] find_relay: check passed and using relay {}: {} {}", idx, cand.id, cand.address);
                let info = RelayInfo { id: cand.id.clone(), address: cand.address, state: None };
                // Cache selection only if no exclusions were specified
                if exclude.is_empty() {
                    let expires = Instant::now() + self.cache_ttl;
                    *self.cache.lock().map_err(|e| format!("cache lock poisoned: {}", e))? = Some(CachedRelay { id: info.id.clone(), address: info.address, expires_at: expires });
                }
                return Ok(info);
            } else {
                tracing::info!("[RelayFinder] find_relay: relay {} failed RelayCheck: {} {}", idx, cand.id, cand.address);
            }
        }
        Err("no available relay (preferred and alternate failed RelayCheck)".to_string())
    }

    /// Return the list of root relays discovered via the blockchain (discover_roots), optionally including ourselves.
    pub fn list_root_relays(&self, my_id: &str, include_self: bool) -> Vec<RelayInfo> {
        tracing::info!("[RelayFinder] list_root_relays: my_id={} include_self={}", my_id, include_self);
        let my_id_norm = my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        // 1) Use cached list if valid
        if let Ok(guard) = self.root_list_cache.lock() {
            if let Some(list) = &*guard {
                if Instant::now() < list.expires_at {
                    let mut result = list.root_relays.clone();
                    if !include_self {
                        result.retain(|r| r.id != my_id_norm);
                    }
                    tracing::info!("[RelayFinder] list_root_relays: using cached root relays: {:?}", result);
                    return result;
                }
            }
        }

        // 2) Discover roots via provided closure
        tracing::info!("[RelayFinder] list_root_relays: discovering roots");
        let mut relays = (self.discover_roots)();
        tracing::info!("[RelayFinder] list_root_relays: discover_roots found {} root relays", relays.len());
        relays.sort_by(|a, b| a.id.cmp(&b.id));

        // 3) Cache FULL list of root relays (including ourselves if present in discovery)
        let expires = Instant::now() + self.cache_ttl;
        if let Ok(mut guard) = self.root_list_cache.lock() {
            *guard = Some(CachedRelayList { root_relays: relays.clone(), expires_at: expires });
        }

        if !include_self {
            relays.retain(|r| r.id != my_id_norm);
        }
        tracing::info!("[RelayFinder] list_root_relays: returning root relays: {:?}", relays);
        relays
    }

    /// Find the preferred root relay for the provided id, performing RelayCheck and caching the result.
    pub fn find_root_relay(&self, my_id: &str) -> Result<RelayInfo, String> {
        tracing::info!("[RelayFinder] find_root_relay: my_id={}", my_id);
        // Normalize our id to raw address
        let my_id_norm = my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);

        // 1) Return cached if valid AND not ourselves
        if let Some(c) = self.cache.lock().map_err(|e| format!("cache lock poisoned: {}", e))?.as_ref() {
            if Instant::now() < c.expires_at && c.id != my_id_norm {
                tracing::info!("find_root_relay: using cached root relay: {} {}", c.id, c.address);
                return Ok(RelayInfo { id: c.id.clone(), address: c.address, state: None });
            }
        }

        // 2) Get candidate list (cached when possible)
        let relays = self.list_root_relays(my_id, false);
        if relays.is_empty() {
            return Err("no root relays discovered (after excluding self)".to_string());
        }
        tracing::info!("[RelayFinder] find_root_relay: candidates = {:?}", relays);

        // 3) Choose preferred and alternate per partitioning rule
        let (pref_idx, alt_idx) = self.select_indices(&relays, my_id_norm);

        // 4) Try preferred then alternate via RelayCheck
        for &idx in &[pref_idx, alt_idx] {
            let cand = &relays[idx];
            if self.relay_check(my_id, &*cand.id, cand.address) {
                tracing::info!("[RelayFinder] find_root_relay: check passed and using relay {}: {} {}", idx, cand.id, cand.address);
                let info = RelayInfo { id: cand.id.clone(), address: cand.address, state: None };
                // Cache single selection
                let expires = Instant::now() + self.cache_ttl;
                *self.cache.lock().map_err(|e| format!("cache lock poisoned: {}", e))? = Some(CachedRelay { id: info.id.clone(), address: info.address, expires_at: expires });
                return Ok(info);
            } else {
                tracing::info!("[RelayFinder] find_root_relay: relay {} failed RelayCheck: {} {}", idx, cand.id, cand.address);
            }
        }

        Err("no available root relay (preferred and alternate failed RelayCheck)".to_string())
    }

    pub fn select_indices(&self, relays: &[RelayInfo], my_id: &str) -> (usize, usize) {
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
            "off" => RelayState::Off,
            "starting" => RelayState::Starting,
            "available" => RelayState::Available,
            "own" => RelayState::Own,
            _ => RelayState::Off,
        }
    }

    fn update_cached_state(&self, id: &str, st: RelayState) {
        tracing::info!("[RelayFinder] update_cached_state: relay={} state={:?}", id, st);
        if let Ok(mut g) = self.root_list_cache.lock() {
            if let Some(list) = &mut *g {
                for r in &mut list.root_relays {
                    if r.id == id { r.state = Some(st); }
                }
            }
        }
        else {
            tracing::error!("[RelayFinder] update_cached_state: root_list_cache lock poisoned");
        }
        if let Ok(mut g) = self.all_list_cache.lock() {
            if let Some(list) = &mut *g {
                for r in &mut list.relays {
                    if r.id == id { r.state = Some(st); }
                }
            }
        }
        else {
            tracing::error!("[RelayFinder] update_cached_state: all_list_cache lock poisoned");
        }
    }

    pub fn load_relay_states(&self, my_id: &str) {
        // Ensure we have a list of all relays cached (includes self)
        let relays = self.list_all_relays(my_id, true);
        for r in relays {
            let _ = self.relay_check(my_id, &r.id, r.address);
        }
    }

    pub fn relays_available(&self) -> HashMap<RelayState, usize> {
        let mut out: HashMap<RelayState, usize> = HashMap::new();
        let mut accumulate = |v: &Vec<RelayInfo>| {
            for r in v {
                if let Some(st) = r.state {
                    *out.entry(st).or_insert(0) += 1;
                }
            }
        };
        if let Ok(g) = self.all_list_cache.lock() {
            if let Some(list) = &*g { accumulate(&list.relays); return out; }
        }
        if let Ok(g) = self.root_list_cache.lock() {
            if let Some(list) = &*g { accumulate(&list.root_relays); }
        }
        out
    }

    fn relay_check(&self, my_id: &str, id: &str, addr: SocketAddr) -> bool {
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
                self.update_cached_state(id, RelayState::Off);
                false
            },
        }
    }

    /// Clear cached relay selection and expire lists; reset all cached states to None.
    /// After calling this, subsequent list/load calls will refresh via discovery and checks.
    pub fn clear_state_cache(&self) {
        // Clear single-relay cache
        if let Ok(mut g) = self.cache.lock() { *g = None; }
        // Expire root list and reset states
        if let Ok(mut g) = self.root_list_cache.lock() {
            if let Some(list) = &mut *g {
                for r in &mut list.root_relays { r.state = None; }
                // Expire immediately to force reload on next access
                list.expires_at = Instant::now() - Duration::from_secs(1);
            }
        }
        // Expire all-relays list and reset states
        if let Ok(mut g) = self.all_list_cache.lock() {
            if let Some(list) = &mut *g {
                for r in &mut list.relays { r.state = None; }
                list.expires_at = Instant::now() - Duration::from_secs(1);
            }
        }
    }

    /// Lookup a root relay id and return its direct NetworkEndpoint if known.
    /// Uses list_root_relays to ensure the root list cache is populated, then searches the cache.
    pub fn lookup_root_id(&self, id: &str) -> Option<NetworkEndpoint> {
        let id_norm = id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        // Ensure root list is populated/cached (include_self=true to avoid filtering out the queried id)
        let _ = self.list_root_relays(id_norm, true);
        if let Ok(g) = self.root_list_cache.lock() {
            if let Some(list) = &*g {
                if let Some(r) = list.root_relays.iter().find(|r| r.id == id_norm) {
                    return Some(NetworkEndpoint::new_direct(r.address));
                }
            }
        }
        None
    }
}
