//! Honouring the global blocklist (issue #95): drop messages from a sender a moderator has blocked,
//! for App Store guideline 1.2.
//!
//! A moderator blocks an account by setting the `blocked_user` bit in its local state on the Bingle
//! DApp (#490). The client reads that bit for each sender and caches the answer:
//! - a block is cached for good, so a sender stays blocked even if it later clears its local state
//!   by opting out of the DApp;
//! - "not blocked" is cached for the refresh period (the handle cache's expiry, 600 seconds by
//!   default), so a new block takes effect within that time;
//! - a failed lookup is not cached, and the sender is not treated as blocked.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long "not blocked" is trusted before the chain is asked again, by default: the handle
/// cache's default expiry, so the block check runs on the same cadence as the handle lookup.
pub const DEFAULT_GLOBAL_BLOCK_REFRESH: Duration = Duration::from_secs(600);

/// What is known about one sender address.
#[derive(Debug, Clone, Copy)]
enum Standing {
    /// Blocked: kept for good.
    Blocked,
    /// Not blocked when last asked.
    Clear(Instant),
}

/// The senders known to be blocked or not, shared by the live and Mailbox receive paths.
#[derive(Debug)]
pub struct GlobalBlocklist {
    refresh: Duration,
    senders: Mutex<HashMap<String, Standing>>,
}

impl GlobalBlocklist {
    /// An empty blocklist that trusts "not blocked" for `refresh`.
    pub fn new(refresh: Duration) -> Self {
        Self {
            refresh,
            senders: Mutex::new(HashMap::new()),
        }
    }

    /// What the cache says about `address`: `Some(true)` once it has been seen blocked,
    /// `Some(false)` when it was seen not blocked within the refresh period, `None` otherwise.
    pub fn cached(&self, address: &str) -> Option<bool> {
        let senders = match self.senders.lock() {
            Ok(senders) => senders,
            Err(e) => {
                tracing::warn!("[GlobalBlocklist] could not lock the cache: {e}");
                return None;
            }
        };
        match senders.get(address) {
            Some(Standing::Blocked) => Some(true),
            Some(Standing::Clear(at)) if at.elapsed() < self.refresh => Some(false),
            _ => None,
        }
    }

    /// Record what the chain said about `address`. A block is kept for good, and is never
    /// overwritten by a later "not blocked".
    pub fn record(&self, address: &str, blocked: bool) {
        let Ok(mut senders) = self.senders.lock() else {
            tracing::warn!("[GlobalBlocklist] could not lock the cache to record {address}");
            return;
        };
        if blocked {
            senders.insert(address.to_string(), Standing::Blocked);
        } else if !matches!(senders.get(address), Some(Standing::Blocked)) {
            senders.insert(address.to_string(), Standing::Clear(Instant::now()));
        }
    }

    /// Whether `address` is blocked, asking `lookup` (the chain) only when the cache has no current
    /// answer. `lookup` returns `Ok(None)` for an account that isn't opted in, which is not blocked.
    /// A failed lookup is logged and treated as not blocked, without caching.
    pub fn is_blocked<F>(&self, address: &str, lookup: F) -> bool
    where
        F: FnOnce(&str) -> Result<Option<bool>, String>,
    {
        if let Some(blocked) = self.cached(address) {
            return blocked;
        }
        match lookup(address) {
            Ok(standing) => {
                let blocked = standing.unwrap_or(false);
                self.record(address, blocked);
                blocked
            }
            Err(e) => {
                tracing::warn!(
                    "[GlobalBlocklist] could not read whether {address} is blocked ({e}); not treating it as blocked"
                );
                false
            }
        }
    }
}
