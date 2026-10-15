use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a relay that could not be reached is left out of relay selection (issue #122), so a
/// dead relay is not re-probed, at the cost of a connect timeout, on every selection pass.
pub const RELAY_PROBE_FAILURE_COOLDOWN: Duration = Duration::from_secs(60);

/// Clock used to age failures; injectable so tests need not sleep.
pub type ProbeClock = Arc<dyn Fn() -> Instant + Send + Sync>;

/// Negative cache of relays whose status probe failed to reach them (issue #122).
///
/// A relay that answered, even with a state other than Available, is not recorded: it is
/// reachable, and re-probing it is cheap. Held by the engine and shared by every relay finder it
/// creates, so a failure seen by one selection pass is honoured by the next.
pub struct RelayProbeFailures {
    cooldown: Duration,
    clock: ProbeClock,
    failed_at: Mutex<HashMap<String, Instant>>,
}

impl RelayProbeFailures {
    /// A cache using the default cooldown and the real clock.
    pub fn new() -> Self {
        Self::with_clock(RELAY_PROBE_FAILURE_COOLDOWN, Arc::new(Instant::now))
    }

    /// A cache with the given cooldown and clock.
    pub fn with_clock(cooldown: Duration, clock: ProbeClock) -> Self {
        Self {
            cooldown,
            clock,
            failed_at: Mutex::new(HashMap::new()),
        }
    }

    /// Record that a probe of `relay_id` could not reach it.
    pub fn record_failure(&self, relay_id: &str) {
        let now = (self.clock)();
        if let Ok(mut g) = self.failed_at.lock() {
            g.insert(relay_id.to_string(), now);
        }
    }

    /// Record that `relay_id` answered a probe, clearing any earlier failure.
    pub fn record_success(&self, relay_id: &str) {
        if let Ok(mut g) = self.failed_at.lock() {
            g.remove(relay_id);
        }
    }

    /// Whether `relay_id` failed a probe within the cooldown. Expired entries are dropped.
    pub fn is_cooling_down(&self, relay_id: &str) -> bool {
        let now = (self.clock)();
        let Ok(mut g) = self.failed_at.lock() else {
            return false;
        };
        match g.get(relay_id) {
            Some(failed) if now.saturating_duration_since(*failed) < self.cooldown => true,
            Some(_) => {
                g.remove(relay_id);
                false
            }
            None => false,
        }
    }
}

impl Default for RelayProbeFailures {
    fn default() -> Self {
        Self::new()
    }
}
