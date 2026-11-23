use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Looks up whether an account may relay: `Some(true)` when its on-chain `allow_relay` flag is
/// set, `Some(false)` when it is not, `None` when the check can't be evaluated.
pub type RelayLookup = Arc<dyn Fn(&str) -> Option<bool> + Send + Sync>;

/// How long a confirmed relay stays trusted without a fresh chain read. Matches the relay set's
/// own permission refresh (`RELAY_PERMISSION_REFRESH_INTERVAL`), so a relay disabled on chain stops
/// being accepted as a message origin within the same interval as it leaves the relay set.
pub const RELAY_ORIGIN_ALLOWED_TTL: Duration = Duration::from_secs(300);

/// How long a "not a relay" answer is reused. Shorter than the allowed TTL, so a newly enabled
/// relay is accepted soon after its flag is set.
pub const RELAY_ORIGIN_DENIED_TTL: Duration = Duration::from_secs(60);

/// Upper bound on cached ids. A full cache first drops expired entries, then starts afresh, so a
/// stream of distinct senders can't grow it without bound.
pub const RELAY_ORIGIN_MAX_ENTRIES: usize = 4096;

/// Answers "is this sender a relay?" for the message-origin check, caching the on-chain answer so
/// relay traffic costs one chain read per sender per TTL rather than one per message.
///
/// An answer that can't be evaluated (`None` from the lookup) is treated as "not a relay" and is not
/// cached, so the check fails closed during a chain outage and recovers as soon as reads succeed.
pub struct RelayOriginCache {
    lookup: Mutex<RelayLookup>,
    entries: Mutex<HashMap<String, (bool, Instant)>>,
    allowed_ttl: Duration,
    denied_ttl: Duration,
}

impl RelayOriginCache {
    /// A cache over `lookup` with the default TTLs.
    pub fn new(lookup: RelayLookup) -> Self {
        Self::with_ttls(lookup, RELAY_ORIGIN_ALLOWED_TTL, RELAY_ORIGIN_DENIED_TTL)
    }

    /// A cache over `lookup` with explicit TTLs for allowed and denied answers.
    pub fn with_ttls(lookup: RelayLookup, allowed_ttl: Duration, denied_ttl: Duration) -> Self {
        Self {
            lookup: Mutex::new(lookup),
            entries: Mutex::new(HashMap::new()),
            allowed_ttl,
            denied_ttl,
        }
    }

    /// Whether `id` is a relay. Uses a cached answer while it is fresh, otherwise asks the lookup.
    /// The lookup runs without holding the cache lock, because it may read the chain.
    pub fn is_relay(&self, id: &str) -> bool {
        let now = Instant::now();
        if let Ok(entries) = self.entries.lock()
            && let Some((allowed, at)) = entries.get(id)
        {
            let ttl = if *allowed {
                self.allowed_ttl
            } else {
                self.denied_ttl
            };
            if now.duration_since(*at) < ttl {
                return *allowed;
            }
        }

        let lookup = match self.lookup.lock() {
            Ok(l) => l.clone(),
            Err(_) => {
                tracing::warn!("[RelayOriginCache::is_relay] lookup lock poisoned");
                return false;
            }
        };
        let Some(allowed) = lookup(id) else {
            return false;
        };

        if let Ok(mut entries) = self.entries.lock() {
            if entries.len() >= RELAY_ORIGIN_MAX_ENTRIES && !entries.contains_key(id) {
                let (allowed_ttl, denied_ttl) = (self.allowed_ttl, self.denied_ttl);
                entries.retain(|_, (a, at)| {
                    now.duration_since(*at) < if *a { allowed_ttl } else { denied_ttl }
                });
                if entries.len() >= RELAY_ORIGIN_MAX_ENTRIES {
                    entries.clear();
                }
            }
            entries.insert(id.to_string(), (allowed, now));
        }
        allowed
    }

    /// Number of cached ids, fresh or expired.
    pub fn len(&self) -> usize {
        self.entries.lock().map(|e| e.len()).unwrap_or(0)
    }

    /// Whether the cache holds no ids.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Test-only: replace the lookup and forget every cached answer.
    #[cfg(feature = "test-hooks")]
    pub fn set_lookup(&self, lookup: RelayLookup) {
        if let Ok(mut l) = self.lookup.lock() {
            *l = lookup;
        }
        if let Ok(mut e) = self.entries.lock() {
            e.clear();
        }
    }
}
