use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;

use crate::api::bingle_api::{BingleApi, NetworkSourceKey};

#[derive(Debug, Clone)]
pub struct RootRelayInfo {
    pub id: String,           // Algorand address of the relay
    pub address: SocketAddr,  // Relay IP:port
}

#[derive(Debug, Clone)]
struct CachedRelay {
    pub id: String,
    pub address: SocketAddr,
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
    api: Arc<dyn BingleApi>,
    cache_ttl: Duration,
    cache: Mutex<Option<CachedRelay>>,
    // Injection point for discovery (keeps this component testable and avoids tying to indexer here)
    discover_roots: Arc<dyn Fn() -> Vec<RootRelayInfo> + Send + Sync>,
}

impl RelayFinder {
    pub fn new(
        api: Arc<dyn BingleApi>,
        cache_ttl: Duration,
        discover_roots: Arc<dyn Fn() -> Vec<RootRelayInfo> + Send + Sync>,
    ) -> Self {
        Self { api, cache_ttl, cache: Mutex::new(None), discover_roots }
    }

    /// Convenience constructor: wire discovery to AlgoBingle::discover_root_relays using provided ops, app_id and candidate accounts.
    #[cfg(not(target_os = "ios"))]
    pub fn with_algo_discovery(
        api: Arc<dyn BingleApi>,
        cache_ttl: Duration,
        ops: crate::blockchain::algo_ops::AlgoOps,
        app_id: u64,
        accounts: Vec<String>,
    ) -> Self {
        let ab = crate::blockchain::algo_bingle::AlgoBingle::new(ops);
        let discover = Arc::new(move || {
            match ab.discover_root_relays(app_id, &accounts) {
                Ok(pairs) => pairs.into_iter().map(|(id, address)| RootRelayInfo { id, address }).collect(),
                Err(_) => Vec::new(),
            }
        });
        Self { api, cache_ttl, cache: Mutex::new(None), discover_roots: discover }
    }

    /// Find any relay suitable for us. Currently identical to finding the preferred root relay.
    pub fn find_relay(&self, my_id: &str) -> Result<SocketAddr, String> {
        self.find_root_relay(my_id).map(|info| info.address)
    }

    /// Find the preferred root relay for the provided id, performing RelayCheck and caching the result.
    pub fn find_root_relay(&self, my_id: &str) -> Result<RootRelayInfo, String> {
        // 1) Return cached if valid
        if let Some(c) = self.cache.lock().map_err(|e| format!("cache lock poisoned: {}", e))?.as_ref() {
            if Instant::now() < c.expires_at {
                println!("find_root_relay: using cached root relay: {} {}", c.id, c.address);
                return Ok(RootRelayInfo { id: c.id.clone(), address: c.address });
            }
        }

        // 2) Discover candidates
        let mut relays = (self.discover_roots)();
        if relays.is_empty() {
            return Err("no root relays discovered".to_string());
        }
        // Sort by id lexicographically (spec)
        relays.sort_by(|a, b| a.id.cmp(&b.id));

        // 3) Choose preferred and alternate per partitioning rule
        let (pref_idx, alt_idx) = self.select_indices(&relays, my_id);

        // 4) Try preferred then alternate via RelayCheck
        let order = [pref_idx, alt_idx];
        for &idx in &order {
            let cand = &relays[idx];
            if self.relay_check(&*cand.id, cand.address) {
                let info = RootRelayInfo { id: cand.id.clone(), address: cand.address };
                // Cache
                let expires = Instant::now() + self.cache_ttl;
                *self.cache.lock().map_err(|e| format!("cache lock poisoned: {}", e))? = Some(CachedRelay { id: info.id.clone(), address: info.address, expires_at: expires });
                return Ok(info);
            }
        }

        Err("no available root relay (preferred and alternate failed RelayCheck)".to_string())
    }

    fn select_indices(&self, relays: &[RootRelayInfo], my_id: &str) -> (usize, usize) {
        let n = relays.len();
        if n == 1 { return (0usize, 0usize); }
        let bucket = self.id_bucket_u32(my_id);
        let span = u64::from(u32::MAX) + 1; // 2^32
        let part_size = span / (n as u64);
        let idx = ((bucket as u64) / part_size) as usize % n;
        let alt = (idx + 1) % n;
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

    fn relay_check(&self, id: &str, addr: SocketAddr) -> bool {
        let nsk = NetworkSourceKey { inet_socket_address: Some(addr), relay_channel: None, relay_address: None };
        let req = json!({ "app": null, "type": "Check" });
        match self.api.send_message_to_network_with_response(&nsk, &id.to_string(), req, None) {
            Ok(resp) => {
                let is_ok = resp.get("type").and_then(|v| v.as_str()) == Some("CheckResponse")
                    && resp.get("available").and_then(|v| v.as_bool()).unwrap_or(false);
                is_ok
            }
            Err(_) => false,
        }
    }
}
